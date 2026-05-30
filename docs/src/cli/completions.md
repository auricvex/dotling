# dotling completions

Generate shell completion scripts.

## Usage

```sh
dotling completions <SHELL>
```

## Arguments

| Argument | Description |
|---|---|
| `<SHELL>` | The shell to generate completions for |

## Supported shells

`bash`, `zsh`, `fish`, `elvish`, `powershell`

## Installation

### Quick install (auto-detect shell)

```sh
just install-completions
```

### Manual install

#### Bash

```sh
dotling completions bash > ~/.local/share/bash-completion/completions/dotling
```

#### Zsh

```sh
dotling completions zsh > ~/.zfunc/_dotling
```

Add to `~/.zshrc` if not already present:

```sh
fpath=(~/.zfunc $fpath)
autoload -Uz compinit && compinit
```

#### Fish

```sh
dotling completions fish > ~/.config/fish/completions/dotling.fish
```

#### Elvish

```sh
dotling completions elvish > ~/.config/elvish/completions/dotling.elv
```

#### PowerShell

```sh
dotling completions powershell > dotling.ps1
```

Then source from your `$PROFILE`.

## Examples

```sh
# Generate bash completions
dotling completions bash > ~/.local/share/bash-completion/completions/dotling

# Generate zsh completions
dotling completions zsh > ~/.zfunc/_dotling

# Generate fish completions
dotling completions fish > ~/.config/fish/completions/dotling.fish
```
