//! How much memory the computer has free.

use std::process::Command;

/// Bytes macOS counts as available (free, or taken by what it can drop or compress without
/// trouble): the "free percentage" `memory_pressure` reports. `None` if it cannot be told.
pub fn available() -> Option<u64> {
    let percent: u64 = sysctl("kern.memorystatus_level")?.parse().ok()?;
    let total: u64 = sysctl("hw.memsize")?.parse().ok()?;
    Some(total / 100 * percent.min(100))
}

fn sysctl(name: &str) -> Option<String> {
    let output = Command::new("/usr/sbin/sysctl").args(["-n", name]).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(target_os = "macos")]
    fn reads_the_free_memory() {
        let available = super::available().unwrap();
        assert!(available > 0 && available <= super::sysctl("hw.memsize").unwrap().parse().unwrap());
    }
}
