use std::io;

use clap::CommandFactory;

use crate::{cli::Cli, error::Result};

/// Generate shell completion scripts to stdout.
pub fn run(shell: clap_complete::Shell) -> Result<()> {
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "dotling", &mut io::stdout());
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use crate::cli::Cli;

    fn generate_to_string(shell: clap_complete::Shell) -> String {
        let mut buf = Vec::new();
        let mut cmd = Cli::command();
        clap_complete::generate(shell, &mut cmd, "dotling", &mut buf);
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn generates_bash_completions() {
        let output = generate_to_string(clap_complete::Shell::Bash);
        assert!(output.contains("dotling"));
        assert!(output.contains("completions"));
    }

    #[test]
    fn generates_zsh_completions() {
        let output = generate_to_string(clap_complete::Shell::Zsh);
        assert!(output.contains("dotling"));
        assert!(output.contains("completions"));
    }

    #[test]
    fn generates_fish_completions() {
        let output = generate_to_string(clap_complete::Shell::Fish);
        assert!(output.contains("dotling"));
        assert!(output.contains("completions"));
    }

    #[test]
    fn generates_elvish_completions() {
        let output = generate_to_string(clap_complete::Shell::Elvish);
        assert!(output.contains("dotling"));
    }

    #[test]
    fn generates_powershell_completions() {
        let output = generate_to_string(clap_complete::Shell::PowerShell);
        assert!(output.contains("dotling"));
    }
}
