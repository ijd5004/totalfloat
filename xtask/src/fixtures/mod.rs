//! Deterministic authored fixture batches, never schedule calculations.
use clap::Subcommand;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    error::Error,
    fmt::Write as _,
    path::{Path, PathBuf},
};
use totalfloat_xer::fresh::{FreshOptions, write};
mod model;
mod spec;
#[derive(Debug, Subcommand)]
pub(crate) enum Action {
    /// Build TOML specs in a directory into synthetic XERs, a manifest and a README.
    Build {
        directory: PathBuf,
        /// Output directory (defaults to the spec directory). Existing differing files are refused.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}
pub(crate) fn run(action: Action) -> Result<(), Box<dyn Error>> {
    match action {
        Action::Build { directory, out } => build(&directory, out.as_deref().unwrap_or(&directory)),
    }
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn cell(value: &str) -> String {
    value.replace('|', "\\|").replace(['\r', '\n'], " ")
}
fn render(directory: &Path) -> Result<BTreeMap<String, Vec<u8>>, Box<dyn Error>> {
    let mut paths = std::fs::read_dir(directory)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.retain(|p| p.extension().is_some_and(|v| v == "toml"));
    paths.sort();
    if paths.is_empty() {
        return Err("no TOML fixture specs found".into());
    }
    let mut output = BTreeMap::new();
    let mut project_guids = std::collections::BTreeSet::new();
    let mut manifest = String::from(
        "# Batch manifest\n\nAuthored synthetic inputs, Apache-2.0, with no stored results. Expected results come from TotalFloat's written rules (docs/rules/).\n\n| File | Input spec SHA-256 | XER SHA-256 | Description | Results |\n|---|---|---|---|---|\n",
    );
    for path in paths {
        if std::fs::symlink_metadata(&path)?.file_type().is_symlink() {
            return Err("spec symlinks are refused".into());
        }
        let name = path
            .file_stem()
            .and_then(|v| v.to_str())
            .ok_or("invalid spec filename")?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_".contains(&b))
        {
            return Err(
                "spec filenames require lowercase ASCII letters, digits, hyphens or underscores"
                    .into(),
            );
        }
        let source = std::fs::read_to_string(&path)?;
        let spec: spec::Spec = toml::from_str(&source)?;
        let schedule = model::build(&spec, name)?;
        let mut options = FreshOptions::new(spec.export_date.parse()?);
        options.seed = spec.seed;
        let xer = write(&schedule, options)?;
        let project = xer.table("PROJECT").ok_or("missing generated project")?;
        let guid = project.rows[0]
            .get(project.field_index("guid").ok_or("missing project GUID")?)
            .ok_or("missing project GUID value")?;
        if !project_guids.insert(guid.to_owned()) {
            return Err("duplicate project GUID across specs; choose distinct seeds".into());
        }
        let mut bytes = Vec::new();
        xer.write(&mut bytes)?;
        writeln!(
            manifest,
            "| `{name}.xer` | `{}` | `{}` | {} | none stored |",
            digest(source.as_bytes()),
            digest(&bytes),
            cell(&spec.description)
        )?;
        output.insert(format!("{name}.xer"), bytes);
    }
    output.insert("MANIFEST.md".to_owned(), manifest.into_bytes());
    output.insert(
        "README.md".to_owned(),
        include_bytes!("batch-readme.md").to_vec(),
    );
    Ok(output)
}
fn build(directory: &Path, out: &Path) -> Result<(), Box<dyn Error>> {
    let files = render(directory)?;
    let built = files.len() - 2;
    crate::generated::write(out, files)?;
    println!("built {built} synthetic XER fixtures, with no stored results");
    Ok(())
}
#[cfg(test)]
mod tests;
