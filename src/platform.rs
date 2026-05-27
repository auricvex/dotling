/// Detected operating system platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Linux,
    Macos,
    Windows,
}

impl Platform {
    /// Returns the platform for the current OS.
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Linux
        }
    }

    /// Parse a platform string (case-insensitive).
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "linux" => Some(Self::Linux),
            "macos" | "darwin" => Some(Self::Macos),
            "windows" | "win" => Some(Self::Windows),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Macos => "macos",
            Self::Windows => "windows",
        }
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Returns `true` if an entry tagged with `os` should be deployed on this machine.
pub fn should_deploy(os: Option<&str>) -> bool {
    match os {
        None | Some("all") => true,
        Some(s) => Platform::parse(s).is_some_and(|p| p == Platform::current()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_is_valid() {
        let p = Platform::current();
        assert!(!p.as_str().is_empty());
    }

    #[test]
    fn parse_roundtrip() {
        for p in [Platform::Linux, Platform::Macos, Platform::Windows] {
            assert_eq!(Platform::parse(p.as_str()), Some(p));
        }
    }

    #[test]
    fn all_always_deploys() {
        assert!(should_deploy(None));
        assert!(should_deploy(Some("all")));
    }
}
