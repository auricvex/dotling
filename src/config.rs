use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

use crate::error::{Error, Result};

// ── Data model ────────────────────────────────────────────────────

/// How an entry is deployed to the filesystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeployMethod {
    Symlink,
    Copy,
}

impl DeployMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Symlink => "symlink",
            Self::Copy => "copy",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "symlink" => Some(Self::Symlink),
            "copy" => Some(Self::Copy),
            _ => None,
        }
    }
}

impl fmt::Display for DeployMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A single tracked dotfile entry.
#[derive(Debug, Clone)]
pub struct Entry {
    /// Repo-relative source path (e.g., `shell/zshrc`).
    pub source: String,
    /// Deploy target path (e.g., `~/.zshrc`).
    pub target: String,
    /// Deploy method override (uses repo default if `None`).
    pub method: Option<DeployMethod>,
    /// Whether this entry is encrypted.
    pub encrypted: bool,
    /// Whether this is a directory entry.
    pub directory: bool,
    /// OS restriction (e.g., `"linux"`, `"macos"`). `None` means all.
    pub os: Option<String>,
}

/// Repo-level settings.
#[derive(Debug, Clone)]
pub struct Settings {
    /// Default deploy method for entries without an explicit override.
    pub method: DeployMethod,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            method: DeployMethod::Symlink,
        }
    }
}

/// The top-level configuration stored in `dotling.toml`.
#[derive(Debug, Clone)]
pub struct Config {
    pub settings: Settings,
    pub entries: Vec<Entry>,
    /// Path to the config file itself.
    path: PathBuf,
}

impl Config {
    /// Create a new, empty config.
    pub fn new(path: PathBuf) -> Self {
        Self {
            settings: Settings::default(),
            entries: Vec::new(),
            path,
        }
    }

    /// Load config from a file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path).map_err(|e| Error::io(path, "read config", e))?;
        let mut config = parse_config(&content, path)?;
        config.path = path.to_path_buf();
        Ok(config)
    }

    /// Save config to its file.
    pub fn save(&self) -> Result<()> {
        let content = serialize_config(self);
        crate::fs::atomic_write(&self.path, content.as_bytes())
    }

    /// Add an entry. Returns an error if the source already exists.
    pub fn add_entry(&mut self, entry: Entry) -> Result<()> {
        if self.entries.iter().any(|e| e.source == entry.source) {
            return Err(Error::User(format!(
                "`{}` is already tracked",
                entry.source
            )));
        }
        if self.entries.iter().any(|e| e.target == entry.target) {
            return Err(Error::User(format!(
                "target `{}` is already in use by `{}`",
                entry.target,
                self.entries
                    .iter()
                    .find(|e| e.target == entry.target)
                    .map_or("?", |e| e.source.as_str()),
            )));
        }
        self.entries.push(entry);
        Ok(())
    }

    /// Remove an entry by source path. Returns the removed entry.
    pub fn remove_entry(&mut self, source: &str) -> Option<Entry> {
        if let Some(i) = self.entries.iter().position(|e| e.source == source) {
            Some(self.entries.remove(i))
        } else {
            None
        }
    }

    /// Find an entry by source path or target path.
    pub fn find_entry(&self, query: &str) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|e| e.source == query || e.target == query)
    }

    /// Find an entry mutably by source path.
    pub fn find_entry_mut(&mut self, source: &str) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|e| e.source == source)
    }
}

// ── TOML parser (minimal subset) ──────────────────────────────────

/// Parse a dotling.toml config string.
fn parse_config(input: &str, path: &Path) -> Result<Config> {
    let mut settings = Settings::default();
    let mut entries = Vec::new();

    let mut current_section: Option<String> = None;
    let mut current_entry: Option<EntryBuilder> = None;

    for (line_num, raw_line) in input.lines().enumerate() {
        let line_num = line_num + 1; // 1-indexed
        let line = raw_line.split('#').next().unwrap_or("").trim();

        if line.is_empty() {
            continue;
        }

        // Array-of-tables: [[entries]]
        if line.starts_with("[[") && line.ends_with("]]") {
            // Flush previous entry
            if let Some(builder) = current_entry.take() {
                entries.push(builder.build(path, line_num)?);
            }
            let name = &line[2..line.len() - 2].trim();
            if *name == "entries" {
                current_entry = Some(EntryBuilder::default());
                current_section = Some("entries".into());
            } else {
                return Err(Error::Config {
                    message: format!("unknown section `[[{name}]]`"),
                    line: Some(line_num),
                });
            }
            continue;
        }

        // Table: [section]
        if line.starts_with('[') && line.ends_with(']') {
            // Flush previous entry
            if let Some(builder) = current_entry.take() {
                entries.push(builder.build(path, line_num)?);
            }
            let name = &line[1..line.len() - 1].trim();
            current_section = Some((*name).to_string());
            continue;
        }

        // Key-value pair
        if let Some((key, value)) = parse_kv(line) {
            handle_kv(
                key,
                &value,
                current_section.as_deref(),
                &mut settings,
                &mut current_entry,
                line_num,
            )?;
        }
    }

    // Flush last entry
    if let Some(builder) = current_entry.take() {
        entries.push(builder.build(path, input.lines().count())?);
    }

    Ok(Config {
        settings,
        entries,
        path: path.to_path_buf(),
    })
}

fn handle_kv(
    key: &str,
    value: &str,
    current_section: Option<&str>,
    settings: &mut Settings,
    current_entry: &mut Option<EntryBuilder>,
    line_num: usize,
) -> Result<()> {
    match current_section {
        Some("settings") => match key {
            "method" => {
                settings.method = DeployMethod::parse(value).ok_or_else(|| Error::Config {
                    message: format!("invalid method `{value}`"),
                    line: Some(line_num),
                })?;
            }
            _ => {
                return Err(Error::Config {
                    message: format!("unknown setting `{key}`"),
                    line: Some(line_num),
                });
            }
        },
        Some("entries") => {
            let builder = current_entry.as_mut().ok_or_else(|| Error::Config {
                message: "key-value outside [[entries]]".into(),
                line: Some(line_num),
            })?;
            match key {
                "source" => builder.source = Some(value.to_string()),
                "target" => builder.target = Some(value.to_string()),
                "method" => builder.method = Some(value.to_string()),
                "encrypted" => builder.encrypted = parse_bool(value),
                "directory" => builder.directory = parse_bool(value),
                "os" => builder.os = Some(value.to_string()),
                _ => {
                    return Err(Error::Config {
                        message: format!("unknown entry field `{key}`"),
                        line: Some(line_num),
                    });
                }
            }
        }
        _ => {}
    }
    Ok(())
}

#[derive(Default)]
struct EntryBuilder {
    source: Option<String>,
    target: Option<String>,
    method: Option<String>,
    encrypted: bool,
    directory: bool,
    os: Option<String>,
}

impl EntryBuilder {
    fn build(self, path: &Path, line: usize) -> Result<Entry> {
        let source = self.source.ok_or_else(|| Error::Config {
            message: "entry missing `source`".into(),
            line: Some(line),
        })?;
        let target = self.target.ok_or_else(|| Error::Config {
            message: format!("entry `{source}` missing `target`"),
            line: Some(line),
        })?;
        let method = self
            .method
            .as_deref()
            .map(|s| {
                DeployMethod::parse(s).ok_or_else(|| Error::Config {
                    message: format!("invalid method `{s}` for entry `{source}`"),
                    line: Some(line),
                })
            })
            .transpose()?;

        let _ = path; // Silence unused warning

        Ok(Entry {
            source,
            target,
            method,
            encrypted: self.encrypted,
            directory: self.directory,
            os: self.os,
        })
    }
}

/// Parse a `key = value` line.
fn parse_kv(line: &str) -> Option<(&str, String)> {
    let (key, rest) = line.split_once('=')?;
    let key = key.trim();
    let value = rest.trim();

    // Strip quotes
    let value = if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
    {
        unescape_string(&value[1..value.len() - 1])
    } else {
        value.to_string()
    };

    Some((key, value))
}

/// Parse a boolean value.
fn parse_bool(s: &str) -> bool {
    matches!(s.to_ascii_lowercase().as_str(), "true" | "1" | "yes")
}

/// Unescape basic TOML string escapes.
fn unescape_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some(ch @ ('\\' | '"')) => result.push(ch),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }
    result
}

// ── TOML serializer ───────────────────────────────────────────────

/// Serialize a config to TOML.
fn serialize_config(config: &Config) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# dotling.toml — managed by dotling, safe to hand-edit\n"
    );

    // [settings]
    if config.settings.method != DeployMethod::Symlink {
        let _ = writeln!(out, "[settings]");
        let _ = writeln!(out, "method = \"{}\"\n", config.settings.method.as_str());
    }

    // [[entries]]
    for entry in &config.entries {
        let _ = writeln!(out, "[[entries]]");
        let _ = writeln!(out, "source = \"{}\"", escape_string(&entry.source));
        let _ = writeln!(out, "target = \"{}\"", escape_string(&entry.target));

        if let Some(method) = entry.method {
            let _ = writeln!(out, "method = \"{}\"", method.as_str());
        }
        if entry.encrypted {
            let _ = writeln!(out, "encrypted = true");
        }
        if entry.directory {
            let _ = writeln!(out, "directory = true");
        }
        if let Some(ref os) = entry.os {
            let _ = writeln!(out, "os = \"{os}\"");
        }
        let _ = writeln!(out);
    }

    out
}

/// Escape a string for TOML output.
fn escape_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_config() {
        let config = parse_config("", Path::new("test.toml")).unwrap();
        assert!(config.entries.is_empty());
        assert_eq!(config.settings.method, DeployMethod::Symlink);
    }

    #[test]
    fn parse_basic_config() {
        let input = r#"
# dotling.toml

[settings]
method = "symlink"

[[entries]]
source = "shell/zshrc"
target = "~/.zshrc"

[[entries]]
source = "config/nvim"
target = "~/.config/nvim"
directory = true
method = "copy"
os = "macos"
"#;

        let config = parse_config(input, Path::new("test.toml")).unwrap();
        assert_eq!(config.settings.method, DeployMethod::Symlink);
        assert_eq!(config.entries.len(), 2);

        assert_eq!(config.entries[0].source, "shell/zshrc");
        assert_eq!(config.entries[0].target, "~/.zshrc");
        assert!(!config.entries[0].directory);
        assert!(config.entries[0].method.is_none());

        assert_eq!(config.entries[1].source, "config/nvim");
        assert_eq!(config.entries[1].target, "~/.config/nvim");
        assert!(config.entries[1].directory);
        assert_eq!(config.entries[1].method, Some(DeployMethod::Copy));
        assert_eq!(config.entries[1].os.as_deref(), Some("macos"));
    }

    #[test]
    fn serialize_roundtrip() {
        let config = Config {
            settings: Settings {
                method: DeployMethod::Symlink,
            },
            entries: vec![
                Entry {
                    source: "shell/zshrc".into(),
                    target: "~/.zshrc".into(),
                    method: None,
                    encrypted: false,
                    directory: false,
                    os: None,
                },
                Entry {
                    source: "config/nvim".into(),
                    target: "~/.config/nvim".into(),
                    method: Some(DeployMethod::Copy),
                    encrypted: true,
                    directory: true,
                    os: Some("linux".into()),
                },
            ],
            path: PathBuf::from("test.toml"),
        };

        let serialized = serialize_config(&config);
        let parsed = parse_config(&serialized, Path::new("test.toml")).unwrap();

        assert_eq!(parsed.entries.len(), 2);
        assert_eq!(parsed.entries[0].source, "shell/zshrc");
        assert_eq!(parsed.entries[1].encrypted, true);
        assert_eq!(parsed.entries[1].directory, true);
    }

    #[test]
    fn duplicate_source_rejected() {
        let mut config = Config::new(PathBuf::from("test.toml"));
        config
            .add_entry(Entry {
                source: "a".into(),
                target: "~/.a".into(),
                method: None,
                encrypted: false,
                directory: false,
                os: None,
            })
            .unwrap();

        let err = config
            .add_entry(Entry {
                source: "a".into(),
                target: "~/.b".into(),
                method: None,
                encrypted: false,
                directory: false,
                os: None,
            })
            .unwrap_err();

        assert!(err.to_string().contains("already tracked"));
    }

    #[test]
    fn duplicate_target_rejected() {
        let mut config = Config::new(PathBuf::from("test.toml"));
        config
            .add_entry(Entry {
                source: "a".into(),
                target: "~/.a".into(),
                method: None,
                encrypted: false,
                directory: false,
                os: None,
            })
            .unwrap();

        let err = config
            .add_entry(Entry {
                source: "b".into(),
                target: "~/.a".into(),
                method: None,
                encrypted: false,
                directory: false,
                os: None,
            })
            .unwrap_err();

        assert!(err.to_string().contains("already in use"));
    }

    #[test]
    fn find_by_source_or_target() {
        let mut config = Config::new(PathBuf::from("test.toml"));
        config
            .add_entry(Entry {
                source: "shell/zshrc".into(),
                target: "~/.zshrc".into(),
                method: None,
                encrypted: false,
                directory: false,
                os: None,
            })
            .unwrap();

        assert!(config.find_entry("shell/zshrc").is_some());
        assert!(config.find_entry("~/.zshrc").is_some());
        assert!(config.find_entry("nope").is_none());
    }

    #[test]
    fn remove_entry() {
        let mut config = Config::new(PathBuf::from("test.toml"));
        config
            .add_entry(Entry {
                source: "a".into(),
                target: "~/.a".into(),
                method: None,
                encrypted: false,
                directory: false,
                os: None,
            })
            .unwrap();

        assert!(config.remove_entry("a").is_some());
        assert!(config.entries.is_empty());
        assert!(config.remove_entry("a").is_none());
    }
}
