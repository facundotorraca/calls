//! Finds the external binaries the engines run.

use std::env;
use std::path::{Path, PathBuf};

/// Looks for `name` in, by priority:
/// 1. `$CALLS_SIDECAR_DIR`
/// 2. next to the running executable (where Tauri places bundled sidecars)
/// 3. `apps/desktop/src-tauri/binaries`, in debug builds (sidecars built by `scripts/build-sidecars.sh`)
/// 4. Homebrew and `$PATH` (GUI apps on macOS do not inherit the shell's `$PATH`)
pub fn locate(name: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    dirs.extend(env::var_os("CALLS_SIDECAR_DIR").map(PathBuf::from));
    dirs.extend(env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)));
    if cfg!(debug_assertions) {
        dirs.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/desktop/src-tauri/binaries"));
    }
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    dirs.extend(env::var_os("PATH").map(|p| env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default());

    let suffixed = format!("{name}-{}", target_triple());
    dirs.iter().flat_map(|dir| [dir.join(name), dir.join(&suffixed)]).find(|path| path.is_file())
}

/// Tauri names sidecars `<name>-<target triple>` in the source tree.
fn target_triple() -> String {
    let arch = env::consts::ARCH;
    match env::consts::OS {
        "macos" => format!("{arch}-apple-darwin"),
        "windows" => format!("{arch}-pc-windows-msvc"),
        _ => format!("{arch}-unknown-linux-gnu"),
    }
}
