//! `cargo xtask rules`: every rule in `docs/rules/` records its sources
//! (ARCHITECTURE.md, Scheduling rules).
//!
//! A rule file is `docs/rules/TF-NNN-slug.md`, written from
//! `docs/rules/template.md`. Its Sources section has one entry per source,
//! each starting with a provenance class, and links to the bibliography in
//! `docs/rules/sources.md` by anchor, and the tests it lists exist.
//! `docs/rules/README.md` gives the same rules for people.

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

/// The rules folder, relative to the repository root.
const DIR: &str = "docs/rules";
/// The bibliography a rule's sources link to, in [`DIR`].
const SOURCES: &str = "sources.md";

/// The provenance classes of CONTRIBUTING.md, Calculation rules and their sources, in its
/// order of preference (`format-fact` is for format facts only).
const CLASSES: [&str; 5] = [
    "standard",
    "vendor-doc",
    "observed-file",
    "format-fact",
    "own-choice",
];

/// Files in [`DIR`] that aren't rules. Every other file there is checked as
/// one.
const NOT_RULES: [&str; 4] = [
    "README.md",
    "template.md",
    SOURCES,
    "open-source-schedulers.md",
];

/// Checks every rule in the repository and prints one line, or every
/// problem found.
pub fn run() -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let (checked, problems) = check_dir(&root.join(DIR), &root)?;
    if problems.is_empty() {
        println!("rules: {checked} rules in {DIR} checked, no problems");
        return Ok(());
    }
    for problem in &problems {
        eprintln!("{problem}");
    }
    Err(format!("rules: {} problems", problems.len()).into())
}

/// Checks every file in `dir` except [`NOT_RULES`] as a rule, against the
/// anchors in its `sources.md`. Returns how many rules were checked and the
/// problems found, each naming its file.
fn check_dir(dir: &Path, root: &Path) -> Result<(usize, Vec<String>), Box<dyn Error>> {
    let sources = dir.join(SOURCES);
    let text =
        fs::read_to_string(&sources).map_err(|error| format!("{}: {error}", sources.display()))?;
    let anchors = anchors(&text);
    let listed = crate::corpus::listed_ids(&root.join(crate::corpus::MANIFEST))?;
    let mut rules: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if path.is_file() && !NOT_RULES.contains(&name) {
            rules.push(path);
        }
    }
    rules.sort();
    let mut problems = Vec::new();
    let mut numbers = BTreeSet::new();
    for path in &rules {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if valid_name(name) && !numbers.insert(&name[..6]) {
            problems.push(format!(
                "{DIR}/{name}: another rule already uses {}; take the next free number",
                &name[..6]
            ));
        }
        let text =
            fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
        problems.extend(
            check_rule(name, &text, &anchors)
                .into_iter()
                .chain(unlisted_files(&text, &listed))
                .chain(check_tests(&text, root))
                .map(|problem| format!("{DIR}/{name}: {problem}")),
        );
    }
    Ok((rules.len(), problems))
}

/// The anchors `sources.md` declares, as `<a id="name"></a>`.
fn anchors(sources: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = sources;
    while let Some(at) = rest.find("<a id=\"") {
        rest = &rest[at + "<a id=\"".len()..];
        if let Some(end) = rest.find('"') {
            found.insert(rest[..end].to_owned());
            rest = &rest[end..];
        }
    }
    found
}

/// The problems with one rule file, named `name`, each saying what to fix.
fn check_rule(name: &str, text: &str, anchors: &BTreeSet<String>) -> Vec<String> {
    let mut problems = Vec::new();
    if !valid_name(name) {
        problems.push(
            "name it TF-NNN-slug.md: a three-digit number and lowercase words joined by hyphens"
                .to_owned(),
        );
    }
    match section(text, "Sources") {
        None => problems.push("no Sources section (## Sources)".to_owned()),
        Some(lines) => {
            let entries = entries(&lines);
            if entries.is_empty() {
                problems.push("the Sources section has no entries".to_owned());
            }
            for (index, entry) in entries.iter().enumerate() {
                problems.extend(
                    check_entry(entry, anchors)
                        .into_iter()
                        .map(|problem| format!("Sources entry {}: {problem}", index + 1)),
                );
            }
        }
    }
    problems
}

/// The tests a rule's Tests section names that don't exist. Each list item,
/// or the paragraph when there is no list, names a file, `` `path` ``, and
/// after `` `path`: `` its tests, `` `name` ``; text in parentheses is left
/// out. Each file must exist, a Rust file must define each name as a
/// function, and the section must name at least one file.
fn check_tests(text: &str, root: &Path) -> Vec<String> {
    let Some(lines) = section(text, "Tests") else {
        return vec!["no Tests section (## Tests)".to_owned()];
    };
    let mut items = entries(&lines);
    if items.is_empty() {
        items.push(lines.join(" "));
    }
    let mut problems = Vec::new();
    let mut files = 0;
    for item in items {
        let mut outside = String::new();
        let mut depth = 0_usize;
        for character in item.chars() {
            match character {
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                _ if depth == 0 => outside.push(character),
                _ => {}
            }
        }
        // The file named last, its code, and whether test names follow it.
        let mut file: Option<(String, Option<String>, bool)> = None;
        let mut pieces = outside.split('`');
        pieces.next();
        while let (Some(token), after) = (pieces.next(), pieces.next()) {
            if token.contains('/') && Path::new(token).extension().is_some() {
                let code = fs::read_to_string(root.join(token)).ok();
                if code.is_none() {
                    problems.push(format!("Tests names `{token}`, which doesn't exist"));
                }
                files += 1;
                let names = after.is_some_and(|a| a.trim_start().starts_with(':'))
                    && Path::new(token).extension().is_some_and(|e| e == "rs");
                file = Some((token.to_owned(), code, names));
            } else if let Some((path, Some(code), true)) = &file {
                let is_name = !token.is_empty()
                    && token
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
                if is_name
                    && !code.contains(&format!("fn {token}("))
                    && !code.contains(&format!("fn {token}<"))
                {
                    problems.push(format!(
                        "Tests names `{token}`, which `{path}` doesn't define"
                    ));
                }
            }
        }
    }
    if files == 0 {
        problems.push("the Tests section names no test file (`path`: `name`)".to_owned());
    }
    problems
}

/// The IDs with `prefix` and `digits` digits that `text` names, in order,
/// each once.
fn ids(text: &str, prefix: &str, digits: usize) -> Vec<String> {
    let mut found = Vec::new();
    for (at, _) in text.match_indices(prefix) {
        let before = text[..at].chars().next_back();
        let rest = &text[at + prefix.len()..];
        let number: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if !before.is_some_and(is_word) && number.len() == digits {
            let id = format!("{prefix}{number}");
            if !found.contains(&id) {
                found.push(id);
            }
        }
    }
    found
}

/// Corpus file IDs (`C-` and four digits) a rule names that
/// `corpus/manifest.toml` doesn't list.
fn unlisted_files(text: &str, listed: &BTreeSet<String>) -> Vec<String> {
    ids(text, "C-", 4)
        .into_iter()
        .filter(|id| !listed.contains(id))
        .map(|id| format!("names {id}, which corpus/manifest.toml doesn't list"))
        .collect()
}

/// Whether `name` is `TF-NNN-slug.md`.
fn valid_name(name: &str) -> bool {
    let Some(stem) = name
        .strip_prefix("TF-")
        .and_then(|rest| rest.strip_suffix(".md"))
    else {
        return false;
    };
    let Some((number, slug)) = stem.split_at_checked(3) else {
        return false;
    };
    let Some(slug) = slug.strip_prefix('-') else {
        return false;
    };
    number.bytes().all(|byte| byte.is_ascii_digit())
        && !slug.is_empty()
        && slug.split('-').all(|word| {
            !word.is_empty()
                && word
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

/// The lines of the `## heading` section, up to the next heading of level
/// one or two, leaving out code fences. `None` when there is no such section.
fn section<'a>(text: &'a str, heading: &str) -> Option<Vec<&'a str>> {
    let mut lines = None::<Vec<&str>>;
    let mut fenced = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let is_heading = line.starts_with("# ") || line.starts_with("## ");
        match &mut lines {
            Some(found) if is_heading => return Some(std::mem::take(found)),
            Some(found) => found.push(line),
            None if is_heading && line.strip_prefix("## ").map(str::trim) == Some(heading) => {
                lines = Some(Vec::new());
            }
            None => {}
        }
    }
    lines
}

/// The entries of a list: each top-level `- ` item with its continuation
/// lines joined by spaces.
fn entries(lines: &[&str]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for line in lines {
        if let Some(item) = line.strip_prefix("- ") {
            found.push(item.trim().to_owned());
        } else if let Some(last) = found.last_mut().filter(|_| !line.trim().is_empty()) {
            last.push(' ');
            last.push_str(line.trim());
        }
    }
    found
}

/// The problems with one Sources entry.
fn check_entry(entry: &str, anchors: &BTreeSet<String>) -> Vec<String> {
    let mut problems = Vec::new();
    let class = entry
        .strip_prefix('`')
        .and_then(|rest| rest.split_once("`:"))
        .map(|(class, rest)| (class, rest.trim()));
    let links = linked_anchors(entry);
    match class {
        None => problems.push(format!(
            "start it with its class, as `class`: (one of {})",
            CLASSES.join(", ")
        )),
        Some((class, _)) if !CLASSES.contains(&class) => problems.push(format!(
            "unknown class `{class}`; use one of {}",
            CLASSES.join(", ")
        )),
        Some((_, "")) => problems.push("say what the source is and what it supports".to_owned()),
        Some((class @ ("standard" | "vendor-doc"), _)) if links.is_empty() => problems.push(
            format!("a `{class}` source links to its anchor in {SOURCES}, as ({SOURCES}#anchor)"),
        ),
        Some(("observed-file", rest)) => {
            if !rest.to_lowercase().contains("corpus") {
                problems.push(
                    "an `observed-file` source names the corpus files it rests on".to_owned(),
                );
            }
        }
        Some(("format-fact", rest))
            if links.is_empty() && !rest.to_lowercase().contains("corpus") =>
        {
            problems.push(format!(
                "a `format-fact` source names the corpus files or links to its anchor in {SOURCES}"
            ));
        }
        Some(_) => {}
    }
    for anchor in links {
        if !anchors.contains(&anchor) {
            problems.push(format!("{SOURCES} has no anchor `{anchor}`"));
        }
    }
    problems
}

/// The anchors an entry links to in `sources.md`, from `(sources.md#name)`.
fn linked_anchors(entry: &str) -> Vec<String> {
    let marker = format!("({SOURCES}#");
    let mut found = Vec::new();
    let mut rest = entry;
    while let Some(at) = rest.find(&marker) {
        rest = &rest[at + marker.len()..];
        let end = rest.find(')').unwrap_or(rest.len());
        found.push(rest[..end].to_owned());
        rest = &rest[end..];
    }
    found
}

fn is_word(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULE: &str = "\
# TF-001: Lag on the predecessor's calendar

A lag counts working time.

## Statement

1. A lag is counted on the predecessor's calendar.

## Options

None.

## Sources

- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), pp. 41–46: a lag
  stands for real waiting time.
- `vendor-doc`: [MSPDI schema](sources.md#msp-xml-schema), `LinkLag`: lag is held in
  tenths of a minute.
- `observed-file`: files in the corpus: stored dates follow the rule.
- `own-choice`: item 1; see Rationale.

## Rationale

Ours.

## Tests

`crates/engine/src/tests.rs`: `lag_counts_working_time`.

## Differences

None known.
";

    fn known() -> BTreeSet<String> {
        anchors("<a id=\"gao-sag\"></a>\n#### GAO\n<a id=\"msp-xml-schema\"></a>\n")
    }

    fn problems(name: &str, text: &str) -> Vec<String> {
        check_rule(name, text, &known())
    }

    #[test]
    fn an_example_rule_passes() {
        assert_eq!(
            problems("TF-001-lag-calendar.md", RULE),
            Vec::<String>::new()
        );
    }

    #[test]
    fn tests_a_rule_names_exist() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let rule = "## Tests\n\n- `xtask/src/rules.rs`: `check_tests` (every `item`).\n\
                    - `xtask/src/rules.rs`: `no_such_test`.\n- `xtask/src/gone.rs`: `a`.\n";
        assert_eq!(
            check_tests(rule, &root),
            [
                "Tests names `no_such_test`, which `xtask/src/rules.rs` doesn't define",
                "Tests names `xtask/src/gone.rs`, which doesn't exist",
            ]
        );
        let paragraph = "## Tests\n\n`xtask/src/rules.rs`: `check_tests` and\n`no_such_test`.\n";
        assert_eq!(
            check_tests(paragraph, &root),
            ["Tests names `no_such_test`, which `xtask/src/rules.rs` doesn't define"]
        );
        assert_eq!(
            check_tests("## Tests\n\nNone yet.\n", &root),
            ["the Tests section names no test file (`path`: `name`)"]
        );
        assert_eq!(
            check_tests("# A rule\n", &root),
            ["no Tests section (## Tests)"]
        );
    }

    #[test]
    fn the_template_s_examples_pass() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(DIR);
        let template = fs::read_to_string(root.join("template.md")).unwrap();
        let sources = anchors(&fs::read_to_string(root.join(SOURCES)).unwrap());
        assert_eq!(
            check_rule("TF-000-template.md", &template, &sources),
            Vec::<String>::new()
        );
    }

    #[test]
    fn the_repository_s_rules_pass() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let (_, problems) = check_dir(&root.join(DIR), &root).unwrap();
        assert_eq!(problems, Vec::<String>::new());
    }

    #[test]
    fn a_rule_with_no_sources_fails() {
        let missing = RULE.replace("## Sources", "## References");
        assert_eq!(
            problems("TF-001-lag-calendar.md", &missing),
            ["no Sources section (## Sources)"]
        );
        let start = RULE.find("- `standard`").unwrap();
        let end = RULE.find("## Rationale").unwrap();
        let empty = format!("{}{}", &RULE[..start], &RULE[end..]);
        assert_eq!(
            problems("TF-001-lag-calendar.md", &empty),
            ["the Sources section has no entries"]
        );
    }

    #[test]
    fn an_unknown_or_missing_class_fails() {
        let unknown = RULE.replace("`own-choice`", "`paper`");
        assert_eq!(
            problems("TF-001-lag-calendar.md", &unknown),
            [
                "Sources entry 4: unknown class `paper`; use one of standard, vendor-doc, \
              observed-file, format-fact, own-choice"
            ]
        );
        let missing = RULE.replace("- `own-choice`: item 1", "- Item 1");
        assert_eq!(
            problems("TF-001-lag-calendar.md", &missing),
            [
                "Sources entry 4: start it with its class, as `class`: (one of standard, \
              vendor-doc, observed-file, format-fact, own-choice)"
            ]
        );
    }

    #[test]
    fn a_missing_anchor_fails() {
        let wrong = RULE.replace("sources.md#gao-sag", "sources.md#gao-guide");
        assert_eq!(
            problems("TF-001-lag-calendar.md", &wrong),
            ["Sources entry 1: sources.md has no anchor `gao-guide`"]
        );
        let unlinked = RULE.replace("(sources.md#msp-xml-schema)", "(https://example.com)");
        assert_eq!(
            problems("TF-001-lag-calendar.md", &unlinked),
            [
                "Sources entry 2: a `vendor-doc` source links to its anchor in sources.md, as \
              (sources.md#anchor)"
            ]
        );
    }

    #[test]
    fn misnamed_and_duplicate_rules_in_the_folder_fail() {
        let root = tempfile::tempdir().unwrap();
        let tests = root.path().join("crates/engine/src");
        fs::create_dir_all(&tests).unwrap();
        fs::write(tests.join("tests.rs"), "fn lag_counts_working_time() {}").unwrap();
        fs::create_dir_all(root.path().join("corpus")).unwrap();
        fs::write(root.path().join(crate::corpus::MANIFEST), "").unwrap();
        let dir = root.path().join(DIR);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(SOURCES),
            "<a id=\"gao-sag\"></a><a id=\"msp-xml-schema\"></a>",
        )
        .unwrap();
        fs::write(dir.join("README.md"), "# Not a rule").unwrap();
        fs::write(dir.join("TF-001-lag-calendar.md"), RULE).unwrap();
        fs::write(dir.join("TF-001-open-ends.md"), RULE).unwrap();
        fs::write(dir.join("tf-002-lag.md"), "# No sources").unwrap();
        let (checked, problems) = check_dir(&dir, root.path()).unwrap();
        assert_eq!(checked, 3);
        assert_eq!(
            problems,
            [
                "docs/rules/TF-001-open-ends.md: another rule already uses TF-001; take the next \
                 free number",
                "docs/rules/tf-002-lag.md: name it TF-NNN-slug.md: a three-digit number and \
                 lowercase words joined by hyphens",
                "docs/rules/tf-002-lag.md: no Sources section (## Sources)",
                "docs/rules/tf-002-lag.md: no Tests section (## Tests)",
            ]
        );
    }

    #[test]
    fn a_rule_names_only_listed_corpus_files() {
        let listed = BTreeSet::from(["C-0062".to_owned()]);
        assert_eq!(
            unlisted_files("As C-0062 shows.", &listed),
            Vec::<String>::new()
        );
        assert_eq!(
            unlisted_files("As C-0062 and C-0070 show.", &listed),
            ["names C-0070, which corpus/manifest.toml doesn't list"]
        );
        let other = "ABC-0062, C-12 and C-00620 are other things.";
        assert_eq!(
            unlisted_files(other, &BTreeSet::new()),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_format_fact_names_the_corpus_or_links_its_source() {
        let named = RULE.replace(
            "- `observed-file`: files in the corpus",
            "- `format-fact`: files in the corpus",
        );
        assert_eq!(
            problems("TF-001-lag-calendar.md", &named),
            Vec::<String>::new()
        );
        let unnamed = RULE.replace(
            "- `observed-file`: files in the corpus",
            "- `format-fact`: some files",
        );
        assert_eq!(
            problems("TF-001-lag-calendar.md", &unnamed),
            [
                "Sources entry 3: a `format-fact` source names the corpus files or links to its \
              anchor in sources.md"
            ]
        );
    }

    #[test]
    fn an_observed_file_source_names_the_corpus() {
        let unnamed = RULE.replace("files in the corpus", "some files");
        assert_eq!(
            problems("TF-001-lag-calendar.md", &unnamed),
            ["Sources entry 3: an `observed-file` source names the corpus files it rests on"]
        );
    }

    #[test]
    fn a_rule_file_is_named_tf_number_slug() {
        for good in ["TF-001-lag.md", "TF-120-lag-on-calendar-2.md"] {
            assert!(valid_name(good), "{good}");
        }
        for bad in [
            "TF-1-lag.md",
            "TF-001.md",
            "TF-001-Lag.md",
            "TF-001-lag--x.md",
            "TF-0011-x.md",
        ] {
            assert!(!valid_name(bad), "{bad}");
        }
        assert_eq!(
            problems("TF-1.md", RULE),
            ["name it TF-NNN-slug.md: a three-digit number and lowercase words joined by hyphens"]
        );
    }

    #[test]
    fn a_heading_inside_a_code_fence_ends_no_section() {
        let fenced = RULE.replace(
            "- `own-choice`: item 1; see Rationale.",
            "- `own-choice`: item 1; see Rationale.\n\n```\n## Not a heading\n- not an entry\n```",
        );
        assert_eq!(
            problems("TF-001-lag-calendar.md", &fenced),
            Vec::<String>::new()
        );
    }
}
