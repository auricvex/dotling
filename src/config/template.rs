use std::collections::HashMap;

use crate::error::{Error, Result};

// ── Public types ───────────────────────────────────────────────────

/// A parsed variable reference found inside a template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateVar {
    /// Full variable expression as written, e.g. `var.hostname | upper`.
    pub raw: String,
    /// Namespace: `"dotling"`, `"var"`, or `"env"`.
    pub namespace: String,
    /// Key within the namespace.
    pub key: String,
}

/// Context used to resolve variables during rendering.
pub struct RenderContext {
    /// `dotling.*` built-in values.
    pub builtins: HashMap<String, String>,
    /// `var.*` values: local store values override config defaults.
    /// Stored as a plain list; lookup scans from front to back so
    /// callers can prepend local vars to override config defaults.
    pub vars: Vec<(String, String)>,
    /// `env.*` values — snapshot of selected env vars.
    pub env: HashMap<String, String>,
}

impl RenderContext {
    /// Build a context from the current machine state, config defaults, and local vars.
    pub fn new(
        repo_root: &str,
        config_vars: &[(String, String)],
        local_vars: &[(String, String)],
    ) -> Self {
        let mut builtins = HashMap::new();

        // dotling.hostname
        builtins.insert(
            "hostname".into(),
            gethostname().unwrap_or_else(|| "unknown".into()),
        );

        // dotling.username
        builtins.insert(
            "username".into(),
            std::env::var("USER")
                .or_else(|_| std::env::var("USERNAME"))
                .unwrap_or_else(|_| "unknown".into()),
        );

        // dotling.os
        let os = if cfg!(target_os = "macos") {
            "macos"
        } else if cfg!(target_os = "windows") {
            "windows"
        } else {
            "linux"
        };
        builtins.insert("os".into(), os.into());

        // dotling.arch
        let arch = if cfg!(target_arch = "aarch64") {
            "aarch64"
        } else if cfg!(target_arch = "arm") {
            "arm"
        } else {
            "x86_64"
        };
        builtins.insert("arch".into(), arch.into());

        // dotling.home
        builtins.insert(
            "home".into(),
            crate::path::home_dir()
                .map_or_else(|_| "~".into(), |p| p.to_string_lossy().into_owned()),
        );

        // dotling.repo
        builtins.insert("repo".into(), repo_root.to_string());

        // Build merged vars: local_vars first (higher priority), then config_vars as fallback.
        let mut vars: Vec<(String, String)> = local_vars.to_vec();
        for (k, v) in config_vars {
            if !vars.iter().any(|(lk, _)| lk == k) {
                vars.push((k.clone(), v.clone()));
            }
        }

        // Capture environment variables
        let env: HashMap<String, String> = std::env::vars().collect();

        Self {
            builtins,
            vars,
            env,
        }
    }

    /// Resolve a variable by namespace and key.
    /// Returns `None` if not found.
    pub fn resolve(&self, namespace: &str, key: &str) -> Option<String> {
        match namespace {
            "dotling" => self.builtins.get(key).cloned(),
            "var" => self
                .vars
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone()),
            "env" => self.env.get(key).cloned(),
            _ => None,
        }
    }
}

// ── Public API ─────────────────────────────────────────────────────

/// Returns true if a source path is a template (ends with `.dtmpl`).
pub fn is_template_source(source: &str) -> bool {
    std::path::Path::new(source)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("dtmpl"))
}

/// Render a template string with the given context.
///
/// Returns the fully rendered string, or an `Error::Template` if any
/// variable is unresolved or a filter is unknown.
pub fn render(template_text: &str, ctx: &RenderContext, source_name: &str) -> Result<String> {
    let mut output = String::with_capacity(template_text.len());
    let mut remaining = template_text;

    while let Some(open_pos) = remaining.find("{{") {
        // Text before the tag
        let before = &remaining[..open_pos];

        // Find closing }}
        let after_open = &remaining[open_pos + 2..];
        let close_pos = after_open.find("}}").ok_or_else(|| Error::Template {
            source: source_name.to_string(),
            message: "unclosed `{{` — missing `}}`".into(),
        })?;

        let tag_inner = &after_open[..close_pos];

        // Whitespace trimming markers
        let trim_left = tag_inner.starts_with('-');
        let trim_right = tag_inner.ends_with('-');

        // Strip markers and surrounding whitespace from inner expression
        let expr = tag_inner
            .trim_start_matches('-')
            .trim_end_matches('-')
            .trim();

        // Add text before tag, applying left-trim if requested
        if trim_left {
            output.push_str(before.trim_end());
        } else {
            output.push_str(before);
        }

        // Parse and resolve the expression
        let value = eval_expr(expr, ctx, source_name)?;

        // Apply right-trim: skip leading whitespace in `remaining` after `}}`
        let rest_start = open_pos + 2 + close_pos + 2;
        remaining = &remaining[rest_start..];

        if trim_right {
            output.push_str(&value);
            // Skip leading whitespace in the remaining text
            remaining = remaining.trim_start_matches([' ', '\t']);
        } else {
            output.push_str(&value);
        }
    }

    // Append any trailing text after the last tag
    output.push_str(remaining);

    Ok(output)
}

/// Evaluate a single template expression (variable + optional pipe filters).
fn eval_expr(expr: &str, ctx: &RenderContext, source_name: &str) -> Result<String> {
    // Split on `|` into variable reference + filters
    let mut parts = expr.splitn(2, '|');
    let var_part = parts.next().unwrap_or("").trim();
    let filter_part = parts.next().map(str::trim);

    // Resolve the variable
    let (namespace, key) = parse_var_ref(var_part, source_name)?;
    let raw_value = ctx.resolve(&namespace, &key);

    // Apply filters (which may supply a default)
    let value = apply_filters(raw_value, filter_part, source_name)?;

    Ok(value)
}

/// Parse `namespace.key` from a variable reference string.
fn parse_var_ref(var_part: &str, source_name: &str) -> Result<(String, String)> {
    let (ns, key) = var_part.split_once('.').ok_or_else(|| Error::Template {
        source: source_name.to_string(),
        message: format!(
            "invalid variable `{var_part}` — expected `namespace.key` (e.g. `var.hostname`)"
        ),
    })?;

    let ns = ns.trim();
    let key = key.trim();

    if !matches!(ns, "dotling" | "var" | "env") {
        return Err(Error::Template {
            source: source_name.to_string(),
            message: format!(
                "unknown namespace `{ns}` in `{var_part}` — valid namespaces: dotling, var, env"
            ),
        });
    }

    Ok((ns.to_string(), key.to_string()))
}

/// Apply zero or more pipe filters to a resolved value.
///
/// If the value is `None` (variable not resolved) and no `default` filter
/// is present, returns an `Error::Template`.
fn apply_filters(
    mut value: Option<String>,
    filter_str: Option<&str>,
    source_name: &str,
) -> Result<String> {
    let Some(filters_raw) = filter_str else {
        // No filters — value must be resolved.
        return value.ok_or_else(|| Error::Template {
            source: source_name.to_string(),
            message: "unresolved variable (use `| default \"fallback\"` to make it optional)"
                .into(),
        });
    };

    // Filters are separated by `|`
    // We already split once on the first `|` in eval_expr, so filter_str is
    // everything after that `|`, potentially containing more `|` separators.
    // Re-split on `|` to get individual filter tokens.
    for raw_filter in filters_raw.split('|') {
        let filter = raw_filter.trim();
        if filter.is_empty() {
            continue;
        }

        // `default "..."` — provides a fallback for unresolved vars
        if let Some(rest) = filter.strip_prefix("default") {
            let fallback = rest.trim().trim_matches('"').trim_matches('\'').to_string();
            if value.is_none() {
                value = Some(fallback);
            }
            continue;
        }

        // All other filters require a resolved value
        let v = value.as_mut().ok_or_else(|| Error::Template {
            source: source_name.to_string(),
            message: "unresolved variable — cannot apply filter (use `| default \"\"` first)"
                .into(),
        })?;

        match filter {
            "upper" => *v = v.to_uppercase(),
            "lower" => *v = v.to_lowercase(),
            "trim" => *v = v.trim().to_string(),
            "quote" => *v = format!("\"{v}\""),
            "squote" => *v = format!("'{v}'"),
            other => {
                return Err(Error::Template {
                    source: source_name.to_string(),
                    message: format!(
                        "unknown filter `{other}` — valid filters: upper, lower, trim, quote, squote, default"
                    ),
                });
            }
        }
    }

    // After all filters, if still unresolved, error.
    value.ok_or_else(|| Error::Template {
        source: source_name.to_string(),
        message: "unresolved variable (use `| default \"fallback\"` to make it optional)".into(),
    })
}

/// Scan a template source for all variable references.
///
/// Returns one `TemplateVar` per `{{ }}` block found.  Duplicate references
/// are included only once (by `namespace:key` identity).
pub fn scan_variables(template_text: &str) -> Vec<TemplateVar> {
    let mut vars: Vec<TemplateVar> = Vec::new();
    let mut remaining = template_text;

    while let Some(open_pos) = remaining.find("{{") {
        let after_open = &remaining[open_pos + 2..];
        let Some(close_pos) = after_open.find("}}") else {
            break;
        };

        let tag_inner = &after_open[..close_pos];
        let expr = tag_inner
            .trim_start_matches('-')
            .trim_end_matches('-')
            .trim();

        // Take only the variable part (before any `|`)
        let var_part = expr.split('|').next().unwrap_or("").trim();

        if let Some((ns, key)) = var_part.split_once('.') {
            let ns = ns.trim().to_string();
            let key = key.trim().to_string();
            let already = vars.iter().any(|v| v.namespace == ns && v.key == key);
            if !already {
                vars.push(TemplateVar {
                    raw: expr.to_string(),
                    namespace: ns,
                    key,
                });
            }
        }

        remaining = &remaining[open_pos + 2 + close_pos + 2..];
    }

    vars
}

// ── Platform helpers ───────────────────────────────────────────────

/// Get the machine hostname using `gethostname` syscall on Unix,
/// or `COMPUTERNAME` env var on Windows.
fn gethostname() -> Option<String> {
    #[cfg(unix)]
    {
        let mut buf = vec![0u8; 256];
        let ret = unsafe {
            unsafe extern "C" {
                fn gethostname(name: *mut u8, len: usize) -> i32;
            }
            gethostname(buf.as_mut_ptr(), buf.len())
        };
        if ret == 0 {
            let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            String::from_utf8(buf[..len].to_vec())
                .ok()
                .map(|s| s.trim().to_string())
        } else {
            std::env::var("HOSTNAME").ok()
        }
    }
    #[cfg(not(unix))]
    {
        std::env::var("COMPUTERNAME").ok()
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> RenderContext {
        RenderContext {
            builtins: {
                let mut m = HashMap::new();
                m.insert("hostname".into(), "test-host".into());
                m.insert("username".into(), "testuser".into());
                m.insert("os".into(), "linux".into());
                m.insert("arch".into(), "x86_64".into());
                m.insert("home".into(), "/home/testuser".into());
                m.insert("repo".into(), "/home/testuser/dotfiles".into());
                m
            },
            vars: vec![
                ("myvar".into(), "hello".into()),
                ("label".into(), "MacBook Air".into()),
            ],
            env: {
                let mut m = HashMap::new();
                m.insert("HOME".into(), "/home/testuser".into());
                m
            },
        }
    }

    #[test]
    fn render_simple_var() {
        let ctx = test_ctx();
        let out = render("host={{ var.myvar }}", &ctx, "test.dtmpl").unwrap();
        assert_eq!(out, "host=hello");
    }

    #[test]
    fn render_builtin() {
        let ctx = test_ctx();
        let out = render("os={{ dotling.os }}", &ctx, "test.dtmpl").unwrap();
        assert_eq!(out, "os=linux");
    }

    #[test]
    fn render_env() {
        let ctx = test_ctx();
        let out = render("home={{ env.HOME }}", &ctx, "test.dtmpl").unwrap();
        assert_eq!(out, "home=/home/testuser");
    }

    #[test]
    fn render_filter_upper() {
        let ctx = test_ctx();
        let out = render("{{ var.myvar | upper }}", &ctx, "test.dtmpl").unwrap();
        assert_eq!(out, "HELLO");
    }

    #[test]
    fn render_filter_lower() {
        let ctx = test_ctx();
        let out = render("{{ var.label | lower }}", &ctx, "test.dtmpl").unwrap();
        assert_eq!(out, "macbook air");
    }

    #[test]
    fn render_filter_trim() {
        let mut ctx = test_ctx();
        ctx.vars.push(("padded".into(), "  hello  ".into()));
        let out = render("{{ var.padded | trim }}", &ctx, "test.dtmpl").unwrap();
        assert_eq!(out, "hello");
    }

    #[test]
    fn render_filter_quote() {
        let ctx = test_ctx();
        let out = render("{{ var.myvar | quote }}", &ctx, "test.dtmpl").unwrap();
        assert_eq!(out, "\"hello\"");
    }

    #[test]
    fn render_filter_squote() {
        let ctx = test_ctx();
        let out = render("{{ var.myvar | squote }}", &ctx, "test.dtmpl").unwrap();
        assert_eq!(out, "'hello'");
    }

    #[test]
    fn render_filter_default_when_missing() {
        let ctx = test_ctx();
        let out = render(
            r#"{{ var.missing | default "fallback" }}"#,
            &ctx,
            "test.dtmpl",
        )
        .unwrap();
        assert_eq!(out, "fallback");
    }

    #[test]
    fn render_filter_default_not_applied_when_present() {
        let ctx = test_ctx();
        let out = render(
            r#"{{ var.myvar | default "fallback" }}"#,
            &ctx,
            "test.dtmpl",
        )
        .unwrap();
        assert_eq!(out, "hello");
    }

    #[test]
    fn render_whitespace_trim_both() {
        let ctx = test_ctx();
        let out = render("  {{- var.myvar -}}  next", &ctx, "test.dtmpl").unwrap();
        assert_eq!(out, "hellonext");
    }

    #[test]
    fn render_whitespace_trim_left() {
        let ctx = test_ctx();
        let out = render("  {{- var.myvar }} rest", &ctx, "test.dtmpl").unwrap();
        assert_eq!(out, "hello rest");
    }

    #[test]
    fn render_whitespace_trim_right() {
        let ctx = test_ctx();
        let out = render("pre {{ var.myvar -}}  rest", &ctx, "test.dtmpl").unwrap();
        assert_eq!(out, "pre hellorest");
    }

    #[test]
    fn render_unresolved_error() {
        let ctx = test_ctx();
        let result = render("{{ var.nonexistent }}", &ctx, "test.dtmpl");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("unresolved"));
    }

    #[test]
    fn render_unknown_filter_error() {
        let ctx = test_ctx();
        let result = render("{{ var.myvar | notafilter }}", &ctx, "test.dtmpl");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("unknown filter"));
    }

    #[test]
    fn render_unknown_namespace_error() {
        let ctx = test_ctx();
        let result = render("{{ bad.thing }}", &ctx, "test.dtmpl");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("unknown namespace"));
    }

    #[test]
    fn render_missing_close_error() {
        let ctx = test_ctx();
        let result = render("{{ var.x", &ctx, "test.dtmpl");
        assert!(result.is_err());
    }

    #[test]
    fn render_no_tags_passthrough() {
        let ctx = test_ctx();
        let src = "plain text\nno tags here";
        let out = render(src, &ctx, "test.dtmpl").unwrap();
        assert_eq!(out, src);
    }

    #[test]
    fn scan_variables_basic() {
        let src = "a={{ var.x }}\nb={{ dotling.hostname }}\nc={{ env.HOME }}";
        let vars = scan_variables(src);
        assert_eq!(vars.len(), 3);
        assert_eq!(vars[0].namespace, "var");
        assert_eq!(vars[0].key, "x");
        assert_eq!(vars[1].namespace, "dotling");
        assert_eq!(vars[1].key, "hostname");
        assert_eq!(vars[2].namespace, "env");
        assert_eq!(vars[2].key, "HOME");
    }

    #[test]
    fn scan_variables_empty() {
        let vars = scan_variables("no template tags here");
        assert!(vars.is_empty());
    }

    #[test]
    fn scan_variables_deduplicates() {
        let src = "{{ var.x }} {{ var.x }} {{ var.y }}";
        let vars = scan_variables(src);
        assert_eq!(vars.len(), 2);
    }

    #[test]
    fn is_template_source_check() {
        assert!(is_template_source("config/nvim/init.lua.dtmpl"));
        assert!(is_template_source("shell/zshrc.dtmpl"));
        assert!(!is_template_source("shell/zshrc"));
        assert!(!is_template_source("shell/zshrc.enc"));
    }

    #[test]
    fn var_priority_local_over_config() {
        // Local vars should win over config defaults
        let config_vars = vec![("key".into(), "config_val".into())];
        let local_vars = vec![("key".into(), "local_val".into())];
        let ctx = RenderContext::new("/repo", &config_vars, &local_vars);
        let out = render("{{ var.key }}", &ctx, "t.dtmpl").unwrap();
        assert_eq!(out, "local_val");
    }

    #[test]
    fn var_falls_back_to_config_default() {
        let config_vars = vec![("key".into(), "config_val".into())];
        let local_vars: Vec<(String, String)> = vec![];
        let ctx = RenderContext::new("/repo", &config_vars, &local_vars);
        let out = render("{{ var.key }}", &ctx, "t.dtmpl").unwrap();
        assert_eq!(out, "config_val");
    }
}
