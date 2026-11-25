//! New output files: created whole or not at all, and never over an existing one.

use std::fs::Permissions;
use std::io::{BufWriter, Write};
use std::path::Path;

use totalfloat_xer::RawXer;

use crate::error::{CliError, exit};

fn write_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(
        "output-not-written",
        format!("can't write new output: {error}"),
        "use a new writable path; existing files are never replaced",
    )
}

/// Hours for reading, to two decimal places. JSON keeps the exact value.
#[must_use]
pub fn rounded(hours: f64) -> f64 {
    (hours * 100.0).round() / 100.0
}

/// Minutes as exact hours.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    reason = "display hours; minutes are far below 2^52"
)]
pub fn hours_of(minutes: totalfloat_model::Minutes) -> f64 {
    minutes.get() as f64 / 60.0
}

/// The same text, ignoring case in any alphabet.
#[must_use]
pub fn same_text(a: &str, b: &str) -> bool {
    a == b || a.to_lowercase() == b.to_lowercase()
}

/// Refuses an output path that's already taken.
pub fn refuse_existing(output: &Path) -> Result<(), CliError> {
    if output.exists() {
        return Err(already_exists());
    }
    Ok(())
}

fn already_exists() -> CliError {
    CliError::new(
        "output-exists",
        "output path already exists",
        "choose a new output path; source and existing files are never replaced",
    )
}

/// The permissions a file written from `source` should have.
pub fn permissions_of(source: &Path) -> Result<Permissions, CliError> {
    let source = crate::location::file_of(source);
    Ok(std::fs::metadata(source)
        .map_err(write_error)?
        .permissions())
}

/// Writes `xer` to a new file with these permissions. A temporary file is
/// private, so without this the output would be readable only by its owner.
pub fn write_new(xer: &RawXer, permissions: Permissions, output: &Path) -> Result<(), CliError> {
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(write_error)?;
    let mut buffered = BufWriter::with_capacity(1 << 20, temporary.as_file_mut());
    xer.write(&mut buffered).map_err(write_error)?;
    buffered.flush().map_err(write_error)?;
    drop(buffered);
    temporary
        .as_file()
        .set_permissions(permissions)
        .map_err(write_error)?;
    temporary.persist_noclobber(output).map_err(write_error)?;
    Ok(())
}

/// Fills a new file with the system's usual permissions, for output with no
/// source file to take them from.
fn create_with(
    output: &Path,
    fill: impl FnOnce(&mut std::fs::File) -> Result<(), CliError>,
) -> Result<(), CliError> {
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let builder = tempfile::Builder::new();
    // The system's usual mask applies to this, as for any new file. Only Unix
    // changes the builder, so only Unix binds it as `mut`.
    #[cfg(unix)]
    let builder = {
        let mut builder = builder;
        builder.permissions(std::os::unix::fs::PermissionsExt::from_mode(0o666));
        builder
    };
    let mut temporary = builder.tempfile_in(parent).map_err(write_error)?;
    fill(temporary.as_file_mut())?;
    match temporary.persist_noclobber(output) {
        Ok(_) => Ok(()),
        Err(failed) if failed.error.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(already_exists())
        }
        Err(failed) => Err(write_error(failed.error)),
    }
}

/// Writes `xer` to a new file, as [`create_with`] does.
pub fn create_new(xer: &RawXer, output: &Path) -> Result<(), CliError> {
    create_with(output, |file| {
        let mut buffered = BufWriter::with_capacity(1 << 20, file);
        xer.write(&mut buffered).map_err(write_error)?;
        buffered.flush().map_err(write_error)
    })
}

/// Writes bytes to a new file, as [`create_with`] does.
pub fn create_new_bytes(bytes: &[u8], output: &Path) -> Result<(), CliError> {
    create_with(output, |file| file.write_all(bytes).map_err(write_error))
}

/// Writes text to a new file, as [`create_with`] does.
pub fn create_new_text(text: &str, output: &Path) -> Result<(), CliError> {
    create_with(output, |file| {
        file.write_all(text.as_bytes()).map_err(write_error)
    })
}

thread_local! {
    /// What [`print`] would have printed on this thread, while [`captured`]
    /// runs.
    static CAPTURED: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Runs `work` with everything it prints kept instead, and gives both back.
/// MCP uses it to run a command whose output is the tool's answer, since its
/// own standard output carries the protocol.
pub fn captured<T>(work: impl FnOnce() -> T) -> (T, String) {
    /// Puts back what was there, even if `work` panics, so a pooled thread
    /// doesn't go on swallowing output.
    struct Restore(Option<String>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let before = self.0.take();
            CAPTURED.with_borrow_mut(|held| *held = before);
        }
    }
    let restore = Restore(CAPTURED.with_borrow_mut(|held| held.replace(String::new())));
    let result = work();
    let text = CAPTURED.with_borrow_mut(Option::take).unwrap_or_default();
    drop(restore);
    (result, text)
}

/// Prints to standard output. A reader that has gone away, like `head`, has
/// everything it wanted, so that ends the command quietly.
pub fn print(text: &str) {
    let kept = CAPTURED
        .with_borrow_mut(|held| held.as_mut().map(|buffer| buffer.push_str(text)).is_some());
    if kept {
        return;
    }
    let mut out = std::io::stdout().lock();
    if let Err(error) = out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        if error.kind() != std::io::ErrorKind::BrokenPipe {
            eprintln!("error[write-failed]: can't write to standard output: {error}");
            std::process::exit(i32::from(exit::ERROR));
        }
        std::process::exit(i32::from(exit::OK));
    }
}

/// [`print`], with a line ending.
pub fn print_line(text: &str) {
    print(&format!("{text}\n"));
}

#[cfg(test)]
mod tests {
    use super::rounded;

    #[test]
    fn hours_round_to_two_places() {
        assert_eq!(format!("{:+}", rounded(977.666_666_666_666_6)), "+977.67");
        assert_eq!(format!("{:+}", rounded(-8.0)), "-8");
        assert_eq!(rounded(1394.85), 1394.85);
    }
}
