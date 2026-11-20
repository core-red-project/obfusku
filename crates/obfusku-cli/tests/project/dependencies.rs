//! Path and git dependencies, and the lock that makes `run` work offline (`ADR-029`).

use crate::support::{TempTree, bin};

#[test]
fn a_path_dependency_supplies_modules_and_a_bundle_carries_them() {
    let tree = TempTree::new();
    tree.write("greet/obfusku.toml", "format = 2\nentry = \"greet\"\n");
    tree.write("greet/greet.obk", "word ≔⟳ \"from-dep\"\n❧\n");
    tree.write("greet/tests/greet.obk", "word ≔⟳ \"WRONG\"\n❧\n");
    tree.write(
        "app/obfusku.toml",
        "format = 2\nentry = \"main\"\n[bundle]\noutput = \"appbin\"\n\
         [dependencies]\ngreet = { path = \"../greet\" }\n",
    );
    tree.write("app/main.obk", "⟲ greet\nr ≔ ⌁↑(word)\n❧\n");
    let app = tree.root().join("app");

    let run = bin().arg("run").arg(&app).output().unwrap();
    assert!(run.status.success(), "{run:?}");
    assert!(String::from_utf8_lossy(&run.stdout).contains("from-dep"));

    let out = bin()
        .arg("bundle")
        .arg(&app)
        .current_dir(tree.root())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let exe = tree
        .root()
        .join(format!("appbin{}", std::env::consts::EXE_SUFFIX));
    assert!(exe.exists());
    // The dependency's sources travel inside the executable.
    std::fs::remove_dir_all(tree.root().join("greet")).unwrap();
    let ran = std::process::Command::new(&exe).output().unwrap();
    assert!(ran.status.success(), "{ran:?}");
    assert_eq!(String::from_utf8_lossy(&ran.stdout).trim(), "from-dep");
}

#[test]
fn fetch_pins_a_git_dependency_and_run_works_offline_from_the_lock() {
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let tree = TempTree::new();
    let dep = tree.root().join("dep");
    std::fs::create_dir_all(&dep).unwrap();
    std::fs::write(dep.join("obfusku.toml"), "format = 2\nentry = \"util\"\n").unwrap();
    std::fs::write(dep.join("util.obk"), "word ≔⟳ \"pinned\"\n❧\n").unwrap();
    let git = |args: &[&str]| {
        let st = std::process::Command::new("git")
            .args([
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(&dep)
            .output()
            .unwrap();
        assert!(st.status.success(), "{st:?}");
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-qm", "init"]);
    git(&["tag", "v1.0.0"]);

    // TOML reads `\` as an escape, and a Windows path needs a third slash.
    let slashed = dep.display().to_string().replace('\\', "/");
    let url = if slashed.starts_with('/') {
        format!("file://{slashed}")
    } else {
        format!("file:///{slashed}")
    };
    tree.write(
        "app/obfusku.toml",
        &format!(
            "format = 2\nentry = \"main\"\n[dependencies]\nutil = {{ git = \"{url}\", tag = \"v1.0.0\" }}\n"
        ),
    );
    tree.write("app/main.obk", "⟲ util\nr ≔ word\n❧\n");
    let app = tree.root().join("app");
    let home = tree.root().join("home");

    let before = bin()
        .env("OBFUSKU_HOME", &home)
        .arg("run")
        .arg(&app)
        .output()
        .unwrap();
    assert_eq!(before.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&before.stderr).contains("obfusku fetch"));

    let fetched = bin()
        .env("OBFUSKU_HOME", &home)
        .arg("fetch")
        .arg(&app)
        .output()
        .unwrap();
    assert!(fetched.status.success(), "{fetched:?}");
    let lock = std::fs::read_to_string(app.join("obfusku.lock")).unwrap();
    assert!(
        lock.contains("tag:v1.0.0") && lock.contains("rev = \""),
        "{lock}"
    );

    // The origin disappears; the pinned copy in the cache is enough.
    std::fs::remove_dir_all(&dep).unwrap();
    let after = bin()
        .env("OBFUSKU_HOME", &home)
        .arg("run")
        .arg(&app)
        .output()
        .unwrap();
    assert!(after.status.success(), "{after:?}");
    assert_eq!(String::from_utf8_lossy(&after.stdout).trim(), "\"pinned\"");
}
