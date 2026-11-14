//! The default presets have one source, `crates/worker/src/default_presets.json`: the app reads it through
//! `presets::DEFAULT_PRESETS_JSON`, the server's fallback and the network test include the same file. A second
//! copy anywhere in the workspace, or a consumer that includes another path, fails here

use std::path::{Path, PathBuf};

const SOURCE: &str = "crates/worker/src/default_presets.json";

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn find(dir: &Path, name: &str, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read dir").flatten() {
        let path = entry.path();
        let file = entry.file_name();
        if path.is_dir() {
            if file == "target" || file == ".git" || file == "node_modules" {
                continue;
            }
            find(&path, name, out);
        } else if file == name {
            out.push(path);
        }
    }
}

#[test]
fn there_is_one_default_presets_file() {
    let root = workspace();
    let mut found = Vec::new();
    find(&root, "default_presets.json", &mut found);
    let rel: Vec<String> = found
        .iter()
        .map(|p| {
            p.strip_prefix(&root)
                .expect("under the workspace")
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    assert_eq!(
        rel,
        vec![SOURCE.to_string()],
        "copies of the default presets"
    );
}

#[test]
fn every_consumer_reads_the_one_source() {
    let root = workspace();
    let mut compared = 0;
    for (file, include) in [
        (
            "crates/worker/src/presets_handler.rs",
            "include_str!(\"default_presets.json\")",
        ),
        (
            "crates/network/src/presets_client.rs",
            "include_str!(\"../../worker/src/default_presets.json\")",
        ),
        (
            "crates/core/src/presets.rs",
            "include_str!(\"../../worker/src/default_presets.json\")",
        ),
    ] {
        let text = std::fs::read_to_string(root.join(file)).expect(file);
        assert!(
            text.contains(include),
            "{file} does not include the one source"
        );
        assert_eq!(
            text.matches("default_presets.json\")").count(),
            1,
            "{file} includes presets more than once"
        );
        compared += 1;
    }
    assert_eq!(compared, 3);
    // the bytes the app sees are the file's bytes
    let on_disk = std::fs::read_to_string(root.join(SOURCE)).expect("source");
    assert_eq!(text_to_print_core::presets::DEFAULT_PRESETS_JSON, on_disk);
    assert_eq!(text_to_print_core::prompt::DEFAULT_PRESETS_JSON, on_disk);
}
