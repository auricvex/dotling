/// OS platform detection for multi-OS dotfile support.
///
/// Each [`LinkEntry`](crate::config::LinkEntry) carries a [`Platform`] tag.
/// Entries tagged [`All`](Platform::All) deploy everywhere; other variants
/// restrict the entry to a single operating system.
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Target operating system for a config entry.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    /// Deploys on every platform (the default).
    #[default]
    All,
    /// Linux only.
    Linux,
    /// macOS only.
    #[serde(alias = "darwin")]
    Macos,
    /// Windows only.
    Windows,
}

impl Platform {
    /// Returns the platform of the current machine.
    pub fn current() -> Self {
        match std::env::consts::OS {
            "linux" => Self::Linux,
            "macos" => Self::Macos,
            "windows" => Self::Windows,
            _ => Self::All,
        }
    }

    /// Returns `true` if this platform matches the current machine.
    ///
    /// [`All`](Self::All) matches every machine.
    pub fn is_active(self) -> bool {
        self == Self::All || self == Self::current()
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::All => write!(f, "all"),
            Self::Linux => write!(f, "linux"),
            Self::Macos => write!(f, "macos"),
            Self::Windows => write!(f, "windows"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_returns_known_variant() {
        let p = Platform::current();
        assert!(matches!(
            p,
            Platform::Linux | Platform::Macos | Platform::Windows
        ));
    }

    #[test]
    fn all_is_always_active() {
        assert!(Platform::All.is_active());
    }

    #[test]
    fn current_is_active() {
        assert!(Platform::current().is_active());
    }

    #[test]
    fn serde_round_trip() {
        #[derive(Debug, Serialize, Deserialize, PartialEq)]
        struct Wrapper {
            os: Platform,
        }
        for p in [
            Platform::All,
            Platform::Linux,
            Platform::Macos,
            Platform::Windows,
        ] {
            let w = Wrapper { os: p };
            let s = toml::to_string(&w).unwrap();
            let back: Wrapper = toml::from_str(&s).unwrap();
            assert_eq!(w, back);
        }
    }
}
