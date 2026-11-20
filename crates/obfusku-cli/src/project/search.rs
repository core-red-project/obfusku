//! Finding `.obk` files under a Project root.
//!
//! Imports are bare module names (`ADR-018`), so resolving one is a search
//! by file name through the whole Project tree rather than a path lookup.
//! That makes *ambiguity* — the same name in two directories — a real
//! outcome the resolver must report instead of silently picking by
//! directory order. The walk never follows symlinks (no cycles, no escaping
//! the root) and skips hidden directories and Cargo's `target`.

use std::path::{Path, PathBuf};

/// The outcome of searching a Project tree for a module by bare name
/// (`ADR-018`): a whole-tree search by filename, not a same-directory
/// lookup — `ModuleName` stays a single `Ident`, so ambiguity (the same
/// base name present under two different subdirectories) is a real
/// possibility this must report, not silently resolve by search order.
pub enum ModuleSearch {
    /// Exactly one match.
    Found(PathBuf),
    /// No file with that name.
    NotFound,
    /// The same name under several directories.
    Ambiguous(Vec<PathBuf>),
}

/// Searches every `.obk` file reachable under `root` for one whose
/// filename (minus `.obk`) equals `name`.
#[must_use]
pub fn find_module(root: &Path, name: &str) -> ModuleSearch {
    find_module_skipping(root, name, &[])
}

/// [`find_module`] that does not descend into directories named in `skip`
/// (a dependency's tests and examples are not importable modules).
#[must_use]
pub fn find_module_skipping(root: &Path, name: &str, skip: &[&str]) -> ModuleSearch {
    let mut matches = Vec::new();
    walk_obk_files_skipping(root, skip, &mut |path| {
        if path.file_stem().and_then(|s| s.to_str()) == Some(name) {
            matches.push(path.to_path_buf());
        }
    });
    match matches.len() {
        0 => ModuleSearch::NotFound,
        1 => ModuleSearch::Found(matches.into_iter().next().expect("len checked above")),
        _ => ModuleSearch::Ambiguous(matches),
    }
}

/// Does not follow symlinks (no cycles, no escaping the root) and skips
/// hidden directories and Cargo's `target`.
pub(crate) fn walk_obk_files(dir: &Path, visit: &mut impl FnMut(&Path)) {
    walk_obk_files_skipping(dir, &[], visit);
}

/// Visits every `.obk` file under `dir`, skipping hidden directories, Cargo's
/// `target` and any directory named in `skip`. Symlinks are not followed.
fn walk_obk_files_skipping(dir: &Path, skip: &[&str], visit: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with('.') && name != "target" && !skip.contains(&&*name) {
                walk_obk_files_skipping(&path, skip, visit);
            }
        } else if kind.is_file() && path.extension().and_then(|e| e.to_str()) == Some("obk") {
            visit(&path);
        }
    }
}

/// `*` matches within one path segment, `**` any number of segments.
fn glob_match(pattern: &[&str], path: &[&str]) -> bool {
    fn segment(p: &str, s: &str) -> bool {
        match p.split_once('*') {
            None => p == s,
            Some((head, tail)) => {
                let Some(rest) = s.strip_prefix(head) else {
                    return false;
                };
                (0..=rest.len())
                    .filter(|i| rest.is_char_boundary(*i))
                    .any(|i| segment(tail, &rest[i..]))
            }
        }
    }
    match pattern.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => (0..=path.len()).any(|i| glob_match(rest, &path[i..])),
        Some((p, rest)) => path
            .split_first()
            .is_some_and(|(s, tail)| segment(p, s) && glob_match(rest, tail)),
    }
}

/// The `.obk` files under `root` matching any of `patterns` (relative,
/// `/`-separated, with `*` and `**`), in path order. Hidden directories and
/// `target` are never searched.
#[must_use]
pub fn matching_files(root: &Path, patterns: &[String]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    walk_obk_files(root, &mut |p| {
        let Ok(rel) = p.strip_prefix(root) else {
            return;
        };
        let parts: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
        let hit = patterns.iter().any(|pat| {
            let pat: Vec<&str> = pat.split('/').collect();
            glob_match(&pat, &parts)
        });
        if hit {
            found.push(p.to_path_buf());
        }
    });
    found.sort();
    found
}

#[cfg(test)]
mod glob_tests {
    use super::glob_match;

    fn m(p: &str, s: &str) -> bool {
        glob_match(
            &p.split('/').collect::<Vec<_>>(),
            &s.split('/').collect::<Vec<_>>(),
        )
    }

    #[test]
    fn star_stays_in_a_segment_and_double_star_crosses() {
        assert!(m("tests/*.obk", "tests/a.obk"));
        assert!(!m("tests/*.obk", "tests/sub/a.obk"));
        assert!(m("tests/**/*.obk", "tests/a.obk"));
        assert!(m("tests/**/*.obk", "tests/x/y/a.obk"));
        assert!(!m("tests/**/*.obk", "src/a.obk"));
        assert!(m("**/*_test.obk", "a/b/c_test.obk"));
        assert!(m("t*t.obk", "test.obk"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::scratch::Scratch;

    #[test]
    fn a_unique_module_is_found_by_bare_name_anywhere_in_the_tree() {
        let s = Scratch::new();
        let file = s.write("deep/er/util.obk", "❧");
        assert!(matches!(find_module(s.root(), "util"), ModuleSearch::Found(p) if p == file));
    }

    #[test]
    fn a_missing_module_is_not_found() {
        let s = Scratch::new();
        s.write("a.obk", "❧");
        assert!(matches!(find_module(s.root(), "b"), ModuleSearch::NotFound));
    }

    #[test]
    fn the_same_name_in_two_directories_is_ambiguous() {
        let s = Scratch::new();
        s.write("x/dup.obk", "❧");
        s.write("y/dup.obk", "❧");
        let ModuleSearch::Ambiguous(paths) = find_module(s.root(), "dup") else {
            panic!("expected ambiguity");
        };
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn hidden_and_target_directories_are_never_searched() {
        let s = Scratch::new();
        s.write(".hidden/m.obk", "❧");
        s.write("target/m.obk", "❧");
        assert!(matches!(find_module(s.root(), "m"), ModuleSearch::NotFound));
    }

    #[test]
    fn skipped_directories_hide_a_dependencys_tests() {
        let s = Scratch::new();
        s.write("tests/t.obk", "❧");
        assert!(matches!(
            find_module_skipping(s.root(), "t", &["tests"]),
            ModuleSearch::NotFound
        ));
        assert!(matches!(find_module(s.root(), "t"), ModuleSearch::Found(_)));
    }

    #[test]
    fn only_obk_files_count() {
        let s = Scratch::new();
        s.write("m.txt", "");
        s.write("m.obk.bak", "");
        assert!(matches!(find_module(s.root(), "m"), ModuleSearch::NotFound));
    }

    #[test]
    fn matching_files_applies_patterns_relative_to_the_root_in_path_order() {
        let s = Scratch::new();
        s.write("tests/b_test.obk", "❧");
        s.write("tests/a_test.obk", "❧");
        s.write("tests/sub/c_test.obk", "❧");
        s.write("src/main.obk", "❧");
        let flat = matching_files(s.root(), &["tests/*.obk".to_string()]);
        assert_eq!(
            flat,
            [
                s.root().join("tests/a_test.obk"),
                s.root().join("tests/b_test.obk")
            ]
        );
        let deep = matching_files(s.root(), &["tests/**/*.obk".to_string()]);
        assert_eq!(deep.len(), 3);
        assert!(matching_files(s.root(), &["nothing/**".to_string()]).is_empty());
    }
}
