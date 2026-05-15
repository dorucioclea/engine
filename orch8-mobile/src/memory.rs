/// Returns the current process RSS in bytes, or `None` if unavailable.
pub fn current_rss_bytes() -> Option<u64> {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        darwin_rss()
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        linux_rss()
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "linux",
        target_os = "android"
    )))]
    {
        None
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn darwin_rss() -> Option<u64> {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let kb: u64 = text.trim().parse().ok()?;
    Some(kb * 1024)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn linux_rss() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kb: u64 = rest.trim().strip_suffix("kB")?.trim().parse().ok()?;
            return Some(kb * 1024);
        }
    }
    None
}

pub fn exceeds_budget(budget_bytes: u64) -> bool {
    if budget_bytes == 0 {
        return false;
    }
    current_rss_bytes().is_some_and(|rss| rss > budget_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rss_returns_positive_value() {
        if let Some(rss) = current_rss_bytes() {
            assert!(rss > 0, "RSS should be positive");
        }
    }

    #[test]
    fn zero_budget_never_exceeds() {
        assert!(!exceeds_budget(0));
    }

    #[test]
    fn huge_budget_never_exceeds() {
        assert!(!exceeds_budget(u64::MAX));
    }
}
