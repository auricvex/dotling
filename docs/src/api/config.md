# config

Configuration data model, hand-rolled TOML parser/serializer, template engine, and variable store.

## `config`

### `DeployMethod`

```rust
pub enum DeployMethod {
    Symlink,
    Copy,
}
```

Deployment method for an entry.

### `DeployMethod::as_str()`

```rust
pub fn as_str(self) -> &'static str
```

Returns `"symlink"` or `"copy"`.

### `Entry`

```rust
pub struct Entry {
    pub source: String,
    pub target: String,
    pub method: Option<DeployMethod>,
    pub encrypted: bool,
    pub directory: bool,
    pub template: bool,
    pub os: Option<String>,
    pub permissions: Option<u32>,
    pub before: Option<String>,
    pub after: Option<String>,
}
```

A tracked file or directory entry.

| Field | Description |
|---|---|
| `source` | Repo-relative path |
| `target` | Deploy target path with `~` support |
| `method` | Override for the default deployment method |
| `encrypted` | Whether the entry is encrypted |
| `directory` | Whether the entry is a directory |
| `template` | Whether the entry is a template |
| `os` | Platform restriction (`"linux"`, `"macos"`, `"windows"`, or `None` for all) |
| `permissions` | Octal permissions applied on sync |
| `before` | Entry-level pre-sync hook |
| `after` | Entry-level post-sync hook |

### `Settings`

```rust
pub struct Settings {
    pub method: DeployMethod,
}
```

Global settings. Default method is `Symlink`.

### `Hooks`

```rust
pub struct Hooks {
    pub init: Option<String>,
    pub before: Option<String>,
    pub after: Option<String>,
}
```

Global lifecycle hooks.

### `Config`

```rust
pub struct Config {
    pub settings: Settings,
    pub entries: Vec<Entry>,
    pub hooks: Hooks,
    pub vars: Vec<(String, String)>,
}
```

The root configuration structure, parsed from `dotling.toml`.

### `Config::new()`

```rust
pub fn new(path: PathBuf) -> Self
```

Creates a new empty config with default settings.

### `Config::load()`

```rust
pub fn load(path: &Path) -> Result<Self>
```

Loads and parses a `dotling.toml` file.

### `Config::save()`

```rust
pub fn save(&self) -> Result<()>
```

Serializes and writes the config back to `dotling.toml`.

### `Config::add_entry()`

```rust
pub fn add_entry(&mut self, entry: Entry) -> Result<()>
```

Adds an entry to the config. Returns an error if an entry with the same source already exists.

### `Config::remove_entry()`

```rust
pub fn remove_entry(&mut self, source: &str) -> Option<Entry>
```

Removes and returns the entry matching the given source path.

### `Config::find_entry()`

```rust
pub fn find_entry(&self, query: &str) -> Option<&Entry>
```

Finds an entry by source path, target path, or partial match.

### `Config::find_entry_mut()`

```rust
pub fn find_entry_mut(&mut self, query: &str) -> Option<&mut Entry>
```

Mutable version of `find_entry`.

---

## `config::template`

### `TemplateVar`

```rust
pub struct TemplateVar {
    pub raw: String,
    pub namespace: String,
    pub key: String,
}
```

A parsed template variable reference.

| Field | Description |
|---|---|
| `raw` | The full expression (e.g., `"var.hostname"`) |
| `namespace` | The namespace: `"dotling"`, `"var"`, or `"env"` |
| `key` | The variable key within the namespace |

### `RenderContext`

```rust
pub struct RenderContext {
    pub builtins: HashMap<String, String>,
    pub vars: Vec<(String, String)>,
    pub env: HashMap<String, String>,
}
```

Context for template rendering.

### `RenderContext::new()`

```rust
pub fn new(
    repo_root: &str,
    config_vars: &[(String, String)],
    local_vars: &[(String, String)],
) -> Self
```

Creates a new render context. Built-in variables (`dotling.*`) are auto-populated. Local vars override config defaults.

### `RenderContext::resolve()`

```rust
pub fn resolve(&self, namespace: &str, key: &str) -> Option<String>
```

Resolves a variable by namespace and key.

### `render()`

```rust
pub fn render(template_text: &str, ctx: &RenderContext, source_name: &str) -> Result<String>
```

Renders a template string with the given context. Returns an error if any variable cannot be resolved (unless a `default` filter is used).

### `scan_variables()`

```rust
pub fn scan_variables(template_text: &str) -> Vec<TemplateVar>
```

Scans a template string and returns all variable references found.

---

## `config::vars`

### `VarStore`

```rust
pub struct VarStore { /* private */ }
```

Machine-local variable store backed by `~/.dotling/vars.toml`.

### `VarStore::load()`

```rust
pub fn load() -> Result<Self>
```

Loads the variable store from `~/.dotling/vars.toml`.

### `VarStore::save()`

```rust
pub fn save(&self) -> Result<()>
```

Saves the variable store to disk.

### `VarStore::get()`

```rust
pub fn get(&self, key: &str) -> Option<&str>
```

Returns the value for a key, or `None`.

### `VarStore::set()`

```rust
pub fn set(&mut self, key: &str, value: &str)
```

Sets a key-value pair. Updates the value if the key already exists.

### `VarStore::remove()`

```rust
pub fn remove(&mut self, key: &str) -> bool
```

Removes a key. Returns `true` if the key existed.

### `VarStore::iter()`

```rust
pub fn iter(&self) -> impl Iterator<Item = (&str, &str)>
```

Iterates over all key-value pairs.

### `VarStore::as_pairs()`

```rust
pub fn as_pairs(&self) -> Vec<(String, String)>
```

Returns all pairs as owned `(String, String)` tuples.

### `VarStore::is_empty()`

```rust
pub fn is_empty(&self) -> bool
```

Returns `true` if the store has no entries.

### `VarStore::len()`

```rust
pub fn len(&self) -> usize
```

Returns the number of entries.

### `VarStore::path()`

```rust
pub fn path() -> Result<PathBuf>
```

Returns the path to `~/.dotling/vars.toml`.

### `import_from_file()`

```rust
pub fn import_from_file(store: &mut VarStore, path: &Path) -> Result<usize>
```

Bulk-imports variables from a TOML file (with `[vars]` section) or `.env` file. Returns the number of variables imported.

### `looks_like_real_value()`

```rust
pub fn looks_like_real_value(key: &str, value: &str, local_store: &VarStore) -> Option<String>
```

Heuristic check for whether a config default looks like a real value (not a placeholder). Returns a warning message if it does.
