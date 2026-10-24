//! What a call was about: summary, decisions and action items, written by an LLM.

use std::fs;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const SUMMARY_JSON: &str = "summary.json";
pub const SUMMARY_MD: &str = "summary.md";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub summary: String,
    #[serde(default)]
    pub decisions: Vec<String>,
    #[serde(default)]
    pub action_items: Vec<ActionItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionItem {
    pub task: String,
    /// Who committed to it, if said.
    pub owner: Option<String>,
    /// When, as said in the call ("Monday", "before the 20th").
    pub due: Option<String>,
}

impl Summary {
    pub fn schema() -> Value {
        let nullable = json!({ "type": ["string", "null"] });
        json!({
            "type": "object",
            "properties": {
                "summary": { "type": "string" },
                "decisions": { "type": "array", "items": { "type": "string" } },
                "action_items": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": { "task": { "type": "string" }, "owner": nullable, "due": nullable },
                        "required": ["task", "owner", "due"]
                    }
                }
            },
            "required": ["summary", "decisions", "action_items"]
        })
    }

    pub fn load(dir: &Path) -> Result<Option<Summary>> {
        let path = dir.join(SUMMARY_JSON);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
    }

    pub fn to_markdown(&self, title: &str) -> String {
        let mut md = format!("# {title}\n\n{}\n", self.summary);
        if !self.decisions.is_empty() {
            md.push_str("\n## Decisiones\n\n");
            for decision in &self.decisions {
                md.push_str(&format!("- {decision}\n"));
            }
        }
        if !self.action_items.is_empty() {
            md.push_str("\n## Pendientes\n\n");
            for item in &self.action_items {
                md.push_str("- [ ] ");
                if let Some(owner) = &item.owner {
                    md.push_str(&format!("**{owner}**: "));
                }
                md.push_str(&item.task);
                if let Some(due) = &item.due {
                    md.push_str(&format!(" ({due})"));
                }
                md.push('\n');
            }
        }
        md
    }
}
