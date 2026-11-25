//! Writes generated files without replacing anything that differs.

use std::collections::BTreeMap;
use std::error::Error;
use std::io::Write;
use std::path::Path;

/// Writes `files` into `out`. A file already there must hold the same bytes, so
/// a regeneration can't silently replace reviewed output.
pub(crate) fn write(out: &Path, files: BTreeMap<String, Vec<u8>>) -> Result<(), Box<dyn Error>> {
    for (name, bytes) in &files {
        let path = out.join(name);
        if let Ok(meta) = std::fs::symlink_metadata(&path)
            && (!meta.is_file() || std::fs::read(path)? != *bytes)
        {
            return Err(format!("refusing to replace {name}; use a new directory").into());
        }
    }
    std::fs::create_dir_all(out)?;
    for (name, bytes) in files {
        let path = out.join(name);
        if !path.exists() {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?
                .write_all(&bytes)?;
        }
    }
    Ok(())
}
