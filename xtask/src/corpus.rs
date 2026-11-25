//! The corpus (`cargo xtask corpus …`): public schedule files TotalFloat is
//! tested on. `corpus/manifest.toml` lists each one with where it's posted,
//! its license as stated there, and its SHA-256. The files aren't in this
//! repository: `fetch` downloads them into a local corpus directory, at
//! `files/<corpus-id>/<original filename>` (corpus/README.md).

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, ErrorKind, Write as _};
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The manifest, relative to the repository root.
pub(crate) const MANIFEST: &str = "corpus/manifest.toml";
const MANIFEST_HEADER: &str = "# The corpus: public schedule files TotalFloat is tested on, with where each\n\
# is posted. The files aren't in this repository; `cargo xtask corpus fetch`\n\
# downloads them and checks each against its SHA-256. See corpus/README.md.\n\n";

/// A corpus subcommand.
#[derive(Debug, Subcommand)]
pub(crate) enum Action {
    /// Download every listed file that's missing, checking each against its
    /// SHA-256. Needs `curl`.
    Fetch,
    /// Copy a file into the corpus directory and list it in the manifest.
    Add(AddArgs),
    /// List manifest entries.
    List,
    /// Verify hashes and look for missing, duplicate or unlisted files.
    Check,
    /// List `inbox/` files with their `NOTES.md` lines for confirmation.
    Inbox,
}

/// What `corpus add` needs to know about a file.
#[derive(Debug, Args)]
pub(crate) struct AddArgs {
    /// File to copy into the corpus.
    file: PathBuf,
    /// Where it's posted: a link anyone can download it from.
    #[arg(long)]
    url: String,
    /// A short name for where it came from.
    #[arg(long)]
    source: String,
    /// The license stated where it's posted (`none stated` if there is none).
    #[arg(long)]
    license: String,
    /// Date obtained, `YYYY-MM-DD`.
    #[arg(long)]
    obtained: String,
    /// Stored results don't match the file's inputs.
    #[arg(long)]
    stale: bool,
    /// Tag, repeatable (for example `p6-export` or `synthetic`).
    #[arg(long = "tag")]
    tags: Vec<String>,
    /// Free-form notes.
    #[arg(long)]
    notes: Option<String>,
}

/// One corpus file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    id: String,
    filename: String,
    source: String,
    url: String,
    license: String,
    obtained: String,
    /// Second field of the `ERMHDR` line, absent for files without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    p6_version: Option<String>,
    sha256: String,
    stale: bool,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    notes: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    #[serde(default)]
    file: Vec<Entry>,
}

impl Manifest {
    fn load(path: &Path) -> Result<Self, Box<dyn Error>> {
        let text = fs::read_to_string(path)
            .map_err(|error| format!("can't read {}: {error}", path.display()))?;
        Ok(toml::from_str(&text)?)
    }

    /// Writes through a temporary file so a failure can't truncate the manifest.
    fn save(&self, path: &Path) -> Result<(), Box<dyn Error>> {
        let text = format!("{MANIFEST_HEADER}{}", toml::to_string_pretty(self)?);
        let temporary = path.with_extension("toml.tmp");
        fs::write(&temporary, text)?;
        fs::rename(temporary, path)?;
        Ok(())
    }

    /// IDs are never reused, so directories left under `files/` count too.
    fn next_id(&self, root: &Path) -> Result<String, Box<dyn Error>> {
        let mut last = self
            .file
            .iter()
            .filter_map(|entry| parse_id(&entry.id))
            .max()
            .unwrap_or(0);
        let files = root.join("files");
        if files.exists() {
            for entry in fs::read_dir(files)? {
                if let Some(id) = entry?.file_name().to_str().and_then(parse_id) {
                    last = last.max(id);
                }
            }
        }
        Ok(format!("C-{:04}", last + 1))
    }
}

fn parse_id(id: &str) -> Option<u32> {
    let digits = id.strip_prefix("C-")?;
    (digits.len() >= 4 && digits.bytes().all(|b| b.is_ascii_digit()))
        .then(|| digits.parse().ok())
        .flatten()
}

/// The IDs `corpus/manifest.toml` lists, for the rules check.
pub(crate) fn listed_ids(manifest: &Path) -> Result<BTreeSet<String>, Box<dyn Error>> {
    Ok(Manifest::load(manifest)?
        .file
        .into_iter()
        .map(|entry| entry.id)
        .collect())
}

fn is_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let number = |range: std::ops::Range<usize>| {
        let digits = value.get(range)?;
        digits
            .bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| digits.parse::<u32>().ok())
            .flatten()
    };
    let (Some(year), Some(month), Some(day)) = (number(0..4), number(5..7), number(8..10)) else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => return false,
    };
    (1..=days).contains(&day)
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut hasher = Sha256::new();
    io::copy(&mut fs::File::open(path)?, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// The export version from the `ERMHDR` line, without parsing the whole file.
fn header_version(bytes: &[u8]) -> Option<String> {
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let line = bytes.split(|b| *b == b'\n').next()?;
    let line = String::from_utf8_lossy(line);
    let mut fields = line.trim_end_matches('\r').split('\t');
    (fields.next() == Some("ERMHDR"))
        .then(|| fields.next())
        .flatten()
        .filter(|version| !version.is_empty())
        .map(str::to_owned)
}

/// Every file under a directory, sorted. The corpus also takes MS Project and
/// PMXML files, so this doesn't filter by extension.
fn all_files(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let mut pending = vec![directory.to_owned()];
    let mut files = Vec::new();
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(next)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
            } else {
                files.push(entry.path());
            }
        }
    }
    files.sort();
    Ok(files)
}

fn stored_path(root: &Path, entry: &Entry) -> PathBuf {
    root.join("files").join(&entry.id).join(&entry.filename)
}

fn provenance_problem(entry: &Entry) -> Option<&'static str> {
    let blank = |value: &str| value.trim().is_empty();
    if blank(&entry.source) {
        Some("source is empty")
    } else if blank(&entry.license) {
        Some("license is empty: say `none stated` if there is none")
    } else if !entry.url.starts_with("https://") {
        Some("url must be an https link anyone can download it from")
    } else if !is_date(&entry.obtained) {
        Some("obtained must be a real YYYY-MM-DD date")
    } else {
        None
    }
}

/// Where to download a file from: a GitHub page link becomes its raw file.
fn download_url(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("https://github.com/") {
        let parts: Vec<&str> = rest.splitn(4, '/').collect();
        if let [owner, repo, "blob", path] = parts[..] {
            return format!("https://raw.githubusercontent.com/{owner}/{repo}/{path}");
        }
    }
    url.to_owned()
}

/// Downloads with `curl`.
fn curl(url: &str) -> Result<Vec<u8>, String> {
    let output = Command::new("curl")
        .args(["-sSfL", "--retry", "2", url])
        .output()
        .map_err(|e| format!("curl didn't run: {e}"))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

/// Downloads every listed file missing from `root`, through `download`, and
/// keeps each only if its SHA-256 matches. Returns how many were fetched and
/// already present, and a problem for each that couldn't be.
fn fetch(
    root: &Path,
    manifest: &Manifest,
    download: impl Fn(&str) -> Result<Vec<u8>, String>,
) -> Result<(usize, usize, Vec<String>), Box<dyn Error>> {
    let (mut fetched, mut present, mut problems) = (0, 0, Vec::new());
    for entry in &manifest.file {
        if Path::new(&entry.filename).file_name() != Some(entry.filename.as_ref()) {
            problems.push(format!("{}: filename must not contain a path", entry.id));
            continue;
        }
        let path = stored_path(root, entry);
        match sha256_file(&path) {
            Ok(hash) if hash == entry.sha256 => {
                present += 1;
                continue;
            }
            Ok(_) => {
                problems.push(format!("{}: a different file is already there", entry.id));
                continue;
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let bytes = match download(&download_url(&entry.url)) {
            Ok(bytes) => bytes,
            Err(error) => {
                problems.push(format!(
                    "{}: couldn't download {}: {error}",
                    entry.id, entry.url
                ));
                continue;
            }
        };
        let hash = sha256(&bytes);
        if hash != entry.sha256 {
            problems.push(format!(
                "{}: {} has changed (SHA-256 {hash}); not kept",
                entry.id, entry.url
            ));
            continue;
        }
        if let Some(directory) = path.parent() {
            fs::create_dir_all(directory)?;
        }
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?
            .write_all(&bytes)?;
        fetched += 1;
    }
    Ok((fetched, present, problems))
}

fn add(manifest_path: &Path, root: &Path, args: &AddArgs) -> Result<Entry, Box<dyn Error>> {
    let filename = args
        .file
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the file name must be valid UTF-8")?;
    let bytes = fs::read(&args.file)?;
    let hash = sha256(&bytes);
    if !root.is_dir() {
        return Err(format!("{} is not a directory", root.display()).into());
    }
    let mut manifest = Manifest::load(manifest_path)?;
    if let Some(existing) = manifest.file.iter().find(|entry| entry.sha256 == hash) {
        return Err(format!("already in the corpus as {}", existing.id).into());
    }
    let entry = Entry {
        id: manifest.next_id(root)?,
        filename: filename.to_owned(),
        source: args.source.clone(),
        url: args.url.clone(),
        license: args.license.clone(),
        obtained: args.obtained.clone(),
        p6_version: header_version(&bytes),
        sha256: hash,
        stale: args.stale,
        tags: args.tags.clone(),
        notes: args.notes.clone(),
    };
    if let Some(problem) = provenance_problem(&entry) {
        return Err(problem.into());
    }
    let path = stored_path(root, &entry);
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }
    // `create_new`: never replace a stored file.
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(&bytes)?;
    manifest.file.push(entry.clone());
    manifest.save(manifest_path)?;
    Ok(entry)
}

fn list(manifest: &Manifest) -> String {
    let mut out = String::new();
    for entry in &manifest.file {
        let _ = writeln!(
            out,
            "{}\t{}\t{}\t{}\t{}\t{}",
            entry.id,
            entry.p6_version.as_deref().unwrap_or("-"),
            entry.tags.join(","),
            if entry.stale { "stale" } else { "" },
            entry.license,
            entry.filename,
        );
    }
    let _ = writeln!(out, "{} files", manifest.file.len());
    out
}

fn check(root: &Path, manifest: &Manifest) -> Result<Vec<String>, Box<dyn Error>> {
    let mut problems = Vec::new();
    let mut ids = BTreeSet::new();
    let mut hashes = BTreeMap::new();
    let mut listed = BTreeSet::new();
    for entry in &manifest.file {
        if parse_id(&entry.id).is_none() {
            problems.push(format!("{}: malformed corpus ID", entry.id));
        }
        if !ids.insert(&entry.id) {
            problems.push(format!("{}: duplicate corpus ID", entry.id));
        }
        if let Some(first) = hashes.insert(&entry.sha256, &entry.id) {
            problems.push(format!("{}: same content as {first}", entry.id));
        }
        if let Some(problem) = provenance_problem(entry) {
            problems.push(format!("{}: {problem}", entry.id));
        }
        if Path::new(&entry.filename).file_name() != Some(entry.filename.as_ref()) {
            problems.push(format!("{}: filename must not contain a path", entry.id));
            continue;
        }
        let path = stored_path(root, entry);
        match sha256_file(&path) {
            Ok(hash) if hash == entry.sha256 => {}
            Ok(_) => problems.push(format!("{}: hash mismatch", entry.id)),
            Err(error) if error.kind() == ErrorKind::NotFound => {
                problems.push(format!("{}: file missing", entry.id));
            }
            Err(error) => problems.push(format!("{}: can't read: {error}", entry.id)),
        }
        listed.insert(path);
    }
    for path in all_files(root)? {
        let relative = path.strip_prefix(root).unwrap_or(&path);
        if relative.starts_with("files") {
            if !listed.contains(&path) {
                problems.push(format!("{}: not in the manifest", report_path(relative)));
            }
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("xer"))
        {
            // Corpus tests read every *.xer under the root.
            let name = report_path(relative);
            if let Some(id) = hashes.get(&sha256_file(&path)?) {
                problems.push(format!(
                    "{name}: copy of {id}, counted twice by corpus tests"
                ));
            } else if !relative.starts_with("inbox") {
                problems.push(format!("{name}: outside files/ and inbox/"));
            }
        }
    }
    Ok(problems)
}

/// Use the same relative path spelling in reports on every operating system.
fn report_path(path: &Path) -> String {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Whether `line` names `file` as a whole token, so a note for `old-plan.xer`
/// isn't taken for `plan.xer`.
fn mentions(line: &str, file: &str) -> bool {
    let name_part = |c: char| c.is_alphanumeric() || "._-/".contains(c);
    line.match_indices(file).any(|(start, _)| {
        !line[..start].ends_with(name_part) && !line[start + file.len()..].starts_with(name_part)
    })
}

fn inbox(root: &Path) -> Result<String, Box<dyn Error>> {
    let directory = root.join("inbox");
    if !directory.exists() {
        return Ok("no inbox directory\n".to_owned());
    }
    let notes = match fs::read(directory.join("NOTES.md")) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    let mut out = String::new();
    let mut count = 0;
    for path in all_files(&directory)? {
        let relative = path.strip_prefix(&directory)?;
        if relative == Path::new("NOTES.md") {
            continue;
        }
        count += 1;
        let name = report_path(relative);
        let _ = writeln!(out, "{name}");
        let base = name.rsplit('/').next().unwrap_or(&name);
        let mut lines = notes
            .lines()
            .filter(|line| mentions(line, &name) || mentions(line, base))
            .peekable();
        if lines.peek().is_none() {
            let _ = writeln!(out, "  (no NOTES.md line)");
        }
        for line in lines {
            let _ = writeln!(out, "  {line}");
        }
    }
    let _ = writeln!(out, "{count} files in inbox");
    Ok(out)
}

pub(crate) fn run(
    manifest_path: &Path,
    root: &Path,
    action: &Action,
) -> Result<(), Box<dyn Error>> {
    match action {
        Action::Fetch => {
            if !root.is_dir() {
                fs::create_dir_all(root)?;
            }
            let (fetched, present, problems) = fetch(root, &Manifest::load(manifest_path)?, curl)?;
            for problem in &problems {
                println!("{problem}");
            }
            println!("{fetched} fetched, {present} already present");
            if !problems.is_empty() {
                return Err(format!("{} files not fetched", problems.len()).into());
            }
        }
        Action::Add(args) => {
            let entry = add(manifest_path, root, args)?;
            println!("added {}", entry.id);
        }
        Action::List => print!("{}", list(&Manifest::load(manifest_path)?)),
        Action::Check => {
            let manifest = Manifest::load(manifest_path)?;
            let problems = check(root, &manifest)?;
            for problem in &problems {
                println!("{problem}");
            }
            if !problems.is_empty() {
                return Err(format!("{} corpus problems", problems.len()).into());
            }
            println!("{} files verified", manifest.file.len());
        }
        Action::Inbox => print!("{}", inbox(root)?),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    type Change = fn(&mut AddArgs);

    const XER: &[u8] = b"ERMHDR\t23.12\t2026-01-01\r\n%T\tPROJECT\r\n%E\r\n";

    fn args(file: &Path) -> AddArgs {
        AddArgs {
            file: file.to_owned(),
            url: "https://example.invalid/sample.xer".to_owned(),
            source: "synthetic".to_owned(),
            license: "none stated".to_owned(),
            obtained: "2026-10-30".to_owned(),
            stale: false,
            tags: vec!["synthetic".to_owned()],
            notes: None,
        }
    }

    /// A corpus directory with an empty manifest beside its files.
    fn empty_corpus() -> (tempfile::TempDir, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let manifest = root.path().join("manifest.toml");
        Manifest::default().save(&manifest).unwrap();
        (root, manifest)
    }

    fn corpus_with_one_file() -> (tempfile::TempDir, PathBuf, Entry) {
        let (root, manifest) = empty_corpus();
        let file = root.path().join("sample.xer");
        fs::write(&file, XER).unwrap();
        let entry = add(&manifest, root.path(), &args(&file)).unwrap();
        fs::remove_file(file).unwrap();
        (root, manifest, entry)
    }

    #[test]
    fn add_copies_hashes_reads_the_version_and_lists() {
        let (root, manifest_path, entry) = corpus_with_one_file();
        assert_eq!(entry.id, "C-0001");
        assert_eq!(entry.p6_version.as_deref(), Some("23.12"));
        assert_eq!(fs::read(stored_path(root.path(), &entry)).unwrap(), XER);

        let manifest = Manifest::load(&manifest_path).unwrap();
        assert_eq!(manifest.file, [entry]);
        let listing = list(&manifest);
        assert!(listing.contains("C-0001\t23.12\tsynthetic\t\tnone stated\tsample.xer"));
        assert!(listing.ends_with("1 files\n"));
        assert_eq!(check(root.path(), &manifest).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn ids_continue_after_the_highest_and_duplicates_are_refused() {
        let (root, manifest_path, _) = corpus_with_one_file();
        let copy = root.path().join("copy.xer");
        fs::write(&copy, XER).unwrap();
        let error = add(&manifest_path, root.path(), &args(&copy)).unwrap_err();
        assert_eq!(error.to_string(), "already in the corpus as C-0001");

        fs::write(&copy, b"ERMHDR\t8.2\r\n%E\r\n").unwrap();
        let mut manifest = Manifest::load(&manifest_path).unwrap();
        manifest.file[0].id = "C-0041".to_owned();
        manifest.save(&manifest_path).unwrap();
        assert_eq!(
            add(&manifest_path, root.path(), &args(&copy)).unwrap().id,
            "C-0042"
        );
    }

    #[test]
    fn add_refuses_incomplete_provenance_before_writing() {
        let (root, manifest_path) = empty_corpus();
        let file = root.path().join("sample.xer");
        fs::write(&file, XER).unwrap();
        let cases: [(Change, &str); 6] = [
            (|a| a.obtained = "yesterday".to_owned(), "obtained must be"),
            (|a| a.obtained = "2026-02-30".to_owned(), "obtained must be"),
            (|a| a.url = String::new(), "url must be an https link"),
            (
                |a| a.url = "file:///home/a.xer".to_owned(),
                "url must be an https link",
            ),
            (|a| a.source = " ".to_owned(), "source is empty"),
            (|a| a.license = String::new(), "license is empty"),
        ];
        for (change, message) in cases {
            let mut bad = args(&file);
            change(&mut bad);
            assert!(
                add(&manifest_path, root.path(), &bad)
                    .unwrap_err()
                    .to_string()
                    .contains(message)
            );
        }
        assert!(!root.path().join("files").exists());
        assert_eq!(
            Manifest::load(&manifest_path).unwrap().file,
            Vec::<Entry>::new()
        );
    }

    #[test]
    fn dates_are_checked_against_the_calendar() {
        for good in ["2026-10-30", "2024-02-29", "2000-02-29", "2026-12-31"] {
            assert!(is_date(good), "{good}");
        }
        for bad in [
            "2026-13-01",
            "2026-00-10",
            "2025-02-29",
            "1900-02-29",
            "2026-04-31",
            "2026-1-01",
            "20261030xx",
            "2026-01-00",
            "١٢٣٤-01-01",
        ] {
            assert!(!is_date(bad), "{bad}");
        }
    }

    #[test]
    fn a_missing_manifest_or_root_is_an_error_not_an_empty_corpus() {
        let (root, manifest_path, entry) = corpus_with_one_file();
        fs::remove_file(&manifest_path).unwrap();
        let file = root.path().join("other.xer");
        fs::write(&file, b"ERMHDR\t8.2\r\n%E\r\n").unwrap();
        let error = add(&manifest_path, root.path(), &args(&file)).unwrap_err();
        assert!(error.to_string().contains("can't read"));
        assert_eq!(fs::read(stored_path(root.path(), &entry)).unwrap(), XER);

        let missing = root.path().join("nowhere");
        assert!(run(&manifest_path, &missing, &Action::Check).is_err());
        assert!(run(&manifest_path, &missing, &Action::List).is_err());
        let (_other, manifest_path) = empty_corpus();
        assert!(add(&manifest_path, &missing, &args(&file)).is_err());
        assert!(!missing.exists());
    }

    #[test]
    fn ids_are_never_reused_and_stored_files_are_never_replaced() {
        let (root, manifest_path, entry) = corpus_with_one_file();
        Manifest::default().save(&manifest_path).unwrap();
        // C-0001's directory survives its manifest entry.
        let file = root.path().join("other.xer");
        fs::write(&file, b"ERMHDR\t8.2\r\n%E\r\n").unwrap();
        assert_eq!(
            add(&manifest_path, root.path(), &args(&file)).unwrap().id,
            "C-0002"
        );
        assert_eq!(fs::read(stored_path(root.path(), &entry)).unwrap(), XER);
    }

    #[test]
    fn manifest_round_trips_optional_fields() {
        let (_root, manifest_path, mut entry) = corpus_with_one_file();
        entry.notes = Some("note".to_owned());
        entry.stale = true;
        let manifest = Manifest {
            file: vec![entry.clone()],
        };
        manifest.save(&manifest_path).unwrap();
        let text = fs::read_to_string(&manifest_path).unwrap();
        assert!(text.starts_with(MANIFEST_HEADER));
        assert_eq!(Manifest::load(&manifest_path).unwrap().file, [entry]);
    }

    #[test]
    fn the_repository_manifest_is_valid_and_complete() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(MANIFEST);
        let manifest = Manifest::load(&path).unwrap();
        let mut ids = BTreeSet::new();
        for entry in &manifest.file {
            assert_eq!(provenance_problem(entry), None, "{}", entry.id);
            assert!(parse_id(&entry.id).is_some(), "{}", entry.id);
            assert!(ids.insert(&entry.id), "{} listed twice", entry.id);
            assert_eq!(entry.sha256.len(), 64, "{}", entry.id);
        }
    }

    #[test]
    fn github_page_links_download_their_raw_file() {
        assert_eq!(
            download_url("https://github.com/owner/repo/blob/main/tests/a%20b.xer"),
            "https://raw.githubusercontent.com/owner/repo/main/tests/a%20b.xer"
        );
        assert_eq!(
            download_url("https://example.invalid/a.xer"),
            "https://example.invalid/a.xer"
        );
        assert_eq!(
            download_url("https://github.com/owner/repo"),
            "https://github.com/owner/repo"
        );
    }

    #[test]
    fn fetch_keeps_only_files_whose_hash_matches() {
        let (_root, manifest_path, entry) = corpus_with_one_file();
        let mut changed = entry.clone();
        changed.id = "C-0002".to_owned();
        changed.filename = "changed.xer".to_owned();
        changed.sha256 = sha256(b"what was listed");
        let mut gone = entry.clone();
        gone.id = "C-0003".to_owned();
        gone.url = "https://example.invalid/gone.xer".to_owned();
        gone.sha256 = sha256(b"gone");
        let manifest = Manifest {
            file: vec![entry.clone(), changed, gone],
        };
        manifest.save(&manifest_path).unwrap();

        let target = tempfile::tempdir().unwrap();
        let download = |url: &str| {
            if url.ends_with("gone.xer") {
                Err("404".to_owned())
            } else if url.ends_with("sample.xer") {
                Ok(XER.to_vec())
            } else {
                Ok(b"something else".to_vec())
            }
        };
        let (fetched, present, problems) = fetch(target.path(), &manifest, download).unwrap();
        assert_eq!((fetched, present), (1, 0));
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems[0].starts_with("C-0002: https://example.invalid/sample.xer has changed"));
        assert!(problems[1].starts_with("C-0003: couldn't download"));
        assert_eq!(fs::read(stored_path(target.path(), &entry)).unwrap(), XER);
        assert!(!target.path().join("files/C-0002").exists());

        // A second run finds the file and downloads nothing.
        let refuse = |_: &str| -> Result<Vec<u8>, String> { panic!("downloaded again") };
        let one = Manifest { file: vec![entry] };
        assert_eq!(fetch(target.path(), &one, refuse).unwrap(), (0, 1, vec![]));
    }

    #[test]
    fn header_version_handles_bom_crlf_and_non_xer() {
        assert_eq!(
            header_version(b"\xef\xbb\xbfERMHDR\t6.2\tx\r\n").as_deref(),
            Some("6.2")
        );
        assert_eq!(header_version(b"ERMHDR\t8.3\r\n").as_deref(), Some("8.3"));
        assert_eq!(header_version(b"ERMHDR\t\t\n"), None);
        assert_eq!(header_version(b"not an export\n"), None);
        assert_eq!(header_version(b""), None);
        assert_eq!(header_version(&[0; 32]), None);
    }

    #[test]
    fn check_reports_each_kind_of_problem() {
        let (root, manifest_path, entry) = corpus_with_one_file();
        let stored = stored_path(root.path(), &entry);

        fs::write(&stored, b"changed").unwrap();
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert_eq!(
            check(root.path(), &manifest).unwrap(),
            ["C-0001: hash mismatch"]
        );

        fs::remove_file(&stored).unwrap();
        assert_eq!(
            check(root.path(), &manifest).unwrap(),
            ["C-0001: file missing"]
        );

        fs::write(&stored, XER).unwrap();
        fs::write(stored.with_file_name("extra.mpp"), XER).unwrap();
        let problems = check(root.path(), &manifest).unwrap();
        assert_eq!(problems.len(), 1);
        assert!(problems[0].ends_with("extra.mpp: not in the manifest"));
        fs::remove_file(stored.with_file_name("extra.mpp")).unwrap();

        let mut bad = entry.clone();
        bad.filename = "../escape.xer".to_owned();
        bad.obtained = "soon".to_owned();
        let mut twin = entry.clone();
        twin.id = "X".to_owned();
        let manifest = Manifest {
            file: vec![entry, bad, twin],
        };
        let problems = check(root.path(), &manifest).unwrap();
        for expected in [
            "C-0001: duplicate corpus ID",
            "C-0001: same content as C-0001",
            "C-0001: obtained must be a real YYYY-MM-DD date",
            "C-0001: filename must not contain a path",
            "X: malformed corpus ID",
            "X: same content as C-0001",
        ] {
            assert!(
                problems.iter().any(|p| p == expected),
                "{expected}: {problems:?}"
            );
        }
    }

    #[test]
    fn check_finds_schedules_that_corpus_tests_would_double_count() {
        let (root, manifest_path, _) = corpus_with_one_file();
        let manifest = Manifest::load(&manifest_path).unwrap();
        fs::create_dir_all(root.path().join("inbox")).unwrap();
        fs::write(root.path().join("inbox/pending.xer"), b"ERMHDR\t8.2\n").unwrap();
        assert_eq!(check(root.path(), &manifest).unwrap(), Vec::<String>::new());

        fs::write(root.path().join("inbox/copy.xer"), XER).unwrap();
        fs::write(root.path().join("stray.xer"), b"ERMHDR\t8.3\n").unwrap();
        fs::write(root.path().join("README.md"), "notes").unwrap();
        let problems = check(root.path(), &manifest).unwrap();
        assert_eq!(
            problems,
            [
                "inbox/copy.xer: copy of C-0001, counted twice by corpus tests",
                "stray.xer: outside files/ and inbox/",
            ]
        );
    }

    #[test]
    fn check_reports_nested_unlisted_files_with_portable_paths() {
        let (root, manifest_path, _) = corpus_with_one_file();
        let manifest = Manifest::load(&manifest_path).unwrap();
        let directory = root.path().join("files").join("C-0002").join("nested");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("extra.xer"), XER).unwrap();
        assert_eq!(
            check(root.path(), &manifest).unwrap(),
            ["files/C-0002/nested/extra.xer: not in the manifest"]
        );
    }

    #[test]
    fn inbox_notes_match_whole_names_and_survive_non_utf8() {
        let root = tempfile::tempdir().unwrap();
        let inbox_dir = root.path().join("inbox");
        fs::create_dir_all(inbox_dir.join("batch")).unwrap();
        fs::write(inbox_dir.join("plan.xer"), XER).unwrap();
        fs::write(inbox_dir.join("batch/noted.xer"), XER).unwrap();
        fs::write(
            inbox_dir.join("NOTES.md"),
            b"- `old-plan.xer`: restricted\n- `noted.xer`: from a donor \x93quoted\x94\n",
        )
        .unwrap();
        let out = inbox(root.path()).unwrap();
        assert!(out.contains("plan.xer\n  (no NOTES.md line)"), "{out}");
        assert!(
            out.contains("batch/noted.xer\n  - `noted.xer`: from a donor"),
            "{out}"
        );
    }

    #[test]
    fn inbox_pairs_files_with_notes_and_flags_the_rest() {
        let root = tempfile::tempdir().unwrap();
        let inbox_dir = root.path().join("inbox");
        fs::create_dir_all(inbox_dir.join("batch")).unwrap();
        fs::write(inbox_dir.join("batch/noted.xer"), XER).unwrap();
        fs::write(inbox_dir.join("bare.mpp"), XER).unwrap();
        fs::write(
            inbox_dir.join("NOTES.md"),
            "- `batch/noted.xer`: from a donor\n",
        )
        .unwrap();
        let out = inbox(root.path()).unwrap();
        assert_eq!(
            out,
            "bare.mpp\n  (no NOTES.md line)\nbatch/noted.xer\n  - `batch/noted.xer`: from a donor\n2 files in inbox\n"
        );
    }

    #[test]
    fn a_new_corpus_starts_from_an_existing_empty_directory() {
        let (root, manifest_path) = empty_corpus();
        assert_eq!(inbox(root.path()).unwrap(), "no inbox directory\n");
        let file = root.path().join("sample.xer");
        fs::write(&file, XER).unwrap();
        assert_eq!(
            add(&manifest_path, root.path(), &args(&file)).unwrap().id,
            "C-0001"
        );
    }
}
