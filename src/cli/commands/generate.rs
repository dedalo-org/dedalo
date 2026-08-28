//! Completion scripts and the manual page.
//!
//! Both are derived from the same [`Cli`] definition that parses arguments, so
//! neither can describe a flag that does not exist or miss one that does. That
//! is the whole reason to generate them rather than to write them: `dedalo`
//! has subcommands whose flags change what happens to money — `--execute`,
//! `--allow-undistributed`, `--since` — and a completion list that has drifted
//! from the parser is worse than none, because it is confidently wrong.
//!
//! Neither is installed by anything. `install.sh` prints the command; it does
//! not write into a shell's configuration, because a script that edits a
//! user's dotfiles unasked is the reason people stop piping scripts to `sh`.

use std::io;

use anyhow::{Context, Result};
use clap::CommandFactory;

use crate::cli::args::{Cli, CompletionsArgs};

/// Write a completion script for one shell to stdout.
pub fn completions(args: &CompletionsArgs) -> Result<()> {
    let mut command = Cli::command();
    clap_complete::generate(args.shell, &mut command, "dedalo", &mut io::stdout().lock());
    Ok(())
}

/// Write the manual page, in roff, to stdout.
pub fn man() -> Result<()> {
    clap_mangen::Man::new(Cli::command())
        .render(&mut io::stdout().lock())
        .context("cannot write the manual page")
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap_complete::Shell;

    /// Every shell we claim to support has to actually generate.
    ///
    /// `clap_complete` panics rather than returning an error when a command
    /// tree is something a given shell's generator cannot express — a
    /// subcommand name a shell function cannot be named after, for instance.
    /// So the assertion that matters is that this does not unwind.
    #[test]
    fn every_shell_generates_a_non_empty_script() {
        for shell in [
            Shell::Bash,
            Shell::Zsh,
            Shell::Fish,
            Shell::PowerShell,
            Shell::Elvish,
        ] {
            let mut command = Cli::command();
            let mut out = Vec::new();
            clap_complete::generate(shell, &mut command, "dedalo", &mut out);
            assert!(
                out.len() > 100,
                "{shell} produced {} bytes, which is not a completion script",
                out.len()
            );
            assert!(
                String::from_utf8_lossy(&out).contains("dedalo"),
                "{shell} produced a script that never names the binary"
            );
        }
    }

    /// The completion list is only worth shipping if it knows the flags that
    /// decide what happens to money. If one of these is ever renamed, this
    /// test says so before a user's shell does.
    #[test]
    fn completions_know_the_flags_that_move_money() {
        let mut command = Cli::command();
        let mut out = Vec::new();
        clap_complete::generate(Shell::Bash, &mut command, "dedalo", &mut out);
        let script = String::from_utf8(out).expect("bash completions are utf-8");

        for flag in ["--execute", "--allow-undistributed", "--since", "--amount"] {
            assert!(script.contains(flag), "bash completions omit {flag}");
        }
    }

    /// A man page that does not name the commands is a man page nobody needs.
    #[test]
    fn the_man_page_renders_and_names_the_commands() {
        let mut out = Vec::new();
        clap_mangen::Man::new(Cli::command())
            .render(&mut out)
            .expect("rendering to a Vec cannot fail");
        let page = String::from_utf8(out).expect("roff is utf-8");

        assert!(page.starts_with(".ie"), "not a roff document: {:.40}", page);
        assert!(page.contains("dedalo"));
        // `hide = true` keeps these out of the rendered page, which is
        // deliberate — but the commands a reader came for must be in it.
        for command in ["plan", "settle", "verify", "propose"] {
            assert!(page.contains(command), "man page omits `{command}`");
        }
    }
}
