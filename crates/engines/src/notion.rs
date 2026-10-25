//! Calls as Notion pages (see `calls_core::notion`), through Notion's official HTTP API and an internal integration
//! token (free; the user creates it and shares the parent pages with it).

use std::collections::HashSet;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use calls_core::notion::{NotionPage, Synced, page_id};
use calls_core::summary::Summary;
use calls_core::transcript::format_timestamp;
use calls_core::{Call, Transcript};
use chrono::Local;
use reqwest::Method;
use reqwest::blocking::Client;
use serde_json::{Value, json};

use crate::keychain;

const API: &str = "https://api.notion.com/v1";
const VERSION: &str = "2022-06-28";
/// The Keychain account the token is stored under.
const KEYCHAIN_ACCOUNT: &str = "notion";
/// Most blocks Notion takes in one request, and most characters in one text.
const MAX_BLOCKS: usize = 100;
const MAX_TEXT: usize = 2000;

pub struct Notion {
    client: Client,
    token: String,
}

impl Notion {
    pub fn new(token: &str) -> Result<Notion> {
        let client = Client::builder().timeout(Duration::from_secs(60)).build()?;
        Ok(Notion { client, token: token.trim().into() })
    }

    /// With the token saved by [`Notion::save_token`].
    pub fn from_keychain() -> Result<Notion> {
        match keychain::get(KEYCHAIN_ACCOUNT)? {
            Some(token) => Notion::new(&token),
            None => bail!("Notion no está conectado: pegá el token en Ajustes → Notion"),
        }
    }

    pub fn save_token(&self) -> Result<()> {
        keychain::set(KEYCHAIN_ACCOUNT, &self.token)
    }

    pub fn forget_token() -> Result<()> {
        keychain::delete(KEYCHAIN_ACCOUNT)
    }

    /// The workspace the token belongs to: checks the token works.
    pub fn workspace(&self) -> Result<String> {
        let me = self.request(Method::GET, "users/me", None)?;
        Ok(me["bot"]["workspace_name"].as_str().unwrap_or("Notion").to_string())
    }

    /// The page a link points to, if the integration can see it.
    pub fn page(&self, link: &str) -> Result<NotionPage> {
        let Some(id) = page_id(link) else { bail!("eso no parece un link de una página de Notion") };
        let page = self.request(Method::GET, &format!("pages/{id}"), None).map_err(|e| {
            if e.to_string().contains("object_not_found") {
                anyhow::anyhow!(
                    "Notion no deja ver esa página: abrila en Notion y, en ··· → Conexiones, agregá tu integración"
                )
            } else {
                e
            }
        })?;
        Ok(NotionPage { id: page["id"].as_str().unwrap_or(&id).into(), title: title(&page), url: url(&page) })
    }

    /// Writes the call as a page under `parent_id`: the page synced before (`previous`) if it
    /// is still there and under the same parent, a new one otherwise. Tasks ticked in Notion
    /// stay ticked.
    pub fn sync(
        &self,
        call: &Call,
        transcript: Option<&Transcript>,
        summary: Option<&Summary>,
        parent_id: &str,
        previous: Option<&Synced>,
    ) -> Result<Synced> {
        let reusable = match previous {
            Some(p) if p.parent_id == parent_id && self.is_live(&p.page_id)? => Some(p),
            Some(p) => {
                // Its parent changed: the old page goes to Notion's trash.
                let _ = self.request(Method::PATCH, &format!("pages/{}", p.page_id), Some(json!({ "archived": true })));
                None
            }
            None => None,
        };

        let (page_id, url, ticked) = match reusable {
            Some(p) => {
                self.set_title(&p.page_id, &call.title)?;
                let ticked = self.clear(&p.page_id)?;
                (p.page_id.clone(), p.url.clone(), ticked)
            }
            None => {
                let page = self.create(parent_id, &call.title)?;
                (id(&page)?, url(&page), HashSet::new())
            }
        };

        if let Err(e) = self.append(&page_id, blocks(call, transcript, summary, &ticked)) {
            // A page made just now and left half written is of no use: next time starts over.
            if reusable.is_none() {
                let _ = self.request(Method::PATCH, &format!("pages/{page_id}"), Some(json!({ "archived": true })));
            }
            return Err(e);
        }
        Ok(Synced { page_id, url, parent_id: parent_id.into(), at: Local::now() })
    }

    /// An empty page titled `title` under `parent_id`.
    fn create(&self, parent_id: &str, title: &str) -> Result<Value> {
        self.request(
            Method::POST,
            "pages",
            Some(json!({
                "parent": { "page_id": parent_id },
                "properties": { "title": { "title": text(title) } },
            })),
        )
    }

    fn set_title(&self, page_id: &str, title: &str) -> Result<()> {
        self.request(
            Method::PATCH,
            &format!("pages/{page_id}"),
            Some(json!({ "properties": { "title": { "title": text(title) } } })),
        )?;
        Ok(())
    }

    fn is_live(&self, page_id: &str) -> Result<bool> {
        match self.request(Method::GET, &format!("pages/{page_id}"), None) {
            Ok(page) => {
                Ok(!page["archived"].as_bool().unwrap_or(false) && !page["in_trash"].as_bool().unwrap_or(false))
            }
            Err(e) if e.to_string().contains("object_not_found") => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Deletes the page's content, returning the tasks that were ticked.
    fn clear(&self, page_id: &str) -> Result<HashSet<String>> {
        let mut ticked = HashSet::new();
        let mut ids = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut path = format!("blocks/{page_id}/children?page_size=100");
            if let Some(c) = &cursor {
                path.push_str(&format!("&start_cursor={c}"));
            }
            let page = self.request(Method::GET, &path, None)?;
            for block in page["results"].as_array().into_iter().flatten() {
                if block["type"] == "to_do" && block["to_do"]["checked"] == true {
                    ticked.insert(plain_text(&block["to_do"]["rich_text"]));
                }
                if let Some(id) = block["id"].as_str() {
                    ids.push(id.to_string());
                }
            }
            match page["next_cursor"].as_str() {
                Some(next) if page["has_more"] == true => cursor = Some(next.into()),
                _ => break,
            }
        }
        // A deleted block takes its children along: the transcript is one block.
        for id in ids {
            self.request(Method::DELETE, &format!("blocks/{id}"), None)?;
        }
        Ok(ticked)
    }

    /// Appends blocks to a page, in as many requests as Notion's limits need.
    fn append(&self, page_id: &str, blocks: Vec<Value>) -> Result<()> {
        for chunk in blocks.chunks(MAX_BLOCKS) {
            let (chunk, later) = split_children(chunk);
            let created =
                self.request(Method::PATCH, &format!("blocks/{page_id}/children"), Some(json!({ "children": chunk })))?;
            for (i, children) in later {
                let id = created["results"][i]["id"].as_str().context("Notion did not return the block id")?;
                for part in children.chunks(MAX_BLOCKS) {
                    self.request(Method::PATCH, &format!("blocks/{id}/children"), Some(json!({ "children": part })))?;
                }
            }
        }
        Ok(())
    }

    /// One API call. Waits and retries when Notion asks to slow down.
    fn request(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
        for _ in 0..5 {
            let mut request = self
                .client
                .request(method.clone(), format!("{API}/{path}"))
                .bearer_auth(&self.token)
                .header("Notion-Version", VERSION);
            if let Some(body) = &body {
                request = request.json(body);
            }
            let response = request.send().context("could not reach Notion")?;
            let status = response.status();
            if status.as_u16() == 429 {
                let wait = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1);
                thread::sleep(Duration::from_secs(wait));
                continue;
            }
            let value: Value = response.json().unwrap_or(Value::Null);
            if !status.is_success() {
                let code = value["code"].as_str().unwrap_or_default();
                if code == "unauthorized" {
                    bail!("Notion no acepta el token: revisalo en Ajustes → Notion");
                }
                bail!("Notion: {} ({code})", value["message"].as_str().unwrap_or(status.as_str()));
            }
            return Ok(value);
        }
        bail!("Notion is busy: try again in a minute")
    }
}

/// Blocks Notion takes in one request, and the children left out of them (by the index of
/// their parent), to add once their parent exists.
fn split_children(blocks: &[Value]) -> (Vec<Value>, Vec<(usize, Vec<Value>)>) {
    let mut later = Vec::new();
    let blocks = blocks
        .iter()
        .enumerate()
        .map(|(i, block)| {
            let mut block = block.clone();
            let kind = block["type"].as_str().unwrap_or_default().to_string();
            // `get_mut`, not indexing: indexing a missing key adds it as null, which Notion rejects.
            if let Some(children) =
                block.get_mut(&kind).and_then(|b| b.get_mut("children")).and_then(Value::as_array_mut)
                && children.len() > MAX_BLOCKS
            {
                later.push((i, children.split_off(MAX_BLOCKS)));
            }
            block
        })
        .collect();
    (blocks, later)
}

/// The page's content: what was said and agreed first, the whole transcript folded away.
fn blocks(
    call: &Call,
    transcript: Option<&Transcript>,
    summary: Option<&Summary>,
    ticked: &HashSet<String>,
) -> Vec<Value> {
    let mut blocks = Vec::new();

    let mut meta = vec![call.created_at.format("%d/%m/%Y %H:%M").to_string()];
    if let Some(ms) = call.duration_ms {
        meta.push(format!("{} min", ms.div_ceil(60_000)));
    }
    if let Some(folder) = &call.folder {
        meta.push(folder.clone());
    }
    if let Some(t) = transcript {
        let people = calls_core::steps::speakers(t);
        if !people.is_empty() {
            meta.push(people.join(", "));
        }
    }
    blocks.push(json!({ "type": "paragraph", "paragraph": { "rich_text": styled(&meta.join(" · "), "gray", false) } }));

    if let Some(summary) = summary {
        blocks.push(heading("Resumen"));
        for paragraph in summary.summary.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
            blocks.push(json!({ "type": "paragraph", "paragraph": { "rich_text": text(paragraph) } }));
        }
        if !summary.decisions.is_empty() {
            blocks.push(heading("Decisiones"));
            for decision in &summary.decisions {
                blocks.push(
                    json!({ "type": "bulleted_list_item", "bulleted_list_item": { "rich_text": text(decision) } }),
                );
            }
        }
        if !summary.action_items.is_empty() {
            blocks.push(heading("Tareas"));
            for item in &summary.action_items {
                let mut task = item.task.clone();
                if let Some(owner) = &item.owner {
                    task.push_str(&format!(" — {owner}"));
                }
                if let Some(due) = &item.due {
                    task.push_str(&format!(" ({due})"));
                }
                let checked = ticked.contains(&task);
                blocks.push(json!({ "type": "to_do", "to_do": { "rich_text": text(&task), "checked": checked } }));
            }
        }
    }

    if let Some(transcript) = transcript.filter(|t| !t.segments.is_empty()) {
        let mut turns: Vec<Value> = Vec::new();
        let mut current: Option<(&Option<String>, u64, String)> = None;
        let mut flush = |turn: (&Option<String>, u64, String)| {
            let (speaker, start, said) = turn;
            let mut rich = styled(&format!("{} ", format_timestamp(start)), "gray", false);
            if let Some(speaker) = speaker {
                rich.extend(styled(&format!("{speaker}: "), "default", true));
            }
            rich.extend(text(&said));
            turns.push(json!({ "type": "paragraph", "paragraph": { "rich_text": rich } }));
        };
        for segment in &transcript.segments {
            match &mut current {
                Some((speaker, _, said)) if *speaker == &segment.speaker => {
                    said.push(' ');
                    said.push_str(segment.text.trim());
                }
                _ => {
                    if let Some(turn) = current.take() {
                        flush(turn);
                    }
                    current = Some((&segment.speaker, segment.start_ms, segment.text.trim().to_string()));
                }
            }
        }
        if let Some(turn) = current.take() {
            flush(turn);
        }
        blocks.push(json!({
            "type": "heading_2",
            "heading_2": { "rich_text": text("Transcripción"), "is_toggleable": true, "children": turns },
        }));
    }
    blocks
}

fn heading(title: &str) -> Value {
    json!({ "type": "heading_2", "heading_2": { "rich_text": text(title) } })
}

fn text(content: &str) -> Vec<Value> {
    styled(content, "default", false)
}

/// Rich text, split in parts Notion accepts.
fn styled(content: &str, color: &str, bold: bool) -> Vec<Value> {
    let chars: Vec<char> = content.chars().collect();
    chars
        .chunks(MAX_TEXT)
        .map(|part| {
            json!({
                "type": "text",
                "text": { "content": part.iter().collect::<String>() },
                "annotations": { "color": color, "bold": bold },
            })
        })
        .collect()
}

fn plain_text(rich_text: &Value) -> String {
    rich_text.as_array().into_iter().flatten().filter_map(|t| t["plain_text"].as_str()).collect()
}

fn title(page: &Value) -> String {
    let properties = page["properties"].as_object();
    let title =
        properties.into_iter().flatten().find(|(_, p)| p["type"] == "title").map(|(_, p)| plain_text(&p["title"]));
    title.filter(|t| !t.is_empty()).unwrap_or_else(|| "Sin título".into())
}

fn id(page: &Value) -> Result<String> {
    Ok(page["id"].as_str().context("Notion did not return the page id")?.to_string())
}

fn url(page: &Value) -> String {
    page["url"].as_str().unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
    use calls_core::Segment;
    use calls_core::summary::ActionItem;

    use super::*;

    #[test]
    fn page_content() {
        let call: Call = serde_json::from_value(json!({
            "id": "x", "title": "Kickoff", "folder": "Acme", "created_at": "2026-10-23T10:02:25-03:00",
            "duration_ms": 125000, "tracks": [], "status": { "state": "ready" }, "completed_steps": {}
        }))
        .unwrap();
        let seg = |start_ms, speaker: &str, text: &str| Segment {
            start_ms,
            end_ms: start_ms + 1000,
            text: text.into(),
            speaker: Some(speaker.into()),
        };
        let transcript = Transcript {
            language: "es".into(),
            segments: vec![seg(0, "Ana", "Hola."), seg(1000, "Ana", "¿Arrancamos?"), seg(2000, "Yo", "Dale.")],
        };
        let summary = Summary {
            summary: "Primero.\n\nSegundo.".into(),
            decisions: vec!["Lanzar el lunes".into()],
            action_items: vec![
                ActionItem { task: "Mandar la propuesta".into(), owner: Some("Ana".into()), due: None },
                ActionItem { task: "Revisar".into(), owner: None, due: Some("viernes".into()) },
            ],
        };
        let ticked = HashSet::from(["Mandar la propuesta — Ana".to_string()]);
        let blocks = blocks(&call, Some(&transcript), Some(&summary), &ticked);

        let types: Vec<&str> = blocks.iter().map(|b| b["type"].as_str().unwrap()).collect();
        assert_eq!(
            types,
            [
                "paragraph",
                "heading_2",
                "paragraph",
                "paragraph",
                "heading_2",
                "bulleted_list_item",
                "heading_2",
                "to_do",
                "to_do",
                "heading_2"
            ]
        );
        assert_eq!(plain(&blocks[0]["paragraph"]["rich_text"]), "23/10/2026 10:02 · 3 min · Acme · Ana, Yo");
        assert_eq!(blocks[7]["to_do"]["checked"], true);
        assert_eq!(plain(&blocks[8]["to_do"]["rich_text"]), "Revisar (viernes)");
        assert_eq!(blocks[8]["to_do"]["checked"], false);

        let turns = blocks[9]["heading_2"]["children"].as_array().unwrap();
        assert_eq!(turns.len(), 2);
        assert_eq!(plain(&turns[0]["paragraph"]["rich_text"]), "00:00 Ana: Hola. ¿Arrancamos?");
    }

    #[test]
    fn long_children_wait_and_others_are_untouched() {
        let paragraph = json!({ "type": "paragraph", "paragraph": { "rich_text": [] } });
        let toggle = json!({
            "type": "heading_2",
            "heading_2": { "rich_text": [], "is_toggleable": true, "children": vec![paragraph.clone(); 250] },
        });
        let (blocks, later) = split_children(&[paragraph.clone(), toggle]);
        assert_eq!(blocks[0], paragraph);
        assert_eq!(blocks[1]["heading_2"]["children"].as_array().unwrap().len(), MAX_BLOCKS);
        assert_eq!(later.len(), 1);
        assert_eq!((later[0].0, later[0].1.len()), (1, 150));
    }

    #[test]
    fn long_texts_are_split() {
        assert_eq!(text(&"a".repeat(4500)).len(), 3);
    }

    fn plain(rich_text: &Value) -> String {
        rich_text.as_array().unwrap().iter().map(|t| t["text"]["content"].as_str().unwrap()).collect()
    }
}
