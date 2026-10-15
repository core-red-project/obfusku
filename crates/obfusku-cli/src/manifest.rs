//! `obfusku.toml` (`ADR-020`, `ADR-029`): format `1` is the original
//! three-field marker, tolerant of unknown fields; format `2` adds the
//! project identity, explicit entry, bundle output, tests, capabilities
//! and dependencies, and rejects any key it does not define.

use obfusku_diagnostics::{Diagnostic, Severity, Span};
use std::path::Path;
use toml::{Table, Value};

pub const MANIFEST_FILE_NAME: &str = "obfusku.toml";
const SUPPORTED_FORMATS: [i64; 2] = [1, 2];

/// How the entry module is named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryRef {
    /// A bare module name, found by a whole-tree filename search (`ADR-018`).
    Module(String),
    /// A path relative to the project root.
    Path(String),
}

/// Where `⌁↓⌬`-family natives may read and write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsAccess {
    None,
    Project,
    Cwd,
}

/// What the program may use (`ADR-029`). Present means everything not
/// declared is denied; absent means everything is allowed.
// One flag per capability family is the manifest's own shape.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub console: bool,
    pub fs: FsAccess,
    pub env: bool,
    pub args: bool,
    pub clock: bool,
    pub random: bool,
    pub exit: bool,
}

impl Capabilities {
    pub const ALL: Capabilities = Capabilities {
        console: true,
        fs: FsAccess::Project,
        env: true,
        args: true,
        clock: true,
        random: true,
        exit: true,
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitRef {
    Tag(String),
    Rev(String),
    Branch(String),
}

impl GitRef {
    /// The spelling stored in `obfusku.lock`.
    #[must_use]
    pub fn key(&self) -> String {
        match self {
            GitRef::Tag(t) => format!("tag:{t}"),
            GitRef::Rev(r) => format!("rev:{r}"),
            GitRef::Branch(b) => format!("branch:{b}"),
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            GitRef::Tag(v) | GitRef::Rev(v) | GitRef::Branch(v) => v,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencySource {
    Path(String),
    Git { url: String, reference: GitRef },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub name: String,
    pub source: DependencySource,
}

#[derive(Debug, Clone, Default)]
pub struct ProjectInfo {
    pub version: Option<String>,
    pub description: Option<String>,
    pub license: Option<String>,
    pub authors: Vec<String>,
    pub obfusku: Option<String>,
}

/// A validated `obfusku.toml`.
#[derive(Debug, Clone)]
pub struct Manifest {
    pub format: i64,
    pub entry: EntryRef,
    pub name: Option<String>,
    pub project: ProjectInfo,
    pub bundle_output: Option<String>,
    pub test_include: Vec<String>,
    /// Seconds one test file may run before it is killed.
    pub test_timeout_secs: Option<u64>,
    pub capabilities: Option<Capabilities>,
    pub dependencies: Vec<Dependency>,
}

fn error(path: &Path, message: impl std::fmt::Display) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        message: format!("invalid manifest '{}': {message}", path.display()),
        primary: Span::default(),
    }
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != *cb)).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

/// Rejects a key not in `allowed`, suggesting the closest one.
fn check_keys(path: &Path, scope: &str, table: &Table, allowed: &[&str]) -> Result<(), Diagnostic> {
    for key in table.keys() {
        if allowed.contains(&key.as_str()) {
            continue;
        }
        let near = allowed
            .iter()
            .filter(|a| edit_distance(key, a) <= 2)
            .min_by_key(|a| edit_distance(key, a));
        let hint = near.map_or_else(
            || format!(" (allowed: {})", allowed.join(", ")),
            |n| format!(" — did you mean '{n}'?"),
        );
        let place = if scope.is_empty() {
            "at the top level".to_string()
        } else {
            format!("in [{scope}]")
        };
        return Err(error(path, format!("unknown key '{key}' {place}{hint}")));
    }
    Ok(())
}

fn string_field(
    path: &Path,
    scope: &str,
    t: &Table,
    key: &str,
) -> Result<Option<String>, Diagnostic> {
    match t.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(error(
            path,
            format!("'{key}' in [{scope}] must be a string"),
        )),
    }
}

fn bool_field(path: &Path, t: &Table, key: &str, default: bool) -> Result<bool, Diagnostic> {
    match t.get(key) {
        None => Ok(default),
        Some(Value::Boolean(b)) => Ok(*b),
        Some(_) => Err(error(
            path,
            format!("'{key}' in [capabilities] must be true or false"),
        )),
    }
}

fn table_of<'a>(path: &Path, v: &'a Value, name: &str) -> Result<&'a Table, Diagnostic> {
    v.as_table()
        .ok_or_else(|| error(path, format!("'{name}' must be a table")))
}

fn string_list(path: &Path, scope: &str, t: &Table, key: &str) -> Result<Vec<String>, Diagnostic> {
    match t.get(key) {
        None => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| {
                v.as_str().map(str::to_string).ok_or_else(|| {
                    error(
                        path,
                        format!("'{key}' in [{scope}] must be a list of strings"),
                    )
                })
            })
            .collect(),
        Some(_) => Err(error(
            path,
            format!("'{key}' in [{scope}] must be a list of strings"),
        )),
    }
}

/// `X.Y[.Z]` as a comparable triple.
pub(crate) fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let mut parts = text.trim().split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    let patch = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

fn check_obfusku_version(path: &Path, requirement: &str) -> Result<(), Diagnostic> {
    let wanted = requirement.trim().trim_start_matches(">=").trim();
    let min = parse_version(wanted).ok_or_else(|| {
        error(
            path,
            format!("'obfusku' must be a version such as \">=1.1.0\", found '{requirement}'"),
        )
    })?;
    let running = env!("CARGO_PKG_VERSION");
    if parse_version(running).is_some_and(|have| have < min) {
        return Err(error(
            path,
            format!("this project requires obfusku {requirement}, but this binary is {running}"),
        ));
    }
    Ok(())
}

fn valid_dependency_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn parse_dependency(path: &Path, name: &str, v: &Value) -> Result<Dependency, Diagnostic> {
    if !valid_dependency_name(name) {
        return Err(error(
            path,
            format!("dependency name '{name}' may only use letters, digits, '_' and '-'"),
        ));
    }
    let table = v.as_table().ok_or_else(|| {
        error(
            path,
            format!("dependency '{name}' must be a table, e.g. {{ github = \"owner/repo\", tag = \"v1.0.0\" }}"),
        )
    })?;
    check_keys(
        path,
        &format!("dependencies.{name}"),
        table,
        &["path", "git", "github", "gitlab", "tag", "rev", "branch"],
    )?;
    let scope = format!("dependencies.{name}");
    let location = ["path", "git", "github", "gitlab"]
        .into_iter()
        .filter(|k| table.contains_key(*k))
        .collect::<Vec<_>>();
    let [kind] = location.as_slice() else {
        return Err(error(
            path,
            format!("dependency '{name}' needs exactly one of path, git, github or gitlab"),
        ));
    };
    let value = string_field(path, &scope, table, kind)?.unwrap_or_default();
    let refs: Vec<GitRef> = [
        ("tag", GitRef::Tag as fn(String) -> GitRef),
        ("rev", GitRef::Rev),
        ("branch", GitRef::Branch),
    ]
    .into_iter()
    .filter_map(|(k, make)| match table.get(k) {
        Some(Value::String(s)) => Some(make(s.clone())),
        _ => None,
    })
    .collect();
    if *kind == "path" {
        if !refs.is_empty() {
            return Err(error(
                path,
                format!("dependency '{name}' is a path; tag, rev and branch apply to git sources"),
            ));
        }
        return Ok(Dependency {
            name: name.to_string(),
            source: DependencySource::Path(value),
        });
    }
    let url = match *kind {
        "github" | "gitlab" => {
            if !valid_repo_slug(&value) {
                return Err(error(
                    path,
                    format!("dependency '{name}': '{value}' is not an 'owner/repo' path"),
                ));
            }
            let host = if *kind == "github" {
                "github.com"
            } else {
                "gitlab.com"
            };
            format!("https://{host}/{value}.git")
        }
        _ => {
            // A leading '-' would be read by git as an option (--upload-pack runs commands).
            if value.is_empty() || value.starts_with('-') || value.chars().any(char::is_control) {
                return Err(error(
                    path,
                    format!("dependency '{name}': '{value}' is not a valid git URL"),
                ));
            }
            value
        }
    };
    let [reference] = <[GitRef; 1]>::try_from(refs).map_err(|_| {
        error(
            path,
            format!("dependency '{name}' needs exactly one of tag, rev or branch"),
        )
    })?;
    Ok(Dependency {
        name: name.to_string(),
        source: DependencySource::Git { url, reference },
    })
}

fn valid_repo_slug(v: &str) -> bool {
    let ok = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-');
    v.split_once('/').is_some_and(|(o, r)| {
        [o, r]
            .iter()
            .all(|p| !p.is_empty() && !p.starts_with('-') && *p != ".." && p.chars().all(ok))
    })
}

fn parse_capabilities(path: &Path, t: &Table) -> Result<Capabilities, Diagnostic> {
    check_keys(
        path,
        "capabilities",
        t,
        &["console", "fs", "env", "args", "clock", "random", "exit"],
    )?;
    let fs = match t.get("fs") {
        None => FsAccess::None,
        Some(Value::String(s)) => match s.as_str() {
            "none" => FsAccess::None,
            "project" => FsAccess::Project,
            "cwd" => FsAccess::Cwd,
            other => {
                return Err(error(
                    path,
                    format!("'fs' must be \"none\", \"project\" or \"cwd\", found \"{other}\""),
                ));
            }
        },
        Some(_) => return Err(error(path, "'fs' in [capabilities] must be a string")),
    };
    Ok(Capabilities {
        console: bool_field(path, t, "console", false)?,
        fs,
        env: bool_field(path, t, "env", false)?,
        args: bool_field(path, t, "args", false)?,
        clock: bool_field(path, t, "clock", false)?,
        random: bool_field(path, t, "random", false)?,
        exit: bool_field(path, t, "exit", false)?,
    })
}

fn parse_v2(path: &Path, table: &Table) -> Result<Manifest, Diagnostic> {
    check_keys(
        path,
        "",
        table,
        &[
            "format",
            "project",
            "entry",
            "bundle",
            "test",
            "capabilities",
            "dependencies",
        ],
    )?;

    let mut project = ProjectInfo::default();
    let mut name = None;
    if let Some(v) = table.get("project") {
        let t = table_of(path, v, "project")?;
        check_keys(
            path,
            "project",
            t,
            &[
                "name",
                "version",
                "description",
                "license",
                "authors",
                "obfusku",
            ],
        )?;
        name = string_field(path, "project", t, "name")?;
        project.version = string_field(path, "project", t, "version")?;
        if let Some(v) = &project.version
            && parse_version(v).is_none()
        {
            return Err(error(
                path,
                format!("'version' must look like 1.2.3, found '{v}'"),
            ));
        }
        project.description = string_field(path, "project", t, "description")?;
        project.license = string_field(path, "project", t, "license")?;
        project.authors = string_list(path, "project", t, "authors")?;
        project.obfusku = string_field(path, "project", t, "obfusku")?;
        if let Some(req) = &project.obfusku {
            check_obfusku_version(path, req)?;
        }
    }

    let entry = match table.get("entry") {
        Some(Value::String(s)) => EntryRef::Module(s.clone()),
        Some(Value::Table(t)) => {
            check_keys(path, "entry", t, &["module", "path"])?;
            match (
                string_field(path, "entry", t, "module")?,
                string_field(path, "entry", t, "path")?,
            ) {
                (Some(m), None) => EntryRef::Module(m),
                (None, Some(p)) => EntryRef::Path(p),
                _ => {
                    return Err(error(
                        path,
                        "[entry] needs exactly one of 'module' or 'path'",
                    ));
                }
            }
        }
        _ => {
            return Err(error(
                path,
                "missing 'entry' (a module name, or [entry] with module/path)",
            ));
        }
    };
    if let EntryRef::Module(m) = &entry
        && (m.contains('/')
            || m.contains('\\')
            || Path::new(m).extension().is_some_and(|e| e == "obk"))
    {
        return Err(error(
            path,
            format!("entry module '{m}' must be a bare name; use [entry] path = \"…\" for a file"),
        ));
    }

    let bundle_output = match table.get("bundle") {
        None => None,
        Some(v) => {
            let t = table_of(path, v, "bundle")?;
            check_keys(path, "bundle", t, &["output"])?;
            string_field(path, "bundle", t, "output")?
        }
    };
    let (test_include, test_timeout_secs) = match table.get("test") {
        None => (Vec::new(), None),
        Some(v) => {
            let t = table_of(path, v, "test")?;
            check_keys(path, "test", t, &["include", "timeout"])?;
            let timeout = match t.get("timeout") {
                None => None,
                Some(Value::Integer(n)) if *n > 0 => Some(n.unsigned_abs()),
                Some(_) => {
                    return Err(error(
                        path,
                        "'timeout' in [test] must be a positive number of seconds".to_string(),
                    ));
                }
            };
            (string_list(path, "test", t, "include")?, timeout)
        }
    };
    let capabilities = match table.get("capabilities") {
        None => None,
        Some(v) => Some(parse_capabilities(
            path,
            table_of(path, v, "capabilities")?,
        )?),
    };
    let mut dependencies = Vec::new();
    if let Some(v) = table.get("dependencies") {
        for (dep_name, spec) in table_of(path, v, "dependencies")? {
            dependencies.push(parse_dependency(path, dep_name, spec)?);
        }
    }

    Ok(Manifest {
        format: 2,
        entry,
        name,
        project,
        bundle_output,
        test_include,
        test_timeout_secs,
        capabilities,
        dependencies,
    })
}

/// Reads and validates `<root>/obfusku.toml`. Malformed TOML, a missing
/// or unsupported `format`, or a missing entry are all the same class of
/// error (`ADR-020`) — never silently treated as "no manifest."
pub fn load_manifest(root: &Path) -> Result<Manifest, Diagnostic> {
    let path = root.join(MANIFEST_FILE_NAME);
    let text =
        std::fs::read_to_string(&path).map_err(|e| error(&path, format!("could not read: {e}")))?;
    parse_manifest(&path, &text)
}

/// [`load_manifest`] over text already in hand (a bundle's embedded copy);
/// `path` only labels diagnostics.
pub fn parse_manifest(path: &Path, text: &str) -> Result<Manifest, Diagnostic> {
    let value: Value = text
        .parse()
        .map_err(|e| error(path, format!("malformed TOML: {e}")))?;
    let table = value
        .as_table()
        .ok_or_else(|| error(path, "must be a TOML table at the top level"))?;
    let format = table
        .get("format")
        .and_then(Value::as_integer)
        .ok_or_else(|| error(path, "missing or non-integer 'format' field"))?;
    if !SUPPORTED_FORMATS.contains(&format) {
        return Err(error(
            path,
            format!(
                "unsupported manifest format {format} (this obfusku binary supports formats 1 and 2)"
            ),
        ));
    }
    if format == 2 {
        return parse_v2(path, table);
    }
    // Format 1: tolerant of unknown fields (`ADR-020`).
    let entry = table
        .get("entry")
        .and_then(Value::as_str)
        .ok_or_else(|| error(path, "missing or non-string 'entry' field"))?;
    Ok(Manifest {
        format,
        entry: EntryRef::Module(entry.to_string()),
        name: table
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string),
        project: ProjectInfo::default(),
        bundle_output: None,
        test_include: Vec::new(),
        test_timeout_secs: None,
        capabilities: None,
        dependencies: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn load(text: &str) -> Result<Manifest, Diagnostic> {
        let dir = std::env::temp_dir().join(format!(
            "obk-manifest-{}-{}",
            std::process::id(),
            text.len() + text.bytes().map(usize::from).sum::<usize>()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(MANIFEST_FILE_NAME), text).unwrap();
        let out = load_manifest(&dir);
        let _ = std::fs::remove_dir_all(PathBuf::from(&dir));
        out
    }

    #[test]
    fn git_urls_that_git_would_read_as_options_are_rejected() {
        for bad in [
            "x = { git = \"--upload-pack=touch /tmp/p\", tag = \"v1\" }",
            "x = { github = \"../evil\", tag = \"v1\" }",
            "x = { github = \"-o/r\", tag = \"v1\" }",
        ] {
            let text = format!("format = 2\nentry = \"m\"\n[dependencies]\n{bad}\n");
            assert!(load(&text).is_err(), "{bad}");
        }
    }

    #[test]
    fn format_one_stays_tolerant() {
        let m = load("format = 1\nentry = \"main\"\nwhatever = true\n").unwrap();
        assert_eq!(m.entry, EntryRef::Module("main".into()));
    }

    #[test]
    fn format_two_rejects_unknown_keys_with_a_suggestion() {
        let e = load("format = 2\nentrty = \"main\"\n").unwrap_err();
        assert!(e.message.contains("did you mean"), "{}", e.message);
        let e = load("format = 2\nentry = \"main\"\n[bundle]\noutptu = \"x\"\n").unwrap_err();
        assert!(e.message.contains("output"), "{}", e.message);
    }

    #[test]
    fn format_two_full_example_parses() {
        let m = load(
            "format = 2\n[project]\nname = \"ouro\"\nversion = \"0.1.0\"\nobfusku = \">=1.0\"\n\
             [entry]\npath = \"src/main.obk\"\n[bundle]\noutput = \"ouro\"\n\
             [capabilities]\nconsole = true\nfs = \"cwd\"\nargs = true\n\
             [dependencies]\nutils = { github = \"o/r\", tag = \"v1\" }\nloc = { path = \"../loc\" }\n",
        )
        .unwrap();
        assert_eq!(m.name.as_deref(), Some("ouro"));
        assert_eq!(m.entry, EntryRef::Path("src/main.obk".into()));
        let caps = m.capabilities.unwrap();
        assert!(caps.console && caps.args && !caps.env);
        assert_eq!(caps.fs, FsAccess::Cwd);
        assert_eq!(m.dependencies.len(), 2);
    }

    #[test]
    fn a_dependency_needs_one_location_and_one_reference() {
        assert!(
            load("format = 2\nentry = \"m\"\n[dependencies]\nx = { github = \"o/r\" }\n").is_err()
        );
        assert!(
            load("format = 2\nentry = \"m\"\n[dependencies]\nx = { path = \"a\", git = \"u\", tag = \"v\" }\n")
                .is_err()
        );
    }

    #[test]
    fn a_too_new_interpreter_requirement_is_refused() {
        let e = load("format = 2\nentry = \"m\"\n[project]\nobfusku = \">=99.0\"\n").unwrap_err();
        assert!(e.message.contains("requires obfusku"), "{}", e.message);
    }

    #[test]
    fn versions_compare_as_triples() {
        assert_eq!(parse_version("1.2"), Some((1, 2, 0)));
        assert!(parse_version("1.x").is_none());
    }
}
