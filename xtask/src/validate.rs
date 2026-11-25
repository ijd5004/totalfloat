//! Independent checks of files TotalFloat writes.
//!
//! No check here shows that P6 or Microsoft Project opens a file. What these
//! show is narrower and stated as such: an XER
//! passes XER Tools 2.2b's integrity checks and parses in MPXJ; an MS Project
//! XML file is valid against Microsoft's published schema and reads back
//! through MPXJ with what our own reader sees. They check files, never
//! scheduling behavior. Every tool lives under `~/.cache/totalfloat/`
//! (docs/validation/validators.md) and a missing one is skipped, not failed.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

use clap::Subcommand;
use sha2::{Digest, Sha256};

#[derive(Debug, Subcommand)]
pub enum Action {
    /// Check an XER with XER Tools 2.2b's integrity checks (under Wine), then
    /// parse it with MPXJ.
    Xer {
        /// The XER file.
        file: PathBuf,
    },
    /// Check MS Project XML against Microsoft's published schema (with
    /// `xmllint`), then read it with MPXJ and compare what it sees with
    /// TotalFloat's own reader.
    Msproject {
        /// The MS Project XML file.
        file: PathBuf,
    },
    /// Assemble Microsoft's MS Project XML schema from its published sections
    /// at a pinned commit, into the cache. Needs `curl`.
    FetchSchema,
}

/// Where the external tools live.
#[derive(Debug, Clone)]
pub struct Tools {
    root: PathBuf,
}

const WINE: &str = "runtime/wine-11.20/usr/bin/wine";
const XERTOOLS: &str = "external/xertools/2.2b/app/XERTools64.exe";
const XERTOOLS_PREFIX: &str = "external/xertools/2.2b/wineprefix";
const JAVA: &str = "runtime/liberica-jdk-21.0.13/bin/java";
const MPXJ: &str = "external/mpxj/mpxj";
const SCHEMA: &str = "external/mspdi/mspdi.xsd";

/// The documentation repository the schema sections come from (CC BY 4.0
/// for documentation, MIT for code), pinned.
const SCHEMA_REPO: &str = "MicrosoftDocs/office-developer-msproject-xml-docs";
const SCHEMA_COMMIT: &str = "c77602db4d0733ce57cb0e5514c6fc898c7fe2c2";
/// The namespace MS Project XML files use. The published schema declares
/// the 2007 one, `…/project/2007`, which no file uses.
const FILE_NAMESPACE: &str = "http://schemas.microsoft.com/project";
const SCHEMA_NAMESPACE: &str = "http://schemas.microsoft.com/project/2007";

/// Each section page, the collection it defines in `Project` (or the complex
/// type), and its SHA-256 at the pinned commit.
const SECTIONS: [(&str, &str, &str); 9] = [
    (
        "xml-schema-for-the-project-element.md",
        "Project",
        "63fa49974c7d64e7d82889229ef64f67381c18bd31cd81b5aded4bf5e0775322",
    ),
    (
        "xml-schema-for-the-outlinecodes-element.md",
        "OutlineCodes",
        "c8ffb0e3e0b771fe058cfe5826e00ca751818a3e58b4683f734866679742fd87",
    ),
    (
        "xml-schema-for-the-wbsmasks-element.md",
        "WBSMasks",
        "6fd3ba503715dcd16a9ee324dc8fd584436213d565628b7f89c244be2f06bada",
    ),
    (
        "xml-schema-for-the-extendedattributes-element.md",
        "ExtendedAttributes",
        "ed52041ba8c2ef0cee2ac955b01e53db07221c1aee94df9baa3fc7002d18fc91",
    ),
    (
        "xml-schema-for-the-calendars-element.md",
        "Calendars",
        "5c684eb2d797368fc5eabda25156739f468899101651d8e1c610769522e7d56c",
    ),
    (
        "xml-schema-for-the-tasks-element.md",
        "Tasks",
        "9b244a5924fd2e8c83afdaf9ef1064f8f195fc1203bd0508de480160ceb9dc70",
    ),
    (
        "xml-schema-for-the-resources-element.md",
        "Resources",
        "17cce749f8e0751ce3da817d526f1484ea5816793d2d2e08ee597fbd682e0e58",
    ),
    (
        "xml-schema-for-the-assignments-element.md",
        "Assignments",
        "f8e298ff452dfa1112b50c747f0a0099a1083ee0553e5c6c61d75670349cb55c",
    ),
    (
        "xml-schema-for-the-timephaseddatatype-complex-type.md",
        "TimephasedDataType",
        "bb4144ad3728e9ee733d0d198cbb0627516213c144f791ce460c4551efe4eedf",
    ),
];

impl Tools {
    /// `$TOTALFLOAT_CACHE`, else `~/.cache/totalfloat`.
    pub fn from_env() -> Self {
        let root = std::env::var_os("TOTALFLOAT_CACHE").map_or_else(
            || {
                let home = std::env::var_os("HOME")
                    .or_else(|| std::env::var_os("USERPROFILE"))
                    .unwrap_or_default();
                PathBuf::from(home).join(".cache/totalfloat")
            },
            PathBuf::from,
        );
        Self { root }
    }

    #[cfg(test)]
    pub fn at(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
        }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    /// The first missing piece of a tool, or `None` when all are present.
    fn missing(&self, pieces: &[&str]) -> Option<PathBuf> {
        pieces
            .iter()
            .map(|piece| self.path(piece))
            .find(|path| !path.exists())
    }
}

/// One check's outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Passed(String),
    Failed(String),
    Skipped(String),
}

impl Outcome {
    fn line(&self, check: &str) -> String {
        match self {
            Self::Passed(what) => format!("{check}: passed. {what}"),
            Self::Failed(what) => format!("{check}: FAILED. {what}"),
            Self::Skipped(why) => format!("{check}: skipped, {why}"),
        }
    }
}

/// `cargo xtask validate …`. Exits non-zero when any check failed.
pub fn run(action: &Action) -> Result<(), Box<dyn Error>> {
    let tools = Tools::from_env();
    let outcomes = match action {
        Action::Xer { file } => vec![
            (
                "XER Tools 2.2b",
                xertools(&tools, file).unwrap_or_else(|e| Outcome::Failed(e.to_string())),
            ),
            ("MPXJ", mpxj_parses(&tools, file)),
        ],
        Action::Msproject { file } => vec![
            ("Schema", schema(&tools, file)),
            ("MPXJ read-back", mpxj_read_back(&tools, file)),
        ],
        Action::FetchSchema => {
            fetch_schema(&tools)?;
            return Ok(());
        }
    };
    let mut failed = false;
    for (check, outcome) in &outcomes {
        println!("{}", outcome.line(check));
        failed |= matches!(outcome, Outcome::Failed(_));
    }
    if failed {
        return Err("a validator found a problem".into());
    }
    Ok(())
}

// ---- XER Tools ----

/// What XER Tools reports when its checks finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Integrity {
    pub completed: u64,
    pub of: u64,
    pub warnings: u64,
    pub errors: u64,
}

fn count(word: &str) -> Option<u64> {
    match word {
        "no" => Some(0),
        _ => word.replace(',', "").parse().ok(),
    }
}

/// Reads the summary line XER Tools writes to its session log, in either
/// form it uses: `107 of 107 Checks completed without warnings or errors!`
/// or `3,553,729 of 3,553,449  checks completed with 4,508 warnings and 5 errors!`.
pub fn integrity(log: &str) -> Option<Integrity> {
    let line = log
        .lines()
        .rev()
        .find(|line| line.to_ascii_lowercase().contains("checks completed"))?;
    // The log separates fields with control characters and stray bytes.
    let clean: String = line
        .chars()
        .map(|c| if c.is_ascii_graphic() { c } else { ' ' })
        .collect();
    let words: Vec<&str> = clean.split_whitespace().collect();
    let of_at = words.iter().position(|w| *w == "of")?;
    let completed = count(words.get(of_at.checked_sub(1)?)?)?;
    let of = count(words.get(of_at + 1)?)?;
    let lower = clean.to_ascii_lowercase();
    if lower.contains("without warnings or errors") {
        return Some(Integrity {
            completed,
            of,
            warnings: 0,
            errors: 0,
        });
    }
    // A noun the summary doesn't mention counts 0; one it mentions without a
    // readable number makes the whole summary unreadable.
    let before = |noun: &str| -> Option<u64> {
        let Some(at) = words.iter().position(|w| {
            w.trim_end_matches('!')
                .to_ascii_lowercase()
                .starts_with(noun)
        }) else {
            return Some(0);
        };
        count(words.get(at.checked_sub(1)?)?)
    };
    Some(Integrity {
        completed,
        of,
        warnings: before("warning")?,
        errors: before("error")?,
    })
}

/// Every session log in the XER Tools prefix, with its size in bytes.
fn session_logs(tools: &Tools) -> BTreeMap<PathBuf, u64> {
    let mut logs = BTreeMap::new();
    let users = tools.path(XERTOOLS_PREFIX).join("drive_c/users");
    let Ok(users) = std::fs::read_dir(users) else {
        return logs;
    };
    for user in users.flatten() {
        let folder = user.path().join("AppData/Roaming/Season's Fall/XER Tools");
        let Ok(entries) = std::fs::read_dir(folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "log") {
                let size = entry.metadata().map_or(0, |m| m.len());
                logs.insert(path, size);
            }
        }
    }
    logs
}

/// What XER Tools appended to its session logs since `before` was taken: the
/// new bytes of the log that grew, or the whole of a log that is new.
fn appended(tools: &Tools, before: &BTreeMap<PathBuf, u64>) -> Option<(PathBuf, String)> {
    session_logs(tools).into_iter().find_map(|(path, size)| {
        let from = before.get(&path).copied().unwrap_or(0);
        if size <= from {
            return None;
        }
        let bytes = std::fs::read(&path).ok()?;
        let from = usize::try_from(from).ok()?;
        let new = String::from_utf8_lossy(bytes.get(from..)?).into_owned();
        new.contains("session started").then_some((path, new))
    })
}

/// The summary of the last session in `log`, if that session finished and
/// reported one. XER Tools appends every session to the same log, so an
/// earlier session's summary must never answer for a later one.
pub fn last_session(log: &str) -> Option<Integrity> {
    let session = log.rfind("session started").map_or(log, |at| &log[at..]);
    if !session.contains("session finished") {
        return None;
    }
    integrity(session)
}

fn wine(tools: &Tools) -> Command {
    let mut command = Command::new(tools.path(WINE));
    command
        .env("WINEPREFIX", tools.path(XERTOOLS_PREFIX))
        .env("WINEDEBUG", "-all")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

/// Waits up to `limit` for `child` to exit.
fn exited(child: &mut std::process::Child, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if !matches!(child.try_wait(), Ok(None)) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    false
}

fn xertools(tools: &Tools, file: &Path) -> Result<Outcome, Box<dyn Error>> {
    if let Some(missing) = tools.missing(&[WINE, XERTOOLS, XERTOOLS_PREFIX]) {
        return Ok(Outcome::Skipped(format!(
            "not installed ({} is missing; see docs/validation/validators.md)",
            missing.display()
        )));
    }
    let file = std::fs::canonicalize(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let started = SystemTime::now();
    let before = session_logs(tools);
    let mut child = wine(tools)
        .arg(tools.path(XERTOOLS))
        .arg("/INTEGRITY")
        .arg(format!("Z:{}", file.display()))
        .spawn()?;
    // XER Tools writes its summary when it closes, so give the checks time to
    // run (more for a larger file), close it, then read the session it adds.
    let size = std::fs::metadata(&file).map_or(0, |m| m.len());
    let checks = Duration::from_secs(30 + (size / 100_000).min(270));
    std::thread::sleep(checks);
    // Runs share one Wine prefix, so this closes any other run's XER Tools
    // too; run one XER check at a time.
    let _ = wine(tools)
        .args(["taskkill", "/IM", "XERTools64.exe"])
        .status();
    if !exited(&mut child, Duration::from_secs(30)) {
        // A dialog can swallow the close; force it, and fail below if that
        // leaves no finished session.
        let _ = wine(tools)
            .args(["taskkill", "/F", "/IM", "XERTools64.exe"])
            .status();
        if !exited(&mut child, Duration::from_secs(15)) {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut session = None;
    while Instant::now() < deadline {
        session = appended(tools, &before);
        if session
            .as_ref()
            .is_some_and(|(_, new)| new.contains("session finished"))
        {
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    let Some((_, new)) = session else {
        return Ok(Outcome::Failed(
            "XER Tools wrote no session log for this run".into(),
        ));
    };
    // Keep this run's session; the log itself holds every earlier one too.
    let stem = file.file_stem().unwrap_or_default().to_string_lossy();
    let since_1970 = started.duration_since(SystemTime::UNIX_EPOCH)?.as_secs();
    let copy = tools.path(&format!("logs/xertools-{stem}-{since_1970}.log"));
    std::fs::create_dir_all(tools.path("logs"))?;
    std::fs::write(&copy, &new)?;
    let Some(summary) = last_session(&new) else {
        return Ok(Outcome::Failed(format!(
            "this run's session has no integrity summary; the file may not have loaded (log: {})",
            copy.display()
        )));
    };
    let what = format!(
        "{} of {} checks, {} warnings, {} errors (log: {})",
        summary.completed,
        summary.of,
        summary.warnings,
        summary.errors,
        copy.display()
    );
    Ok(verdict(summary, what))
}

/// A pass needs every check to have run and none to have found an error.
pub fn verdict(summary: Integrity, what: String) -> Outcome {
    if summary.completed < summary.of {
        Outcome::Failed(format!(
            "the checks didn't finish before XER Tools was closed: {what}"
        ))
    } else if summary.errors == 0 {
        Outcome::Passed(what)
    } else {
        Outcome::Failed(what)
    }
}

// ---- MPXJ ----

/// What a reader saw in a file.
pub type Counts = BTreeMap<String, u64>;

/// Reads `key value` lines, as `MpxjSummary.java` prints them.
pub fn counts(text: &str) -> Counts {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.trim().split_once(' ')?;
            Some((key.to_owned(), value.trim().parse().ok()?))
        })
        .collect()
}

fn mpxj(tools: &Tools, file: &Path) -> Result<Counts, Outcome> {
    if let Some(missing) = tools.missing(&[JAVA, MPXJ]) {
        return Err(Outcome::Skipped(format!(
            "not installed ({} is missing; see docs/validation/validators.md)",
            missing.display()
        )));
    }
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/validate/MpxjSummary.java");
    let mpxj = tools.path(MPXJ);
    let separator = if cfg!(windows) { ";" } else { ":" };
    let classpath = format!(
        "{}{separator}{}",
        mpxj.join("mpxj.jar").display(),
        mpxj.join("lib/*").display()
    );
    let output = Command::new(tools.path(JAVA))
        .args(["-cp", &classpath])
        .arg(source)
        .arg(file)
        .output()
        .map_err(|e| Outcome::Failed(format!("java didn't run: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let last = stderr.lines().rev().find(|l| !l.trim().is_empty());
        return Err(Outcome::Failed(format!(
            "MPXJ couldn't read the file: {}",
            last.unwrap_or("no message").trim()
        )));
    }
    let counts = counts(&String::from_utf8_lossy(&output.stdout));
    let expected = [
        "activities",
        "summaries",
        "links",
        "resources",
        "assignments",
        "calendars",
    ];
    if let Some(gap) = expected.iter().find(|key| !counts.contains_key(**key)) {
        return Err(Outcome::Failed(format!(
            "MpxjSummary printed no {gap} count, so its output can't be trusted"
        )));
    }
    Ok(counts)
}

fn show(counts: &Counts) -> String {
    counts
        .iter()
        .map(|(key, value)| format!("{value} {key}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn mpxj_parses(tools: &Tools, file: &Path) -> Outcome {
    match mpxj(tools, file) {
        Ok(counts) => Outcome::Passed(format!("read {}", show(&counts))),
        Err(outcome) => outcome,
    }
}

/// What TotalFloat's own reader sees in an MS Project XML file, under the
/// names `MpxjSummary.java` uses.
/// Import notes that each stand for one item TotalFloat's MS Project reader
/// leaves out on purpose, and the count the item belongs to.
const LEFT_OUT: [(&str, &str); 4] = [
    ("inactive-task", "activities"),
    ("summary-logic", "links"),
    ("link-to-left-out-task", "links"),
    ("unused-resource", "resources"),
];

/// What TotalFloat's reader kept, plus what its notes say it left out on
/// purpose, so a difference from MPXJ is one nothing explains.
pub fn accounted(mut kept: Counts, notes: &[(&str, usize)]) -> (Counts, Vec<String>) {
    let mut explained = Vec::new();
    for (kind, count) in notes {
        if let Some((_, key)) = LEFT_OUT.iter().find(|(k, _)| k == kind) {
            let count = u64::try_from(*count).unwrap_or(u64::MAX);
            *kept.entry((*key).into()).or_default() += count;
            explained.push(format!("{count} {key} ({kind})"));
        }
    }
    (kept, explained)
}

fn ours(file: &Path) -> Result<(Counts, Vec<String>), Box<dyn Error>> {
    let text = std::fs::read_to_string(file)?;
    let imported = totalfloat_interop::msproject::read(&text)
        .map_err(|e| format!("TotalFloat's reader refused the file: {e}"))?;
    let schedule = imported.schedule;
    let n = |len: usize| u64::try_from(len).unwrap_or(u64::MAX);
    let kept = Counts::from([
        ("activities".into(), n(schedule.activities.len())),
        ("links".into(), n(schedule.relationships.len())),
        ("resources".into(), n(schedule.resources.len())),
        ("assignments".into(), n(schedule.assignments.len())),
        ("calendars".into(), n(schedule.calendars.len())),
    ]);
    let notes: Vec<(&str, usize)> = imported.notes.iter().map(|n| (n.kind, n.count)).collect();
    Ok(accounted(kept, &notes))
}

/// Each count both readers report that differs, as `key: ours vs theirs`.
pub fn differences(ours: &Counts, theirs: &Counts) -> Vec<String> {
    ours.iter()
        .filter_map(|(key, mine)| match theirs.get(key) {
            Some(other) if other == mine => None,
            Some(other) => Some(format!("{key}: TotalFloat {mine}, MPXJ {other}")),
            None => Some(format!("{key}: TotalFloat {mine}, MPXJ none")),
        })
        .collect()
}

fn mpxj_read_back(tools: &Tools, file: &Path) -> Outcome {
    let theirs = match mpxj(tools, file) {
        Ok(counts) => counts,
        Err(outcome) => return outcome,
    };
    let (ours, explained) = match ours(file) {
        Ok(found) => found,
        Err(e) => return Outcome::Failed(e.to_string()),
    };
    let left_out = if explained.is_empty() {
        String::new()
    } else {
        format!(
            "; TotalFloat's counts include {}, which its import notes say it left out",
            explained.join(", ")
        )
    };
    let differences = differences(&ours, &theirs);
    if differences.is_empty() {
        Outcome::Passed(format!("both readers see {}{left_out}", show(&ours)))
    } else {
        Outcome::Failed(format!("{}{left_out}", differences.join("; ")))
    }
}

// ---- Schema ----

fn schema(tools: &Tools, file: &Path) -> Outcome {
    if let Some(missing) = tools.missing(&[SCHEMA]) {
        return Outcome::Skipped(format!(
            "no schema ({} is missing; run `cargo xtask validate fetch-schema`)",
            missing.display()
        ));
    }
    let Ok(output) = Command::new("xmllint")
        .args(["--noout", "--schema"])
        .arg(tools.path(SCHEMA))
        .arg(file)
        .output()
    else {
        return Outcome::Skipped("xmllint (libxml2) is not installed".into());
    };
    if output.status.success() {
        return Outcome::Passed("valid against Microsoft's published schema".into());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lines: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
    let errors: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| line.contains("error"))
        .collect();
    let shown = if errors.is_empty() { &lines } else { &errors };
    let mut what = shown
        .iter()
        .take(5)
        .copied()
        .collect::<Vec<_>>()
        .join("\n  ");
    if what.is_empty() {
        what = format!("xmllint exited with {}", output.status);
    }
    // xmllint exits 5 when the schema itself doesn't compile.
    if output.status.code() == Some(5) {
        return Outcome::Failed(format!(
            "the schema didn't compile, so the file wasn't checked; run `cargo xtask validate fetch-schema` again: {what}"
        ));
    }
    Outcome::Failed(what)
}

/// The first ```` ``` xml ```` block of a section page.
fn xml_block(page: &str) -> Option<&str> {
    let start = page.find("``` xml\n")? + "``` xml\n".len();
    let length = page[start..].find("\n```")?;
    Some(&page[start..start + length])
}

/// A section's definition from its first `<xsd:element name="…"` or
/// `<xsd:complexType name="…"` on, without the page's elision marks.
fn definition(page: &str, name: &str) -> Result<String, String> {
    let block = xml_block(page).ok_or_else(|| format!("{name}: no xml block"))?;
    let start = [
        format!("<xsd:element name=\"{name}\""),
        format!("<xsd:complexType name=\"{name}\""),
    ]
    .iter()
    .filter_map(|opening| block.find(opening.as_str()))
    .min()
    .ok_or_else(|| format!("{name}: no definition in its section"))?;
    Ok(block[start..]
        .lines()
        .filter(|line| line.trim() != ". . .")
        .collect::<Vec<_>>()
        .join("\n"))
}

/// Puts the schema together: the Project section, with each collection's
/// elided definition (`<xsd:element name="Tasks" minOccurs="0"> . . . </xsd:element>`)
/// and the elided `TimephasedDataType` replaced by its own section, and the
/// namespace files use.
pub fn assemble(pages: &BTreeMap<&str, String>) -> Result<String, String> {
    let project = pages.get("Project").ok_or("no Project section")?;
    let mut schema = xml_block(project)
        .ok_or("Project: no xml block")?
        .to_owned();
    for (name, page) in pages.iter().filter(|(name, _)| **name != "Project") {
        let full = definition(page, name)?;
        let elided = if *name == "TimephasedDataType" {
            format!("<xsd:complexType name=\"{name}\"> . . . </xsd:complexType>")
        } else {
            format!("<xsd:element name=\"{name}\" minOccurs=\"0\"> . . . </xsd:element>")
        };
        if !schema.contains(&elided) {
            return Err(format!("{name}: the Project section has no place for it"));
        }
        schema = schema.replacen(&elided, &full, 1);
    }
    if schema.contains(". . .") {
        return Err("a section is still elided".into());
    }
    if !schema.contains("</xsd:schema>") {
        schema.push_str("\n</xsd:schema>\n");
    }
    Ok(schema.replace(
        &format!("\"{SCHEMA_NAMESPACE}\""),
        &format!("\"{FILE_NAMESPACE}\""),
    ))
}

fn fetch_schema(tools: &Tools) -> Result<(), Box<dyn Error>> {
    let mut pages = BTreeMap::new();
    for (page, name, sha256) in SECTIONS {
        let url = format!(
            "https://raw.githubusercontent.com/{SCHEMA_REPO}/{SCHEMA_COMMIT}/project-xml-data-interchange/{page}"
        );
        let output = Command::new("curl")
            .args(["-sSfL", &url])
            .output()
            .map_err(|e| format!("curl didn't run: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "couldn't download {page}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )
            .into());
        }
        let digest = format!("{:x}", Sha256::digest(&output.stdout));
        if digest != sha256 {
            return Err(format!("{page}: SHA-256 {digest}, expected {sha256}").into());
        }
        pages.insert(name, String::from_utf8(output.stdout)?);
    }
    let schema = assemble(&pages)?;
    let path = tools.path(SCHEMA);
    let folder = path.parent().ok_or("no folder for the schema")?;
    std::fs::create_dir_all(folder)?;
    std::fs::write(&path, &schema)?;
    let mut provenance = String::new();
    writeln!(
        provenance,
        "Assembled by `cargo xtask validate fetch-schema` from the section pages of\n\
         https://github.com/{SCHEMA_REPO} at commit {SCHEMA_COMMIT}\n\
         (documentation CC BY 4.0, code MIT; see that repository's LICENSE and LICENSE-CODE).\n\
         Target namespace changed from {SCHEMA_NAMESPACE} to {FILE_NAMESPACE}, the one files use.\n\
         For local validation only; not committed to TotalFloat."
    )?;
    std::fs::write(folder.join("PROVENANCE.txt"), provenance)?;
    println!("Wrote {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests;
