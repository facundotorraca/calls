//! Exporting calls to Notion: each call becomes a page, named after it, under a parent page
//! chosen at three levels, from the most general to the most specific: the default one (in
//! the settings), the call's folder's, the call's own. The page is created once and
//! updated in place on every sync.

use std::fs;
use std::path::Path;

use anyhow::Result;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

/// A call's Notion state, in its folder.
pub const NOTION_FILE: &str = "notion.json";

/// A Notion page, as the user picked it (pasting its link).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotionPage {
    pub id: String,
    pub title: String,
    pub url: String,
}

/// Which level a call's parent page was set at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Default,
    Folder,
    Call,
}

/// Where a call's page goes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Destination {
    pub page: NotionPage,
    pub level: Level,
}

/// The most specific parent page set for a call, if any is.
pub fn destination(
    call: Option<&NotionPage>,
    folder: Option<&NotionPage>,
    default: Option<&NotionPage>,
) -> Option<Destination> {
    [(call, Level::Call), (folder, Level::Folder), (default, Level::Default)]
        .into_iter()
        .find_map(|(page, level)| page.map(|page| Destination { page: page.clone(), level }))
}

/// The page a call was last synced to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Synced {
    pub page_id: String,
    pub url: String,
    /// The parent it was created under: syncing to another one makes a new page.
    pub parent_id: String,
    pub at: DateTime<Local>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallNotion {
    /// The parent page for this call only, over its folder's and the default one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<NotionPage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synced: Option<Synced>,
}

impl CallNotion {
    pub fn load(dir: &Path) -> CallNotion {
        fs::read_to_string(dir.join(NOTION_FILE))
            .ok()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        fs::write(dir.join(NOTION_FILE), serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

/// The page id in a Notion link (`https://www.notion.so/Title-<32 hex>?...`), or in an id
/// pasted as is, with or without dashes. Formatted with dashes, as the API returns them.
pub fn page_id(link: &str) -> Option<String> {
    let path = link.trim().split(['?', '#']).next()?;
    // The id is at the end: the last 32 hex digits, dashes aside.
    let digits: Vec<char> = path.chars().rev().filter(|c| *c != '-').take_while(char::is_ascii_hexdigit).collect();
    if digits.len() < 32 {
        return None;
    }
    let id: String = digits[..32].iter().rev().collect::<String>().to_lowercase();
    Some(format!("{}-{}-{}-{}-{}", &id[0..8], &id[8..12], &id[12..16], &id[16..20], &id[20..32]))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "1a2b3c4d-5e6f-7a8b-9c0d-1e2f3a4b5c6d";

    #[test]
    fn the_most_specific_parent_wins() {
        let page = |id: &str| NotionPage { id: id.into(), title: id.into(), url: String::new() };
        let (call, folder, default) = (page("call"), page("folder"), page("default"));
        assert_eq!(destination(Some(&call), Some(&folder), Some(&default)).unwrap().level, Level::Call);
        assert_eq!(destination(None, Some(&folder), Some(&default)).unwrap().page, folder);
        assert_eq!(destination(None, None, Some(&default)).unwrap().level, Level::Default);
        assert_eq!(destination(None, None, None), None);
    }

    #[test]
    fn page_ids_from_links() {
        assert_eq!(page_id("https://www.notion.so/me/Clientes-1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d?pvs=4").unwrap(), ID);
        assert_eq!(page_id("https://notion.so/1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d").unwrap(), ID);
        // A title ending in hex letters ("Cafe") does not shift the id.
        assert_eq!(page_id("https://www.notion.so/Cafe-1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d").unwrap(), ID);
        assert_eq!(page_id(" 1A2B3C4D-5E6F-7A8B-9C0D-1E2F3A4B5C6D ").unwrap(), ID);
        assert_eq!(page_id("https://www.notion.so/me/Clientes"), None);
        assert_eq!(page_id(""), None);
    }
}
