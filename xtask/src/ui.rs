//! `cargo xtask ui`: build, check and license-check the web app in `ui/`
//! (ARCHITECTURE.md, Local app). Needs Node.js and pnpm; the Rust build never does.

use std::collections::BTreeSet;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// What to do. With neither flag, the app is built into `ui/dist/`, which
/// the next `cargo build` compiles into the binary.
#[derive(Debug, Clone, Copy)]
pub struct Asked {
    /// Type check, lint and test instead of building.
    pub check: bool,
    /// Refuse production dependencies whose licenses `deny.toml` doesn't
    /// allow.
    pub licenses: bool,
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

/// `pnpm`: a `.cmd` script on Windows, and elsewhere started through `sh`,
/// because until its native binary is installed, pnpm's launcher is a
/// shebang-less script that only a shell will run.
pub(crate) fn pnpm() -> Command {
    if cfg!(windows) {
        Command::new("pnpm.cmd")
    } else {
        let mut command = Command::new("sh");
        command.args(["-c", "exec pnpm \"$@\"", "pnpm"]);
        command
    }
}

fn pnpm_in(ui: &Path, args: &[&str]) -> Result<(), Box<dyn Error>> {
    let status = pnpm().args(args).current_dir(ui).status().map_err(|e| {
        format!("couldn't run pnpm ({e}); install Node.js (see ui/.node-version) and pnpm")
    })?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`pnpm {}` failed in ui/", args.join(" ")).into())
    }
}

/// Runs what was asked.
///
/// # Errors
///
/// pnpm missing or failing, or a dependency with a license not allowed.
pub fn run(asked: Asked) -> Result<(), Box<dyn Error>> {
    let root = root();
    let ui = root.join("ui");
    pnpm_in(&ui, &["install", "--frozen-lockfile", "--prefer-offline"])?;
    if asked.licenses {
        let out = pnpm()
            .args(["licenses", "list", "--prod", "--json"])
            .current_dir(&ui)
            .output()?;
        if !out.status.success() {
            return Err(format!(
                "`pnpm licenses list` failed: {}",
                String::from_utf8_lossy(&out.stderr)
            )
            .into());
        }
        let allowed = allow_list(&std::fs::read_to_string(root.join("deny.toml"))?)?;
        let refused = refused(&serde_json::from_slice(&out.stdout)?, &allowed);
        if !refused.is_empty() {
            return Err(format!(
                "these production dependencies of ui/ aren't licensed as deny.toml allows (ARCHITECTURE.md, License and dependencies):\n  {}",
                refused.join("\n  ")
            )
            .into());
        }
        println!("ui licenses: every production dependency is allowed.");
    }
    if asked.check {
        for script in ["typecheck", "lint", "test"] {
            pnpm_in(&ui, &["run", script])?;
        }
    }
    if !asked.check && !asked.licenses {
        pnpm_in(&ui, &["run", "build"])?;
        let page = std::fs::read_to_string(ui.join("dist/index.html"))?;
        let inline = inline_code(&page);
        if !inline.is_empty() {
            return Err(format!(
                "ui/dist/index.html has inline {}, which the server's Content-Security-Policy blocks; \
                 load it from a file instead (see ui/vite.config.ts)",
                inline.join(" and ")
            )
            .into());
        }
        // crates/cli/build.rs watches this file while there's no ui/dist/.
        std::fs::File::options()
            .append(true)
            .open(ui.join("package.json"))?
            .set_modified(std::time::SystemTime::now())?;
        println!("Built ui/dist/; `cargo build` now compiles it into the binary.");
    }
    Ok(())
}

/// What the built page runs or styles inline: a `<script>` without a `src`
/// attribute, a `<style>` element, or a `style` attribute on any element.
fn inline_code(page: &str) -> Vec<&'static str> {
    let (mut script, mut style) = (false, false);
    let mut rest = page;
    while let Some(open) = rest.find('<') {
        let (name, attributes, after) = tag(&rest[open + 1..]);
        rest = after;
        style |= name == "style" || attributes.iter().any(|a| a == "style");
        if name == "script" {
            script |= !attributes.iter().any(|a| a == "src");
            // A script's text isn't markup.
            let lower = rest.to_ascii_lowercase();
            rest = lower.find("</script").map_or("", |end| &rest[end..]);
        }
    }
    let mut found = Vec::new();
    if script {
        found.push("script");
    }
    if style {
        found.push("style");
    }
    found
}

/// The tag starting just after a `<`: its lower-case name and attribute
/// names, with quoted values skipped, and the text after its `>`.
fn tag(text: &str) -> (String, Vec<String>, &str) {
    let mut chars = text.char_indices().peekable();
    let name_end = text
        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .unwrap_or(text.len());
    let name = text[..name_end].to_ascii_lowercase();
    while chars.peek().is_some_and(|&(i, _)| i < name_end) {
        chars.next();
    }
    let mut attributes = Vec::new();
    let mut attribute = String::new();
    let mut after_equals = false;
    while let Some((i, c)) = chars.next() {
        match c {
            '>' => {
                if !attribute.is_empty() {
                    attributes.push(attribute.to_ascii_lowercase());
                }
                return (name, attributes, &text[i + 1..]);
            }
            '"' | '\'' if after_equals => {
                // Skip the quoted value.
                for (_, q) in chars.by_ref() {
                    if q == c {
                        break;
                    }
                }
                after_equals = false;
            }
            '=' => {
                if !attribute.is_empty() {
                    attributes.push(attribute.to_ascii_lowercase());
                    attribute.clear();
                }
                after_equals = true;
            }
            c if c.is_whitespace() || c == '/' => {
                if !attribute.is_empty() && !after_equals {
                    attributes.push(attribute.to_ascii_lowercase());
                    attribute.clear();
                }
            }
            _ if after_equals => {
                // An unquoted value runs to whitespace or `>`.
                while chars
                    .peek()
                    .is_some_and(|&(_, n)| !n.is_whitespace() && n != '>')
                {
                    chars.next();
                }
                after_equals = false;
            }
            c => attribute.push(c),
        }
    }
    (name, attributes, "")
}

/// The licenses `deny.toml` allows, the same list `cargo deny` enforces.
///
/// # Errors
///
/// A file that isn't TOML or has no `[licenses] allow` list.
pub fn allow_list(deny: &str) -> Result<BTreeSet<String>, Box<dyn Error>> {
    let table: toml::Table = deny.parse()?;
    let allow = table
        .get("licenses")
        .and_then(|l| l.get("allow"))
        .and_then(toml::Value::as_array)
        .ok_or("deny.toml has no [licenses] allow list")?;
    Ok(allow
        .iter()
        .filter_map(toml::Value::as_str)
        .map(str::to_owned)
        .collect())
}

/// Whether an SPDX expression is satisfied by the allowed licenses, with
/// `AND` binding tighter than `OR` and parentheses grouping, as SPDX defines:
/// one side of an `OR` must be allowed, every side of an `AND`. A license
/// `WITH` an exception is one license, allowed only as a whole. Anything that
/// doesn't parse, such as `Unknown` with extra words or a custom license
/// text, isn't permitted.
fn permitted(expression: &str, allowed: &BTreeSet<String>) -> bool {
    let spaced = expression.replace('(', " ( ").replace(')', " ) ");
    let tokens: Vec<&str> = spaced.split_whitespace().collect();
    let mut at = 0;
    let result = any_of(&tokens, &mut at, allowed);
    result == Some(true) && at == tokens.len()
}

/// `term (OR term)*`; `None` if it doesn't parse.
fn any_of(tokens: &[&str], at: &mut usize, allowed: &BTreeSet<String>) -> Option<bool> {
    let mut found = all_of(tokens, at, allowed)?;
    while tokens.get(*at) == Some(&"OR") {
        *at += 1;
        found |= all_of(tokens, at, allowed)?;
    }
    Some(found)
}

/// `license (AND license)*`; `None` if it doesn't parse.
fn all_of(tokens: &[&str], at: &mut usize, allowed: &BTreeSet<String>) -> Option<bool> {
    let mut found = license(tokens, at, allowed)?;
    while tokens.get(*at) == Some(&"AND") {
        *at += 1;
        found &= license(tokens, at, allowed)?;
    }
    Some(found)
}

/// `( expression )`, or a license id, optionally `WITH` an exception.
fn license(tokens: &[&str], at: &mut usize, allowed: &BTreeSet<String>) -> Option<bool> {
    let token = *tokens.get(*at)?;
    *at += 1;
    if token == "(" {
        let inner = any_of(tokens, at, allowed)?;
        (tokens.get(*at) == Some(&")")).then(|| *at += 1)?;
        return Some(inner);
    }
    if matches!(token, ")" | "AND" | "OR" | "WITH") {
        return None;
    }
    if tokens.get(*at) == Some(&"WITH") {
        let exception = *tokens.get(*at + 1)?;
        if matches!(exception, "(" | ")" | "AND" | "OR" | "WITH") {
            return None;
        }
        *at += 2;
        return Some(allowed.contains(&format!("{token} WITH {exception}")));
    }
    Some(allowed.contains(token))
}

/// Every package in `pnpm licenses list --json` output whose license isn't
/// permitted, as `name@version (license)`, sorted.
pub fn refused(listed: &Value, allowed: &BTreeSet<String>) -> Vec<String> {
    let mut refused = Vec::new();
    let Some(by_license) = listed.as_object() else {
        return vec!["the license list isn't a JSON object".to_owned()];
    };
    for (license, packages) in by_license {
        if permitted(license, allowed) {
            continue;
        }
        for package in packages.as_array().into_iter().flatten() {
            let name = package["name"].as_str().unwrap_or("?");
            let versions: Vec<&str> = package["versions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            refused.push(format!("{name}@{} ({license})", versions.join(",")));
        }
    }
    refused.sort();
    refused
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn allowed() -> BTreeSet<String> {
        allow_list(&std::fs::read_to_string(root().join("deny.toml")).unwrap()).unwrap()
    }

    #[test]
    fn the_allow_list_is_deny_toml_s() {
        let allowed = allowed();
        assert!(allowed.contains("MIT"));
        assert!(allowed.contains("Apache-2.0"));
        assert!(!allowed.iter().any(|l| l.contains("GPL")));
    }

    #[test]
    fn a_gpl_dependency_is_refused() {
        let listed = json!({
            "MIT": [{"name": "react", "versions": ["19.3.1"]}],
            "GPL-3.0-only": [{"name": "copyleft", "versions": ["2.0.0"]}],
        });
        assert_eq!(
            refused(&listed, &allowed()),
            ["copyleft@2.0.0 (GPL-3.0-only)"]
        );
    }

    #[test]
    fn inline_scripts_and_styles_are_found() {
        let built = r#"<script type="module" crossorigin src="/assets/a.js"></script><link rel="stylesheet" href="/a.css">"#;
        assert_eq!(inline_code(built), Vec::<&str>::new());
        assert_eq!(inline_code("<script>alert(1)</script>"), ["script"]);
        assert_eq!(inline_code(r#"<div style="color:red"></div>"#), ["style"]);
        assert_eq!(inline_code("<STYLE>p{}</STYLE>"), ["style"]);
        assert_eq!(inline_code("<body\nstyle=\"color:red\">"), ["style"]);
        assert_eq!(inline_code("<body style = \"color:red\">"), ["style"]);
        assert_eq!(
            inline_code(r#"<script data-note=" src=">alert(1)</script>"#),
            ["script"]
        );
        assert_eq!(
            inline_code(r#"<script data-x='a>b' src=/a.js></script><p title="style=">"#),
            Vec::<&str>::new()
        );
        assert_eq!(
            inline_code("<script src=/a.js></script><script>if (a<b) {}</script>"),
            ["script"]
        );
    }

    #[test]
    fn expressions_need_one_allowed_choice_or_every_part() {
        let allowed = allowed();
        assert!(permitted("(MIT OR GPL-3.0-only)", &allowed));
        assert!(permitted("MIT AND ISC", &allowed));
        assert!(!permitted("MIT AND GPL-3.0-only", &allowed));
        assert!(!permitted("Unknown", &allowed));
        assert!(!permitted("SEE LICENSE IN LICENSE.md", &allowed));
        // AND binds tighter than OR, and a group is one term.
        assert!(!permitted(
            "GPL-3.0-only AND (ISC OR MIT OR Apache-2.0)",
            &allowed
        ));
        assert!(permitted("GPL-3.0-only AND ISC OR MIT", &allowed));
        assert!(!permitted("MIT AND GPL-3.0-only OR GPL-2.0-only", &allowed));
        assert!(permitted("(MIT AND (ISC OR GPL-3.0-only))", &allowed));
        assert!(permitted("Apache-2.0 WITH LLVM-exception", &allowed));
        assert!(!permitted(
            "GPL-2.0-only WITH Classpath-exception-2.0",
            &allowed
        ));
        assert!(!permitted("(MIT", &allowed));
        assert!(!permitted("MIT OR", &allowed));
        assert!(!permitted("GPL-3.0-only WITH ) OR MIT", &allowed));
        assert!(!permitted("GPL-3.0-only WITH AND OR MIT", &allowed));
        assert!(!permitted("", &allowed));
    }
}
