use serde::{Deserialize, Serialize};

/// Version information for update checking.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SemVer {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl SemVer {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self { major, minor, patch }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.trim_start_matches('v').split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        Some(Self {
            major: parts[0].parse().ok()?,
            minor: parts[1].parse().ok()?,
            patch: parts[2].parse().ok()?,
        })
    }
}

impl std::fmt::Display for SemVer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Current version of the PRISM server.
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Result of an update check.
#[derive(Debug, Clone)]
pub enum UpdateStatus {
    UpToDate,
    UpdateAvailable { latest: SemVer, download_url: String },
    CheckFailed { reason: String },
}

/// Check if an update is available by comparing versions.
pub fn check_version(current: &SemVer, latest: &SemVer) -> bool {
    latest > current
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_parse() {
        let v = SemVer::parse("0.1.0").expect("should parse");
        assert_eq!(v, SemVer::new(0, 1, 0));
    }

    #[test]
    fn semver_parse_with_v_prefix() {
        let v = SemVer::parse("v1.2.3").expect("should parse with v prefix");
        assert_eq!(v, SemVer::new(1, 2, 3));
    }

    #[test]
    fn semver_ordering() {
        let v100 = SemVer::new(1, 0, 0);
        let v101 = SemVer::new(1, 0, 1);
        let v110 = SemVer::new(1, 1, 0);
        let v200 = SemVer::new(2, 0, 0);
        assert!(v100 < v101);
        assert!(v101 < v110);
        assert!(v110 < v200);
    }

    #[test]
    fn check_version_detects_update() {
        let current = SemVer::new(0, 1, 0);
        let latest = SemVer::new(0, 2, 0);
        assert!(check_version(&current, &latest));
    }

    #[test]
    fn check_version_up_to_date() {
        let current = SemVer::new(0, 1, 0);
        let latest = SemVer::new(0, 1, 0);
        assert!(!check_version(&current, &latest));
    }

    #[test]
    fn current_version_parses() {
        let v = SemVer::parse(CURRENT_VERSION)
            .unwrap_or_else(|| panic!("CURRENT_VERSION '{}' should parse as SemVer", CURRENT_VERSION));
        // Sanity: display round-trips
        assert_eq!(v.to_string(), CURRENT_VERSION);
    }
}
