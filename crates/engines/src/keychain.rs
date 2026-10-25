//! Secrets in the macOS Keychain, through the system's `security` tool.

use std::process::Command;

use anyhow::{Context, Result, bail};

const SERVICE: &str = "calls";
/// What `security` exits with when there is no such item.
const NOT_FOUND: i32 = 44;

/// The secret stored as `account`, if there is one.
pub fn get(account: &str) -> Result<Option<String>> {
    let output = security(&["find-generic-password", "-s", SERVICE, "-a", account, "-w"])?;
    match output.status.code() {
        Some(0) => Ok(Some(String::from_utf8_lossy(&output.stdout).trim().to_string())),
        Some(NOT_FOUND) => Ok(None),
        _ => bail!("could not read the Keychain: {}", String::from_utf8_lossy(&output.stderr).trim()),
    }
}

/// Stores (or replaces) the secret for `account`.
pub fn set(account: &str, secret: &str) -> Result<()> {
    let output = security(&["add-generic-password", "-U", "-s", SERVICE, "-a", account, "-w", secret])?;
    if !output.status.success() {
        bail!("could not write to the Keychain: {}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(())
}

pub fn delete(account: &str) -> Result<()> {
    let output = security(&["delete-generic-password", "-s", SERVICE, "-a", account])?;
    match output.status.code() {
        Some(0 | NOT_FOUND) => Ok(()),
        _ => bail!("could not delete from the Keychain: {}", String::from_utf8_lossy(&output.stderr).trim()),
    }
}

fn security(args: &[&str]) -> Result<std::process::Output> {
    Command::new("/usr/bin/security").args(args).output().context("could not run security")
}
