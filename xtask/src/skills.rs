//! Keep repository skill bundles discoverable by both Claude and Codex.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn bundle_files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, directory: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    fs::read(&path).unwrap(),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}

#[test]
fn project_skill_bundles_match_for_claude_and_codex() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let claude = bundle_files(&root.join(".claude/skills"));
    let codex = bundle_files(&root.join(".agents/skills"));
    assert!(
        !claude.is_empty(),
        "project skills must remain discoverable"
    );
    let paths: std::collections::BTreeSet<_> = claude.keys().chain(codex.keys()).collect();
    for path in paths {
        assert!(
            claude.contains_key(path),
            "missing .claude/skills/{}",
            path.display()
        );
        assert!(
            codex.contains_key(path),
            "missing .agents/skills/{}",
            path.display()
        );
        assert!(
            claude[path] == codex[path],
            "skill copies differ: {}; update both discovery directories",
            path.display()
        );
    }
}
