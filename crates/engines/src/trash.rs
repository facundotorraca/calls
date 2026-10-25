//! The macOS Trash, through the system's own API (`NSFileManager`), run by `osascript`: the
//! Trash folder itself is off limits to apps, and moving things there by hand would lose
//! "Put Back" in the Finder.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

const SCRIPT: &str = r#"
function run(argv) {
    ObjC.import("Foundation");
    const error = $();
    const url = $.NSURL.fileURLWithPath(argv[0]);
    if (!$.NSFileManager.defaultManager.trashItemAtURLResultingItemURLError(url, null, error)) {
        throw new Error(ObjC.unwrap(error.localizedDescription));
    }
}
"#;

/// Moves `path` (a file or a folder) to the Trash.
pub fn move_to_trash(path: &Path) -> Result<()> {
    let output = Command::new("/usr/bin/osascript")
        .args(["-l", "JavaScript", "-e", SCRIPT])
        .arg(path)
        .output()
        .context("could not run osascript")?;
    if !output.status.success() {
        bail!("could not move {} to the Trash: {}", path.display(), String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Leaves a folder in the real Trash: `cargo test -- --ignored` to run it.
    #[test]
    #[ignore]
    fn trashes_a_folder() {
        let dir = std::env::temp_dir().join(format!("calls trash test {}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "x").unwrap();
        move_to_trash(&dir).unwrap();
        assert!(!dir.exists());
        assert!(move_to_trash(&dir).is_err());
    }
}
