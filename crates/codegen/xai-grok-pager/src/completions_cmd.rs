//! `grok completions <shell>` — generate shell completion scripts.
//!
//! Used by the installers and npm postinstall; must stay side-effect free
//! (no network, auth, tracing, or tokio).

use clap::CommandFactory as _;
use clap_complete::{Shell, generate};

use crate::app::PagerArgs;

/// Generate and print the completion script for the given shell.
pub fn run(shell: &str) {
    if matches!(shell, "cmd" | "pwsh") {
        eprintln!("Shell '{shell}' does not support completion script generation");
        return;
    }

    let shell: Shell = match shell.parse() {
        Ok(s) => s,
        Err(_) => {
            eprintln!("Unknown shell: '{shell}'. Supported: bash, zsh, fish, powershell, elvish");
            return;
        }
    };

    // Ensure the script always uses the public "grok" name (matches historical
    // behavior and what the installers + docs expect).
    let mut cmd = PagerArgs::command().name("grok");
    if shell != Shell::Zsh {
        generate(shell, &mut cmd, "grok", &mut std::io::stdout());
        return;
    }
    // zsh needs post-processing (see fix_zsh_root_prompt_positional).
    let mut buf = Vec::new();
    generate(shell, &mut cmd, "grok", &mut buf);
    match String::from_utf8(buf) {
        Ok(script) => print!("{}", fix_zsh_root_prompt_positional(&script)),
        Err(e) => {
            use std::io::Write as _;
            let _ = std::io::stdout().write_all(e.as_bytes());
        }
    }
}

/// Work around clap_complete's broken zsh output for an optional free-form
/// positional (`[PROMPT]`) preceding the subcommand slot
/// (<https://github.com/clap-rs/clap/issues/6282>).
///
/// The generated root `_arguments` spec emits a `'::prompt …'` slot before
/// the subcommand slot but dispatches subcommands with `case $line[2]`. zsh
/// assigns the typed subcommand to the *prompt* slot (`$line[1]`), leaves
/// `$line[2]` empty, and the dispatch falls through — so `grok worktree <TAB>`
/// re-offers every top-level command. (`hide = true` on the positional does
/// not change the generated script.)
///
/// Completing an arbitrary prompt string is useless, so drop the prompt slot
/// and shift the root dispatch to `$line[1]`. Nested subcommand dispatch
/// blocks already use `$line[1]` and are untouched; the three rewritten
/// patterns are unique to the root block (pinned by the test below — delete
/// this whole workaround once upstream fixes the generator).
fn fix_zsh_root_prompt_positional(script: &str) -> String {
    let mut out = String::with_capacity(script.len());
    script
        .lines()
        .filter(|line| !line.starts_with("'::prompt -- "))
        .for_each(|line| {
            out.push_str(line);
            out.push('\n');
        });
    for (from, to) in [
        (
            r#"words=($line[2] "${words[@]}")"#,
            r#"words=($line[1] "${words[@]}")"#,
        ),
        (
            r#"curcontext="${curcontext%:*:*}:grok-command-$line[2]:""#,
            r#"curcontext="${curcontext%:*:*}:grok-command-$line[1]:""#,
        ),
        (r#"case $line[2] in"#, r#"case $line[1] in"#),
    ] {
        out = out.replacen(from, to, 1);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generate the zsh completion script exactly like `run` does.
    fn zsh_script() -> String {
        let mut cmd = PagerArgs::command().name("grok");
        let mut buf = Vec::new();
        generate(Shell::Zsh, &mut cmd, "grok", &mut buf);
        String::from_utf8(buf).expect("completion script is UTF-8")
    }

    #[test]
    fn zsh_completions_drop_prompt_slot_and_dispatch_on_line_1() {
        let raw = zsh_script();
        assert!(raw.contains("'::prompt -- "), "raw script has prompt slot");
        assert!(
            raw.contains("case $line[2] in"),
            "raw root dispatch on $line[2]"
        );

        let fixed = fix_zsh_root_prompt_positional(&raw);
        assert!(
            !fixed.contains("::prompt"),
            "prompt positional must not appear in the emitted zsh script"
        );
        assert!(
            !fixed.contains("$line[2]"),
            "root dispatch must be shifted to $line[1]"
        );
        assert!(
            fixed.contains(r#"curcontext="${curcontext%:*:*}:grok-command-$line[1]:""#),
            "root dispatch context must use $line[1]"
        );
        assert!(
            fixed.contains("grok-worktree-command-$line[1]"),
            "nested subcommand dispatch must be untouched"
        );
        assert!(fixed.contains("_grok_commands"), "root command list intact");
    }

    #[test]
    fn cmd_returns_unsupported() {
        // Must not panic and must print an unsupported message.
        run("cmd");
        run("pwsh");
    }

    #[test]
    fn unknown_shell_returns_error() {
        run("nosuchshell");
    }
}
