//! Source-level guards: the app can only start processes in three reviewed places and never runs git.
const SRC: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src");

fn sources() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(SRC)];
    while let Some(p) = stack.pop() {
        for e in std::fs::read_dir(&p).unwrap() {
            let path = e.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|x| x == "rs") {
                let src = std::fs::read_to_string(&path).unwrap();
                // Only production code; test modules may mention anything.
                let code = src.split("#[cfg(test)]").next().unwrap().to_string();
                let rel = path.strip_prefix(SRC).unwrap().to_string_lossy().replace('\\', "/");
                out.push((rel, code));
            }
        }
    }
    out
}

#[test]
fn processes_are_spawned_only_in_reviewed_files() {
    let allowed = ["gitlab/runner.rs", "agents/runner.rs", "platform.rs"];
    for (rel, code) in sources() {
        if code.contains("Command::new") {
            assert!(allowed.contains(&rel.as_str()), "{rel} spawns a process");
        }
    }
}

#[test]
fn never_runs_git_or_writes_to_gitlab() {
    for (rel, code) in sources() {
        for bad in [
            "Command::new(\"git\")",
            "\"push\"",
            "\"commit\"",
            "--method POST",
            "Method::POST",
            "reqwest",
            "ureq",
        ] {
            assert!(!code.contains(bad), "{rel} contains {bad}");
        }
    }
}

#[test]
fn platform_spawns_only_shell_lookup_and_url_opener() {
    let (_, code) = sources().into_iter().find(|(r, _)| r == "platform.rs").unwrap();
    assert_eq!(
        code.matches("Command::new").count(),
        3,
        "login-shell lookup + URL opener + macOS reopen after update only"
    );
    assert!(
        code.contains("starts_with(\"https://\")"),
        "opener only takes http(s) URLs"
    );
}
