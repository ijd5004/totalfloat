//! `cargo xtask site`: builds the docs site from the Markdown in `docs/` and
//! checks it before it's published (`.github/workflows/pages.yml`).
//!
//! mdBook renders the pages listed in `docs/SUMMARY.md` (configured in
//! `site/book.toml`) into `target/site/`. This command then reads every page
//! it wrote and:
//!
//! - rewrites each link to a repository file that isn't on the site, such as
//!   `../ARCHITECTURE.md` or `../../ui/README.md`, to the file on GitHub;
//! - fails on a link to a page or file that doesn't exist, or to a heading
//!   that isn't on the page it links to;
//! - fails on anything a page or style sheet loads from another host, since
//!   the site has no analytics, trackers, or third-party scripts or fonts.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The site's mdBook configuration, relative to the repository root.
const BOOK: &str = "site";
/// Where mdBook writes the site (`build-dir` in `site/book.toml`).
const OUT: &str = "target/site";
/// The folder the site's pages come from (`src` in `site/book.toml`).
const DOCS: &str = "docs";
/// The site's path on its host (`site-url` in `site/book.toml`).
const BASE: &str = "/totalfloat/";
/// Where links to repository files point.
const GITHUB: &str = "https://github.com/ijd5004/totalfloat";

/// Pages in `docs/` that a page links to before the change adding them has
/// merged. A link to one of them that doesn't exist yet is reported and left
/// alone; once the page exists, it's checked like any other. Remove an entry
/// when its page lands.
const NOT_YET: &[&str] = &[];

/// Builds the site with `mdbook` and checks it. Prints one line per page that
/// isn't there yet, and one line, or every problem found, at the end.
pub fn run(mdbook: &Path) -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let problems = summary_problems(&root)?;
    if !problems.is_empty() {
        for problem in &problems {
            eprintln!("{problem}");
        }
        return Err(format!("site: {} problems in docs/SUMMARY.md", problems.len()).into());
    }
    let status = Command::new(mdbook)
        .arg("build")
        .arg(root.join(BOOK))
        .status()
        .map_err(|error| format!("couldn't run {}: {error}", mdbook.display()))?;
    if !status.success() {
        return Err(format!("{} build failed: {status}", mdbook.display()).into());
    }
    let out = root.join(OUT);
    let report = check(&out, &root, NOT_YET)?;
    for page in &report.not_yet {
        println!("site: {page} doesn't exist yet; links to it are left as they are");
    }
    for page in NOT_YET {
        if root.join(DOCS).join(page).is_file() {
            println!("site: {DOCS}/{page} exists now; remove it from NOT_YET in xtask/src/site.rs");
        }
    }
    if report.problems.is_empty() {
        println!(
            "site: {} pages in {OUT}, {} links checked, {} rewritten to GitHub, nothing loaded from another host",
            report.pages, report.links, report.rewritten
        );
        return Ok(());
    }
    for problem in &report.problems {
        eprintln!("{problem}");
    }
    Err(format!("site: {} problems", report.problems.len()).into())
}

/// What [`check`] found in a built site.
#[derive(Debug, Default)]
struct Report {
    /// HTML pages read.
    pages: usize,
    /// Links and loaded resources checked, outside links included.
    links: usize,
    /// Links rewritten to a repository file on GitHub.
    rewritten: usize,
    /// [`NOT_YET`] pages that links pointed to and that don't exist.
    not_yet: BTreeSet<String>,
    /// Each problem, naming its page.
    problems: Vec<String>,
}

/// Checks the site built in `out`, the files of the repository at `root`,
/// and rewrites the links to repository files in its pages. A link to one of
/// the `not_yet` pages that doesn't exist is reported apart from the problems.
fn check(out: &Path, root: &Path, not_yet: &[&str]) -> Result<Report, Box<dyn Error>> {
    let mut files = BTreeSet::new();
    list_files(out, out, &mut files)?;
    let mut ids = BTreeMap::new();
    let mut texts = BTreeMap::new();
    for file in &files {
        let path = out.join(file);
        if has_extension(file, "html") {
            let text = fs::read_to_string(&path)?;
            ids.insert(file.clone(), ids_in(&text));
            texts.insert(file.clone(), text);
        } else if has_extension(file, "css") {
            let text = fs::read_to_string(&path)?;
            texts.insert(file.clone(), text);
        }
    }
    let site = Site {
        files: &files,
        ids: &ids,
        root,
        not_yet,
    };
    let mut report = Report::default();
    for (file, text) in &texts {
        if has_extension(file, "css") {
            for url in css_urls(text) {
                report.links += 1;
                if is_remote(&url) {
                    report
                        .problems
                        .push(format!("{file}: loads {url} from another host"));
                }
            }
            continue;
        }
        report.pages += 1;
        let rewritten = site.page(file, text, &mut report);
        if rewritten != *text {
            fs::write(out.join(file), rewritten)?;
        }
    }
    Ok(report)
}

/// Lists every file under `dir` into `files`, as `/`-separated paths
/// relative to `out`.
fn list_files(out: &Path, dir: &Path, files: &mut BTreeSet<String>) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            list_files(out, &path, files)?;
        } else if let Ok(relative) = path.strip_prefix(out) {
            let parts: Vec<_> = relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect();
            files.insert(parts.join("/"));
        }
    }
    Ok(())
}

/// The built site, for resolving links.
struct Site<'a> {
    /// Every file in the site, relative to its root.
    files: &'a BTreeSet<String>,
    /// The element ids on each HTML page.
    ids: &'a BTreeMap<String, BTreeSet<String>>,
    /// The repository root.
    root: &'a Path,
    /// Pages that may not exist yet ([`NOT_YET`]).
    not_yet: &'a [&'a str],
}

/// Where a link on the site goes.
#[derive(Debug, PartialEq)]
enum Target {
    /// A file or page on the site, or another host: left as it is.
    Fine,
    /// A folder's index page on the site, at this link.
    Index(String),
    /// A repository file, at this GitHub URL.
    GitHub(String),
    /// A [`NOT_YET`] page that doesn't exist yet.
    NotYet(String),
    /// Nothing: the problem, without the page name.
    Broken(String),
}

impl Site<'_> {
    /// Checks every link and loaded resource on the page `file`, adding to
    /// `report`, and returns the page with links to repository files
    /// rewritten.
    fn page(&self, file: &str, text: &str, report: &mut Report) -> String {
        // mdBook's 404 page resolves its links from the site root.
        let base = if text.contains("<base href=") {
            ""
        } else {
            file.rsplit_once('/').map_or("", |(dir, _)| dir)
        };
        for css in style_elements(text) {
            for url in css_urls(css) {
                report.links += 1;
                if is_remote(&url) {
                    report
                        .problems
                        .push(format!("{file}: <style> loads {url} from another host"));
                }
            }
        }
        let mut out = String::with_capacity(text.len());
        let mut copied = 0;
        for attribute in attributes(text) {
            let Attribute {
                tag,
                name,
                value,
                start,
                end,
            } = attribute;
            let urls = match name.as_str() {
                "style" => css_urls(&value),
                // Candidates separated by commas, each a URL and a size.
                "srcset" => value
                    .split(',')
                    .filter_map(|candidate| candidate.split_whitespace().next())
                    .map(str::to_owned)
                    .collect(),
                _ => Vec::new(),
            };
            if !urls.is_empty() {
                for url in urls {
                    report.links += 1;
                    if is_remote(&url) {
                        report
                            .problems
                            .push(format!("{file}: loads {url} from another host"));
                    }
                }
                continue;
            }
            if name != "href" && name != "src" {
                continue;
            }
            report.links += 1;
            let loads = name == "src" || tag == "link";
            if is_remote(&value) {
                if loads {
                    report
                        .problems
                        .push(format!("{file}: <{tag}> loads {value} from another host"));
                }
                continue;
            }
            match self.resolve(file, base, &value) {
                Target::Fine => {}
                Target::Index(url) => {
                    out.push_str(&text[copied..start]);
                    out.push_str(&url);
                    copied = end;
                }
                Target::GitHub(url) => {
                    out.push_str(&text[copied..start]);
                    out.push_str(&url);
                    copied = end;
                    report.rewritten += 1;
                }
                Target::NotYet(page) => {
                    report.not_yet.insert(page);
                }
                Target::Broken(problem) => report.problems.push(format!("{file}: {problem}")),
            }
        }
        out.push_str(&text[copied..]);
        out
    }

    /// Resolves the link `value` (HTML-escaped, as on the page) found on the
    /// page `file`, whose relative links start from the folder `base`.
    fn resolve(&self, file: &str, base: &str, value: &str) -> Target {
        let value = value.replace("&amp;", "&");
        if value.starts_with("mailto:") || value.starts_with("javascript:") {
            return Target::Fine;
        }
        let (path, fragment) = match value.split_once('#') {
            Some((path, fragment)) => (path, Some(fragment)),
            None => (value.as_str(), None),
        };
        if path.contains(':') {
            return Target::Broken(format!("unknown kind of link {value}"));
        }
        let joined = match path.strip_prefix(BASE) {
            Some(rest) => rest.to_owned(),
            None if path.starts_with('/') => {
                return Target::Broken(format!("{value} is outside the site"));
            }
            None if path.is_empty() => file.to_owned(),
            None if base.is_empty() => path.to_owned(),
            None => format!("{base}/{path}"),
        };
        let Some(parts) = normalize(&joined) else {
            return Target::Broken(format!("{value} is outside the repository"));
        };
        let above = parts.iter().take_while(|part| *part == "..").count();
        let target = parts.join("/");
        if above == 0 {
            // On the site: a page, a copied file, or a folder's index page.
            let page = if target.is_empty() || path.ends_with('/') {
                format!(
                    "{target}{}index.html",
                    if target.is_empty() { "" } else { "/" }
                )
            } else {
                target.clone()
            };
            if self.files.contains(&page) {
                return self.fragment(&page, fragment, &value);
            }
            // mdBook writes a folder's README.md as its index.html but leaves
            // links to the README alone.
            if let Some(dir) = page.strip_suffix("README.html") {
                let index = format!("{dir}index.html");
                if self.files.contains(&index) {
                    return match self.fragment(&index, fragment, &value) {
                        Target::Fine => {
                            let stem = path.strip_suffix("README.html").unwrap_or(path);
                            let rest = fragment.map(|f| format!("#{f}")).unwrap_or_default();
                            Target::Index(format!("{stem}index.html{rest}"))
                        }
                        other => other,
                    };
                }
            }
        }
        // A repository file the site doesn't hold. mdBook turned a link to
        // `name.md` into `name.html`.
        let in_repo = normalize(&format!("{DOCS}/{target}"));
        let Some(in_repo) = in_repo.map(|parts| parts.join("/")) else {
            return Target::Broken(format!("{value} is outside the repository"));
        };
        let source = in_repo
            .strip_suffix(".html")
            .map(|stem| format!("{stem}.md"))
            .filter(|source| self.root.join(source).is_file());
        if let Some(source) = &source
            && let Some(id) = fragment.filter(|id| !id.is_empty())
        {
            let text = fs::read_to_string(self.root.join(source)).unwrap_or_default();
            if !github_anchors(&text).contains(id) {
                return Target::Broken(format!("{value}: {source} has no heading #{id}"));
            }
        }
        let fragment = fragment.map(|f| format!("#{f}")).unwrap_or_default();
        if let Some(source) = source {
            return Target::GitHub(format!("{GITHUB}/blob/main/{source}{fragment}"));
        }
        let path = self.root.join(&in_repo);
        if path.is_file() {
            return Target::GitHub(format!("{GITHUB}/blob/main/{in_repo}{fragment}"));
        }
        if path.is_dir() && !in_repo.is_empty() {
            return Target::GitHub(format!("{GITHUB}/tree/main/{in_repo}{fragment}"));
        }
        let wanted = in_repo.strip_prefix(&format!("{DOCS}/")).map(|page| {
            page.strip_suffix(".html")
                .map_or(page.to_owned(), |stem| format!("{stem}.md"))
        });
        if let Some(page) = wanted.filter(|page| self.not_yet.contains(&page.as_str())) {
            return Target::NotYet(page);
        }
        Target::Broken(format!("{value} doesn't exist"))
    }

    /// Checks that the page `page` on the site has the heading or element
    /// `fragment` names, if any.
    fn fragment(&self, page: &str, fragment: Option<&str>, value: &str) -> Target {
        match (fragment, self.ids.get(page)) {
            (Some(id), Some(ids)) if !id.is_empty() && !ids.contains(id) => {
                Target::Broken(format!("{value}: {page} has no #{id}"))
            }
            _ => Target::Fine,
        }
    }
}

/// The anchors GitHub gives the headings of the Markdown `text`: lowercase,
/// punctuation other than `-` and `_` dropped, spaces as `-`, and `-1`, `-2`
/// on repeats. Headings inside fenced code blocks don't count.
fn github_anchors(text: &str) -> BTreeSet<String> {
    let mut anchors = BTreeSet::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut fence: Option<&str> = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(open) = fence {
            if trimmed.starts_with(open) {
                fence = None;
            }
            continue;
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fence = Some(&trimmed[..3]);
            continue;
        }
        let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
        if !(1..=6).contains(&hashes) || !trimmed[hashes..].starts_with(' ') {
            continue;
        }
        let heading = trimmed[hashes..].trim().trim_end_matches('#').trim();
        let slug: String = heading
            .to_lowercase()
            .chars()
            .filter_map(|c| match c {
                ' ' => Some('-'),
                '-' | '_' => Some(c),
                c if c.is_alphanumeric() => Some(c),
                _ => None,
            })
            .collect();
        let count = seen.entry(slug.clone()).or_insert(0);
        let anchor = if *count == 0 {
            slug.clone()
        } else {
            format!("{slug}-{count}")
        };
        *count += 1;
        anchors.insert(anchor);
    }
    anchors
}

/// Removes `.` and folder`/..` pairs from the `/`-separated `path`. Leading
/// `..` parts stay. Returns `None` for a path that climbs out of the
/// repository root (two levels above the site, which is `docs/`).
fn normalize(path: &str) -> Option<Vec<String>> {
    let mut parts: Vec<String> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|last| last != "..") => {
                parts.pop();
            }
            _ => parts.push(part.to_owned()),
        }
    }
    let above = parts.iter().take_while(|part| *part == "..").count();
    (above <= 1).then_some(parts)
}

/// Whether the file `name` has the extension `extension`, in any case.
fn has_extension(name: &str, extension: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|found| found.eq_ignore_ascii_case(extension))
}

/// Whether `url` points at another host.
fn is_remote(url: &str) -> bool {
    let url = url.trim().to_ascii_lowercase();
    url.starts_with("http:") || url.starts_with("https:") || url.starts_with("//")
}

/// An attribute in an HTML tag.
#[derive(Debug, PartialEq)]
struct Attribute {
    /// The tag's name, lowercase.
    tag: String,
    /// The attribute's name, lowercase.
    name: String,
    /// Its value, as written (HTML-escaped).
    value: String,
    /// Where the value starts in the page, in bytes.
    start: usize,
    /// Where the value ends in the page, in bytes.
    end: usize,
}

/// Every attribute of every tag in `html`, in page order. Skips comments and
/// the text of `<script>` and `<style>` elements. Unquoted values are read
/// to the next space or `>`.
fn attributes(html: &str) -> Vec<Attribute> {
    let bytes = html.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(offset) = html[at..].find('<') {
        let open = at + offset;
        if html[open..].starts_with("<!--") {
            at = html[open..]
                .find("-->")
                .map_or(html.len(), |end| open + end + 3);
            continue;
        }
        let name_end = html[open + 1..]
            .find(|c: char| !c.is_ascii_alphanumeric())
            .map_or(html.len(), |end| open + 1 + end);
        let tag = html[open + 1..name_end].to_ascii_lowercase();
        if tag.is_empty() || !tag.starts_with(|c: char| c.is_ascii_alphabetic()) {
            at = open + 1;
            continue;
        }
        let mut i = name_end;
        loop {
            while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] == b'>' {
                break;
            }
            let name_start = i;
            while i < bytes.len()
                && !bytes[i].is_ascii_whitespace()
                && !matches!(bytes[i], b'=' | b'>' | b'/')
            {
                i += 1;
            }
            let name = html[name_start..i].to_ascii_lowercase();
            if i < bytes.len() && bytes[i] == b'=' {
                i += 1;
                let (start, end) = if let Some(&quote @ (b'"' | b'\'')) = bytes.get(i) {
                    let start = i + 1;
                    let end = html[start..]
                        .find(char::from(quote))
                        .map_or(html.len(), |end| start + end);
                    i = (end + 1).min(html.len());
                    (start, end)
                } else {
                    let start = i;
                    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
                        i += 1;
                    }
                    (start, i)
                };
                found.push(Attribute {
                    tag: tag.clone(),
                    name,
                    value: html[start..end].to_owned(),
                    start,
                    end,
                });
            } else if name.is_empty() {
                i += 1;
            }
        }
        at = (i + 1).min(html.len());
        if tag == "script" || tag == "style" {
            let close = format!("</{tag}");
            at = html[at..].find(&close).map_or(html.len(), |end| at + end);
        }
    }
    found
}

/// The text of each `<style>` element on an HTML page.
fn style_elements(html: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(offset) = html[at..].find("<style") {
        let open = at + offset;
        let Some(start) = html[open..].find('>').map(|end| open + end + 1) else {
            break;
        };
        let end = html[start..]
            .find("</style")
            .map_or(html.len(), |end| start + end);
        found.push(&html[start..end]);
        at = end;
    }
    found
}

/// The ids of every element on an HTML page.
fn ids_in(html: &str) -> BTreeSet<String> {
    attributes(html)
        .into_iter()
        .filter(|attribute| attribute.name == "id")
        .map(|attribute| attribute.value)
        .collect()
}

/// The URLs in a style sheet's `url(…)` and `@import "…"`.
fn css_urls(css: &str) -> Vec<String> {
    let mut urls = Vec::new();
    for (marker, ends) in [("url(", &[')'][..]), ("@import", &[';'][..])] {
        let mut at = 0;
        while let Some(offset) = css[at..].find(marker) {
            let start = at + offset + marker.len();
            let end = css[start..]
                .find(|c| ends.contains(&c))
                .map_or(css.len(), |end| start + end);
            let url = css[start..end]
                .trim()
                .trim_start_matches("url(")
                .trim_matches(|c| c == '"' || c == '\'' || c == ')');
            urls.push(url.trim().to_owned());
            at = end;
        }
    }
    urls
}

/// The entries of `docs/SUMMARY.md`, the site's table of contents: title and
/// path, in order.
fn summary(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| link(line.trim_start().trim_start_matches("- ")))
        .collect()
}

/// The guide's pages in `docs/guide/README.md`'s numbered list, title and
/// path from `docs/`, in order.
fn guide_pages(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let (number, rest) = line.split_once(". ")?;
            if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let (title, path) = link(rest)?;
            Some((title, format!("guide/{path}")))
        })
        .collect()
}

/// The title and target of a line that is exactly one Markdown link.
fn link(line: &str) -> Option<(String, String)> {
    let rest = line.trim().strip_prefix('[')?;
    let (title, rest) = rest.split_once("](")?;
    let path = rest.strip_suffix(')')?;
    Some((title.to_owned(), path.to_owned()))
}

/// What's wrong with `docs/SUMMARY.md`: the guide's pages must come in the
/// order of `docs/guide/README.md`'s list, with the same titles, and every
/// rule page in `docs/rules/` but the template must be listed.
fn summary_problems(root: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let docs = root.join(DOCS);
    let read = |path: PathBuf| {
        fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))
    };
    let entries = summary(&read(docs.join("SUMMARY.md"))?);
    let guide = guide_pages(&read(docs.join("guide/README.md"))?);
    let mut problems = Vec::new();
    let listed: Vec<_> = entries
        .iter()
        .filter(|(_, path)| path.starts_with("guide/") && path != "guide/README.md")
        .cloned()
        .collect();
    if listed != guide {
        problems.push(format!(
            "docs/SUMMARY.md: the guide's pages must be the list in docs/guide/README.md, in its order and with its titles\n  SUMMARY.md: {listed:?}\n  README.md:  {guide:?}"
        ));
    }
    let paths: BTreeSet<_> = entries.iter().map(|(_, path)| path.as_str()).collect();
    let mut rules: Vec<String> = fs::read_dir(docs.join("rules"))?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| has_extension(name, "md") && name != "template.md")
        .map(|name| format!("rules/{name}"))
        .collect();
    rules.sort();
    for rule in rules {
        if !paths.contains(rule.as_str()) {
            problems.push(format!("docs/SUMMARY.md doesn't list {rule}"));
        }
    }
    Ok(problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
    }

    #[test]
    fn the_table_of_contents_follows_the_guide() {
        assert_eq!(summary_problems(&root()).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn github_anchors_follow_its_slugs() {
        let text = "# Title\n\n## Design decisions\n\n```sh\n# not a heading\n```\n\n## Who's `it` for? (v1)\n\n### Notes\n\n### Notes\n";
        let anchors = github_anchors(text);
        let expected = [
            "title",
            "design-decisions",
            "whos-it-for-v1",
            "notes",
            "notes-1",
        ];
        assert_eq!(anchors, expected.map(str::to_owned).into_iter().collect());
    }

    #[test]
    fn the_guide_list_is_read_in_order() {
        let text = "Intro\n\n1. [Install](install.md)\n2. [Schedule a file](schedule.md)\n\n- [Other](x.md)\n";
        assert_eq!(
            guide_pages(text),
            [
                ("Install".to_owned(), "guide/install.md".to_owned()),
                ("Schedule a file".to_owned(), "guide/schedule.md".to_owned()),
            ]
        );
    }

    #[test]
    fn attributes_skip_scripts_and_comments() {
        let html = r#"<a href="x.html">x</a><!-- <a href="no"> --><script>if (a<b) { x = '<img src="no">'; }</script><img src='y.png' alt=z>"#;
        let found: Vec<_> = attributes(html)
            .into_iter()
            .map(|a| (a.tag, a.name, a.value))
            .collect();
        let expected = [
            ("a", "href", "x.html"),
            ("img", "src", "y.png"),
            ("img", "alt", "z"),
        ]
        .map(|(t, n, v)| (t.to_owned(), n.to_owned(), v.to_owned()));
        assert_eq!(found, expected);
    }

    #[test]
    fn remote_urls_in_style_sheets_are_found() {
        let css = "a { background: url('https://cdn.example/x.png') } @import \"//fonts.example/f.css\"; b { background: url(local.png) }";
        let urls = css_urls(css);
        assert!(urls.iter().any(|url| url == "https://cdn.example/x.png"));
        assert!(urls.iter().any(|url| url == "//fonts.example/f.css"));
        assert!(urls.iter().any(|url| url == "local.png"));
        assert_eq!(urls.iter().filter(|url| is_remote(url)).count(), 2);
    }

    /// A small built site in a temporary folder, inside a fake repository.
    fn built(pages: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path().to_owned();
        for file in [
            "docs/ARCHITECTURE.md",
            "docs/guide/install.md",
            "ui/README.md",
        ] {
            let path = root.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "# x\n").unwrap();
        }
        let out = root.join("target/site");
        for (file, text) in pages {
            let path = out.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        (repo, root, out)
    }

    #[test]
    fn links_to_repository_files_point_to_github() {
        let (_repo, root, out) = built(&[
            (
                "guide/install.html",
                r#"<h2 id="linux">Linux</h2><a href="../ARCHITECTURE.html#x">a</a> <a href="../../ui/README.html">u</a> <a href="../guide/install.html#linux">l</a> <a href="README.html">r</a>"#,
            ),
            ("guide/index.html", "<h1 id=\"guide\">Guide</h1>"),
        ]);
        let report = check(&out, &root, NOT_YET).unwrap();
        assert_eq!(report.problems, Vec::<String>::new());
        assert_eq!(report.rewritten, 2);
        let page = fs::read_to_string(out.join("guide/install.html")).unwrap();
        assert!(page.contains(&format!(
            r#"href="{GITHUB}/blob/main/docs/ARCHITECTURE.md#x""#
        )));
        assert!(page.contains(&format!(r#"href="{GITHUB}/blob/main/ui/README.md""#)));
        assert!(page.contains(r#"href="../guide/install.html#linux""#));
        assert!(page.contains(r#"href="index.html""#));
    }

    #[test]
    fn broken_links_and_remote_loads_are_problems() {
        let (_repo, root, out) = built(&[
            (
                "index.html",
                r#"<a href="gone.html">g</a><a href="guide/install.html#nowhere">n</a><a href="../../../x.md">x</a><a href="ARCHITECTURE.html#gone">y</a><script src="https://cdn.example/a.js"></script><a href="https://example.com/">fine</a><img srcset="local.png 1x, https://cdn.example/c.png 2x"><style>@import "https://cdn.example/d.css";</style>"#,
            ),
            ("guide/install.html", "<h1 id=\"install\">Install</h1>"),
            (
                "css/site.css",
                "body { background: url(https://cdn.example/b.png) }",
            ),
        ]);
        let report = check(&out, &root, NOT_YET).unwrap();
        assert_eq!(report.problems.len(), 8, "{:#?}", report.problems);
        assert!(
            report
                .problems
                .iter()
                .any(|p| p.contains("loads https://cdn.example/c.png"))
        );
        assert!(
            report
                .problems
                .iter()
                .any(|p| p.contains("<style> loads https://cdn.example/d.css"))
        );
        assert!(
            report
                .problems
                .iter()
                .any(|p| p.contains("has no heading #gone"))
        );
        assert!(
            report
                .problems
                .iter()
                .any(|p| p.contains("gone.html doesn't exist"))
        );
        assert!(
            report
                .problems
                .iter()
                .any(|p| p.contains("has no #nowhere"))
        );
        assert!(
            report
                .problems
                .iter()
                .any(|p| p.contains("outside the repository"))
        );
        assert!(
            report
                .problems
                .iter()
                .any(|p| p.contains("<script> loads https://cdn.example/a.js"))
        );
        assert!(
            report
                .problems
                .iter()
                .any(|p| p.starts_with("css/site.css: loads"))
        );
    }

    #[test]
    fn a_page_not_merged_yet_is_tolerated() {
        let (_repo, root, out) = built(&[
            (
                "index.html",
                r#"<a href="guide/start-here.html">s</a><a href="guide/install.html">i</a>"#,
            ),
            ("guide/install.html", ""),
        ]);
        let report = check(&out, &root, &["guide/start-here.md"]).unwrap();
        assert_eq!(report.problems, Vec::<String>::new());
        assert_eq!(
            report.not_yet.into_iter().collect::<Vec<_>>(),
            ["guide/start-here.md"]
        );
    }
}
