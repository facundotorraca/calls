//! Local LLM served by [Ollama](https://ollama.com), over its HTTP API.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use calls_core::engines::Llm;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::{Value, json};

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
        #[derive(Deserialize)]
        struct Tags {
            models: Vec<Model>,
        }
        #[derive(Deserialize)]
        struct Model {
            name: String,
        }
        let tags: Tags = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .timeout(Duration::from_secs(3))
            .send()
            .context("Ollama is not running (open the Ollama app)")?
            .json()?;
        if !tags.models.iter().any(|m| m.name == self.model) {
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
        }
        #[derive(Deserialize)]
        struct Message {
            content: String,
        }
        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .send()
            .context("Ollama is not running (open the Ollama app)")?;
        if !response.status().is_success() {
            bail!("Ollama: {} {}", response.status(), response.text().unwrap_or_default());
        }
        Ok(response.json::<Response>()?.message.content)
    }
}

impl Llm for Ollama {
    fn json(&self, system: &str, prompt: &str, schema: &Value) -> Result<Value> {
        // Fast path: no thinking, schema described in the prompt. (Ollama ignores a schema
        // `format` when thinking is off, so ask for plain JSON.)
        let described = format!("{system}\n\nAnswer only with JSON matching this JSON Schema:\n{schema}");
        if let Ok(answer) = self.chat(&described, prompt, json!("json"), false)
            && let Ok(value) = serde_json::from_str::<Value>(&answer)
            && has_required_fields(&value, schema)
        {
            return Ok(value);
        }
        // Strict path: slower, but Ollama enforces the schema.
        let answer = self.chat(system, prompt, schema.clone(), true)?;
        serde_json::from_str(&answer).context("the model did not answer with JSON")
    }
}

fn has_required_fields(value: &Value, schema: &Value) -> bool {
    let required = schema["required"].as_array().map(Vec::as_slice).unwrap_or_default();
    required.iter().filter_map(Value::as_str).all(|field| value.get(field).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_fields() {
        let schema = json!({ "required": ["summary", "decisions"] });
        assert!(has_required_fields(&json!({ "summary": "x", "decisions": [] }), &schema));
        assert!(!has_required_fields(&json!({ "summary": "x" }), &schema));
        assert!(!has_required_fields(&json!("just text"), &schema));
    }
}
