//! `completions`: shell completion scripts.
//!
//! Generated from [`COMMANDS`] and
//! [`GLOBAL_FLAGS`] rather than written per shell, so
//! the four scripts and the usage text cannot disagree.

use super::{COMMANDS, GLOBAL_FLAGS};
use std::fmt::Write as _;
use std::process::ExitCode;

/// Shells a completion script exists for.
const SHELLS: &[&str] = &["bash", "zsh", "fish", "powershell"];

/// A completion script for `shell`, or `None` for an unsupported shell.
fn script(shell: &str) -> Option<String> {
    let commands = COMMANDS.join(" ");
    let flags = GLOBAL_FLAGS.join(" ");
    let shells = SHELLS.join(" ");
    Some(match shell {
        "bash" => format!(
            "_obfusku() {{\n    local cur=\"${{COMP_WORDS[COMP_CWORD]}}\"\n    \
             if [ \"$COMP_CWORD\" -eq 1 ]; then\n        \
             COMPREPLY=( $(compgen -W \"{commands} {flags}\" -- \"$cur\") )\n    \
             elif [ \"${{COMP_WORDS[1]}}\" = completions ]; then\n        \
             COMPREPLY=( $(compgen -W \"{shells}\" -- \"$cur\") )\n    else\n        \
             COMPREPLY=( $(compgen -f -W \"{flags}\" -- \"$cur\") )\n    fi\n}}\n\
             complete -o filenames -F _obfusku obfusku\n"
        ),
        "zsh" => format!(
            "#compdef obfusku\n_obfusku() {{\n    \
             if (( CURRENT == 2 )); then\n        \
             compadd {commands} {flags}\n    \
             elif [[ $words[2] == completions ]]; then\n        compadd {shells}\n    \
             else\n        _files\n    fi\n}}\n_obfusku \"$@\"\n"
        ),
        "fish" => {
            let mut s = String::new();
            for c in COMMANDS {
                let _ = writeln!(s, "complete -c obfusku -n __fish_use_subcommand -a {c}");
            }
            let _ = writeln!(
                s,
                "complete -c obfusku -n '__fish_seen_subcommand_from completions' -a '{shells}'"
            );
            for f in GLOBAL_FLAGS.iter().filter(|f| f.starts_with("--")) {
                let _ = writeln!(s, "complete -c obfusku -l {}", &f[2..]);
            }
            s
        }
        "powershell" => format!(
            "Register-ArgumentCompleter -Native -CommandName obfusku -ScriptBlock {{\n    \
             param($wordToComplete, $commandAst, $cursorPosition)\n    \
             $words = $commandAst.CommandElements | ForEach-Object {{ $_.ToString() }}\n    \
             $candidates = if ($words.Count -le 2) {{ '{commands} {flags}' -split ' ' }} \
             elseif ($words[1] -eq 'completions') {{ '{shells}' -split ' ' }} \
             else {{ '{flags}' -split ' ' }}\n    \
             $candidates | Where-Object {{ $_ -like \"$wordToComplete*\" }} | \
             ForEach-Object {{ [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_) }}\n}}\n"
        ),
        _ => return None,
    })
}

/// `completions <shell>`: a completion script on stdout.
pub(super) fn run(args: &[String]) -> ExitCode {
    let Some(shell) = args.get(2) else {
        eprintln!(
            "expected a shell: bash, zsh, fish or powershell\nusage: obfusku completions <shell>"
        );
        return ExitCode::from(2);
    };
    if let Some(text) = script(shell) {
        print!("{text}");
        ExitCode::SUCCESS
    } else {
        eprintln!("error: unsupported shell '{shell}'\nexpected bash, zsh, fish or powershell");
        ExitCode::from(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_shell_has_a_script_and_others_none() {
        for s in SHELLS {
            assert!(script(s).is_some_and(|t| t.contains("obfusku")), "{s}");
        }
        assert!(script("tcsh").is_none());
    }
}
