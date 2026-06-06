# Templates

Some dotfiles contain machine-specific values — a hostname in a Nix flake, a username in a config, a path that differs per machine. dotling supports opt-in templating to handle this.

## How it works

Any file tracked with `--template` is marked with `template: true` in `dotling.toml`. On every `sync`, dotling renders the template and writes the output to the deploy target — the repo source is never deployed directly.

```sh
# 1. Set your machine-local variables
dotling vars set hostname "Some hostname"
dotling vars set primary_user "someuser"

# 2. Add a file as a template
dotling add ~/.config/nix-darwin/flake.nix --template

# 3. On another machine, sync will detect missing vars and prompt for them
dotling sync
```

## Template syntax

Variables are wrapped in double curly braces:

```
{{ var.key }}
{{ dotling.hostname }}
{{ env.VAR }}
```

### Namespaces

| Expression | Description |
|---|---|
| `{{ var.key }}` | User-defined variable (local or config default) |
| `{{ dotling.hostname }}` | Current machine hostname |
| `{{ dotling.username }}` | Current OS username |
| `{{ dotling.os }}` | `macos`, `linux`, or `windows` |
| `{{ dotling.arch }}` | `x86_64`, `aarch64`, or `arm` |
| `{{ dotling.home }}` | Home directory path |
| `{{ dotling.repo }}` | Dotfiles repo root path |
| `{{ env.VAR }}` | Environment variable |

### Filters

Filters are applied with the pipe (`|`) syntax:

```
{{ var.key | upper }}          Convert to uppercase
{{ var.key | lower }}          Convert to lowercase
{{ var.key | trim }}           Strip leading/trailing whitespace
{{ var.key | quote }}          Wrap in double quotes: "value"
{{ var.key | squote }}         Wrap in single quotes: 'value'
{{ var.key | default "foo" }}  Use fallback if variable is not set
```

Filters can be chained:

```
{{ var.name | trim | upper }}
```

### Whitespace control

Use `-` to strip surrounding whitespace:

```
{{- var.key -}}     Strip whitespace on both sides
{{- var.key }}      Strip whitespace on the left only
{{ var.key -}}      Strip whitespace on the right only
```

This is useful for controlling indentation in generated files:

```nix
{{- if var.enable_feature -}}
  feature = true;
{{- end -}}
```

### Scripts

Templates can execute inline shell commands and insert their standard output into the document using backticks inside template tags:

```
{{ `uname -s` | lower }}
{{ `curl -sSf https://api.ipify.org` | trim }}
```

- **Interpreter**: Commands are executed using `sh -c` on Unix and `cmd /C` on Windows.
- **Environment**: Internal shell pipes, redirects, and environment variables work as expected (e.g. `` {{ `echo $USER` }} ``).
- **Whitespace**: A command's output usually ends with a trailing newline. Dotling trims trailing whitespace automatically before the value is processed by filters, so it substitutes cleanly inline.
- **Failures**: A non-zero exit code or an execution failure fails the template render immediately. You can use the `` `default` `` filter to catch command failures and provide a fallback value: `` {{ `brew --prefix` | default "/opt/homebrew" }} ``.

#### Script Security

Arbitrary command execution during config syncing is a security risk for shared dotfiles. Dotling reuses the `HookSession` trust system to protect against malicious template scripts.

When an untrusted script is encountered during `dotling sync` or `dotling add`, the user is interactively prompted to trust it. Trusted scripts are hashed and remembered in `~/.dotling/trusted_hooks`. In non-interactive environments (e.g. CI), untrusted scripts are skipped (which fails the template render unless caught by a `default` filter) unless explicitly allowed by setting `DOTLING_ALLOW_HOOKS=1`.

## Variable sources

Variables are resolved in priority order:

1. **Local store** — `~/.dotling/vars.toml` (machine-specific, never committed)
2. **Config defaults** — `[vars]` in `dotling.toml` (shared, committed)

Built-in variables (`dotling.*`) and environment variables (`env.*`) are separate namespaces resolved directly.

### Shared defaults

Shared defaults in `dotling.toml` act as documentation and fallbacks — use placeholders, not real values:

```toml
# dotling.toml
[vars]
hostname = "my-mac"       # placeholder — override in ~/.dotling/vars.toml
primary_user = "user"     # placeholder
```

### Machine-local variables

Machine-specific values are stored in `~/.dotling/vars.toml` and are never committed to git:

```sh
dotling vars set hostname "work-laptop"
dotling vars set primary_user "alice"
```

## `dotling vars` reference

```sh
dotling vars list                    # show all resolved variables
dotling vars set hostname "my-mac"   # set a machine-local variable
dotling vars get hostname            # print the resolved value
dotling vars unset hostname          # remove from local store
dotling vars check                   # validate all templates for unresolved variables
dotling vars import ~/.env           # bulk-import from .env or TOML file
dotling vars export                  # print local variables as TOML
```

## Encrypted templates

Sensitive templates (e.g. a config containing tokens) can be both templated and encrypted:

```sh
dotling add ~/.config/secret.conf --template --encrypt
```

The pipeline on sync is: **vault decrypt -> render with vars -> deploy**.

## Example

Given this template at `config/nix-darwin/flake.nix`:

```nix
{
  description = "Nix darwin config for {{ var.hostname }}";

  darwinConfigurations = {
    "{{ var.hostname }}" = darwin.lib.darwinSystem {
      system = "{{ var.arch }}-darwin";
      modules = [ ./configuration.nix ];
    };
  };
}
```

And these local variables:

```sh
dotling vars set hostname "work-mbp"
```

The rendered output on an Apple Silicon Mac would be:

```nix
{
  description = "Nix darwin config for work-mbp";

  darwinConfigurations = {
    "work-mbp" = darwin.lib.darwinSystem {
      system = "aarch64-darwin";
      modules = [ ./configuration.nix ];
    };
  };
}
```
