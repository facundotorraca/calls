//! Local LLM served by [Ollama](https://ollama.com), over its HTTP API.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use calls_core::engines::{Llm, MemoryShortage};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const NOT_RUNNING: &str = "Ollama is not running (open the Ollama app)";
const GB: u64 = 1_000_000_000;

pub struct Ollama {
    base_url: String,
    model: String,
    client: Client,
}

impl Ollama {
    pub fn new(base_url: &str, model: &str) -> Result<Ollama> {
        // Long calls take a while to read; generous timeout.
        let client = Client::builder().timeout(Duration::from_secs(15 * 60)).build()?;
        Ok(Ollama { base_url: base_url.trim_end_matches('/').into(), model: model.into(), client })
    }

    /// Whether Ollama is running and has the model.
    pub fn check(&self) -> Result<()> {
        if !installed(&self.base_url)?.iter().any(|m| m.name == self.model) {
            bail!("Ollama does not have {} (ollama pull {})", self.model, self.model);
        }
        Ok(())
    }

    fn chat(&self, system: &str, prompt: &str, format: Value, think: bool) -> Result<String> {
        // Ollama's default context is small; size it to the prompt (~3 chars per token).
        let tokens = (system.len() + prompt.len()) / 3 + 4096;
        let num_ctx = tokens.div_ceil(8192).clamp(1, 16) * 8192;
        let body = json!({
            "model": self.model,
            "stream": false,
            // Summaries happen once per call: free the model's memory (GBs) soon after.
            "keep_alive": "30s",
            "think": think,
            "format": format,
            "options": { "temperature": 0, "num_ctx": num_ctx },
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": prompt },
            ],
        });

        #[derive(Deserialize)]
        struct Response {
            message: Message,
            #[serde(default)]
            done_reason: String,
        }
        #[derive(Deserialize)]
        struct Message {
            content: String,
        }
        let response =
            self.client.post(format!("{}/api/chat", self.base_url)).json(&body).send().context(NOT_RUNNING)?;
        if !response.status().is_success() {
            bail!("Ollama: {} {}", response.status(), response.text().unwrap_or_default());
        }
        let response: Response = response.json()?;
        // A thinking model can spend the whole context thinking and never answer.
        if response.message.content.trim().is_empty() && response.done_reason == "length" {
            bail!("the model ran out of context before answering (it thought for too long)");
        }
        Ok(response.message.content)
    }
}

impl Llm for Ollama {
    fn json(&self, system: &str, prompt: &str, schema: &Value) -> Result<Value> {
        // Fast path: no thinking, schema described in the prompt. (Ollama ignores a schema
        // `format` when thinking is off, so ask for plain JSON.)
        let described = format!("{system}\n\nAnswer only with JSON matching this JSON Schema:\n{schema}");
        if let Ok(answer) = self.chat(&described, prompt, json!("json"), false)
            && let Ok(value) = parse_json(&answer)
            && has_required_fields(&value, schema)
        {
            return Ok(value);
        }
        // Strict path: slower, but Ollama enforces the schema.
        let answer = self.chat(system, prompt, schema.clone(), true)?;
        parse_json(&answer).context("the model did not answer with JSON")
    }

    fn memory_shortage(&self) -> Option<MemoryShortage> {
        // Only this computer's memory can be told, and a loaded model needs nothing more.
        if !is_local(&self.base_url) || self.is_loaded().unwrap_or(true) {
            return None;
        }
        let size = installed(&self.base_url).ok()?.into_iter().find(|m| m.name == self.model)?.size;
        // The weights, plus room for the context and Ollama itself.
        let needed = size + size / 10 + GB;
        let available = crate::memory::available()?;
        (available < needed).then_some(MemoryShortage { needed, available })
    }
}

impl Ollama {
    /// Whether Ollama has the model in memory now.
    fn is_loaded(&self) -> Result<bool> {
        #[derive(Deserialize)]
        struct Running {
            models: Vec<Model>,
        }
        #[derive(Deserialize)]
        struct Model {
            name: String,
        }
        let running: Running = self
            .client
            .get(endpoint(&self.base_url, "ps"))
            .timeout(Duration::from_secs(3))
            .send()?
            .error_for_status()?
            .json()?;
        Ok(running.models.iter().any(|m| m.name == self.model))
    }
}

fn is_local(base_url: &str) -> bool {
    let host = base_url.split("://").nth(1).unwrap_or(base_url);
    ["localhost", "127.0.0.1", "[::1]"].iter().any(|local| host.starts_with(local))
}

/// A model Ollama has downloaded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledModel {
    pub name: String,
    /// Bytes on disk.
    pub size: u64,
    #[serde(default)]
    pub details: ModelDetails,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelDetails {
    /// e.g. `35B`.
    #[serde(default)]
    pub parameter_size: String,
    /// e.g. `Q4_K_M`.
    #[serde(default)]
    pub quantization_level: String,
}

/// The models Ollama at `base_url` has, as `ollama list` shows them.
pub fn installed(base_url: &str) -> Result<Vec<InstalledModel>> {
    #[derive(Deserialize)]
    struct Tags {
        models: Vec<InstalledModel>,
    }
    let tags: Tags = Client::new()
        .get(endpoint(base_url, "tags"))
        .timeout(Duration::from_secs(3))
        .send()
        .context(NOT_RUNNING)?
        .error_for_status()?
        .json()?;
    Ok(tags.models)
}

/// Downloads a model, like `ollama pull`. Ollama keeps what it got if cancelled, so pulling
/// the same model again resumes.
pub fn pull(base_url: &str, model: &str, progress: &dyn Fn(f32), cancelled: &AtomicBool) -> Result<()> {
    #[derive(Deserialize)]
    struct Line {
        #[serde(default)]
        error: Option<String>,
        #[serde(default)]
        digest: Option<String>,
        #[serde(default)]
        total: Option<u64>,
        #[serde(default)]
        completed: Option<u64>,
    }
    let client = Client::builder().connect_timeout(Duration::from_secs(5)).timeout(None).build()?;
    let response = client
        .post(endpoint(base_url, "pull"))
        .json(&json!({ "model": model, "stream": true }))
        .send()
        .context(NOT_RUNNING)?;
    if !response.status().is_success() {
        bail!("Ollama: {} {}", response.status(), response.text().unwrap_or_default());
    }

    // A model is several layers, each reported on its own; progress is over all of them.
    let mut layers: HashMap<String, (u64, u64)> = HashMap::new();
    let mut reported = 0.0f32;
    for line in BufReader::new(response).lines() {
        if cancelled.load(Ordering::Relaxed) {
            bail!("cancelled");
        }
        let line: Line = serde_json::from_str(&line?)?;
        if let Some(error) = line.error {
            bail!("Ollama: {error}");
        }
        if let (Some(digest), Some(total)) = (line.digest, line.total) {
            layers.insert(digest, (line.completed.unwrap_or(0), total));
            let (done, total) = layers.values().fold((0, 0), |(d, t), (ld, lt)| (d + ld, t + lt));
            let fraction = if total > 0 { done as f32 / total as f32 } else { 0.0 };
            // Ollama reports many times a second; every half percent is plenty.
            if (fraction - reported).abs() >= 0.005 {
                reported = fraction;
                progress(fraction);
            }
        }
    }
    Ok(())
}

/// Deletes a downloaded model, like `ollama rm`.
pub fn delete(base_url: &str, model: &str) -> Result<()> {
    let response = Client::new()
        .delete(endpoint(base_url, "delete"))
        .json(&json!({ "model": model }))
        .send()
        .context(NOT_RUNNING)?;
    if !response.status().is_success() {
        bail!("Ollama: {} {}", response.status(), response.text().unwrap_or_default());
    }
    Ok(())
}

fn endpoint(base_url: &str, path: &str) -> String {
    format!("{}/api/{path}", base_url.trim_end_matches('/'))
}

/// The JSON in an answer. Some models wrap it in a Markdown code block even when asked for
/// JSON only, so whatever surrounds the outermost object is dropped.
fn parse_json(answer: &str) -> serde_json::Result<Value> {
    let json = match (answer.find('{'), answer.rfind('}')) {
        (Some(start), Some(end)) if start < end => &answer[start..=end],
        _ => answer,
    };
    serde_json::from_str(json)
}

fn has_required_fields(value: &Value, schema: &Value) -> bool {
    let required = schema["required"].as_array().map(Vec::as_slice).unwrap_or_default();
    required.iter().filter_map(Value::as_str).all(|field| value.get(field).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_this_computer_is_local() {
        assert!(is_local("http://localhost:11434"));
        assert!(is_local("http://127.0.0.1:11434/"));
        assert!(!is_local("https://gpu-box.lan:11434"));
    }

    #[test]
    fn json_in_a_code_block() {
        assert_eq!(parse_json("```json\n{\"a\": 1}\n```").unwrap(), json!({ "a": 1 }));
        assert_eq!(parse_json("{\"a\": {}}").unwrap(), json!({ "a": {} }));
        assert!(parse_json("").is_err());
    }

    #[test]
    fn required_fields() {
        let schema = json!({ "required": ["summary", "decisions"] });
        assert!(has_required_fields(&json!({ "summary": "x", "decisions": [] }), &schema));
        assert!(!has_required_fields(&json!({ "summary": "x" }), &schema));
        assert!(!has_required_fields(&json!("just text"), &schema));
    }
}
