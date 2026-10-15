//! The CLI's machine-facing surface: `--json` diagnostics and shell
//! completions. Presentation only, like the rest of this crate.

use obfusku_diagnostics::{Diagnostic, Severity, SourceMap};
use std::fmt::Write as _;

/// Subcommands and their flags; the single list every completion script
/// and the usage text agree on.
const COMMANDS: &[&str] = &[
    "run",
    "check",
    "fmt",
    "inspect",
    "build",
    "bundle",
    "fetch",
    "test",
    "repl",
    "completions",
    "version",
];
const GLOBAL_FLAGS: &[&str] = &["-h", "--help", "-V", "--version", "-q", "--quiet", "--json"];
const SHELLS: &[&str] = &["bash", "zsh", "fish", "powershell"];

fn escape_json(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
}

/// `{"ok":…,"diagnostics":[{"severity","message","source","line","column",
/// "end_line","end_column"}]}` on one line; `source` is the id of the file
/// the span belongs to, positions are 1-based and the end is exclusive.
#[must_use]
pub fn diagnostics_json(map: &SourceMap, ok: bool, diagnostics: &[Diagnostic]) -> String {
    let mut out = format!("{{\"ok\":{ok},\"diagnostics\":[");
    for (i, d) in diagnostics.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let severity = match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        };
        let (at, end) = map.span_location(d.primary);
        let _ = write!(out, "{{\"severity\":\"{severity}\",\"message\":\"");
        escape_json(&d.message, &mut out);
        let _ = write!(
            out,
            "\",\"source\":{},\"line\":{},\"column\":{},\"end_line\":{},\"end_column\":{}}}",
            d.primary.source.0, at.line, at.column, end.line, end.column
        );
    }
    out.push_str("]}");
    out
}

/// A completion script for `shell`, or `None` for an unsupported shell.
#[must_use]
pub fn completions(shell: &str) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use obfusku_diagnostics::Span;

    #[test]
    fn json_escapes_and_locates() {
        let mut map = SourceMap::new();
        let id = map.add_file("ab\ncd");
        let d = Diagnostic {
            severity: Severity::Error,
            message: "say \"hi\"\n".to_string(),
            primary: Span {
                source: id,
                start: 4,
                end: 5,
            },
        };
        assert_eq!(
            diagnostics_json(&map, false, &[d]),
            "{\"ok\":false,\"diagnostics\":[{\"severity\":\"error\",\
             \"message\":\"say \\\"hi\\\"\\n\",\"source\":0,\"line\":2,\"column\":2,\
             \"end_line\":2,\"end_column\":3}]}"
        );
    }

    #[test]
    fn every_listed_shell_has_a_script_and_others_none() {
        for s in SHELLS {
            assert!(completions(s).is_some_and(|t| t.contains("obfusku")), "{s}");
        }
        assert!(completions("tcsh").is_none());
    }
}
