# Environment Variables

dotling reads and respects the following environment variables.

## User-facing variables

| Variable | Description |
|---|---|
| `NO_COLOR` | Disables ANSI color output when set (follows the [no-color.org](https://no-color.org) standard) |
| `DOTLING_EDITOR` | Highest-priority editor override for `dotling edit` |
| `VISUAL` | Editor override (standard Unix convention) |
| `EDITOR` | Editor fallback |
| `DOTLING_ALLOW_HOOKS` | Set to `1` or `true` to auto-trust and execute all hooks without prompting |
| `DOTLING_NO_HOOKS` | Set to `1` or `true` to completely disable hook execution |
| `HOSTNAME` | Fallback hostname for `dotling.hostname` built-in if the syscall fails |
| `USER` / `USERNAME` | Used for `dotling.username` built-in (`USERNAME` is the Windows fallback) |

## Hook context variables

These variables are set in the environment when hooks execute:

| Variable | Description |
|---|---|
| `DOTLING_HOOK_TYPE` | Type of hook: `global_init`, `global_before`, `global_after`, `entry_before`, `entry_after` |
| `DOTLING_REPO_ROOT` | Absolute path to the dotfiles repository |
| `DOTLING_DRY_RUN` | `"true"` if running with `--dry-run`, otherwise `"false"` |
| `DOTLING_ENTRY_SOURCE` | (Entry hooks only) Repo-relative path of the entry's source file/folder |
| `DOTLING_ENTRY_TARGET` | (Entry hooks only) Target path of the entry's deployed file/folder |
| `DOTLING_ENTRY_ACTION` | (Entry hooks only) Current action being performed: `"push"` or `"pull"` |

## Template environment variables

Any environment variable can be accessed in templates using the `{{ env.VAR }}` syntax. For example, `{{ env.HOME }}` resolves to the value of `$HOME`.
