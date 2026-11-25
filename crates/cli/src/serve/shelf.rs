//! Files kept beside the schedule served, one JSON file per name, in a
//! folder named for the schedule: layouts (`plant.layouts/`) and named
//! filters (`plant.filters/`). Names are portable file names, compared
//! without case, and each file is written whole before it replaces another.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};

use super::view::{Source, View};
use crate::error::CliError;

/// What a shelf holds, and how it's named and checked.
pub struct Kind {
    /// The folder's extension beside the schedule: `layouts`.
    pub folder: &'static str,
    /// What one is called: `layout`.
    pub noun: &'static str,
    /// The error for a name that can't be a file: `bad-layout-name`.
    pub bad_name: &'static str,
    /// The error for a name with no file: `no-such-layout`.
    pub missing: &'static str,
    /// The error for a saved file that doesn't read: `bad-layout-file`.
    pub bad_file: &'static str,
    /// Its published schema, for hints: `docs/schemas/layout-1.schema.json`.
    pub schema_file: &'static str,
    /// Checks a file read back, or says why it doesn't fit.
    pub check: fn(&[u8]) -> Result<(), CliError>,
    /// Reads a body and gives the file as it's saved, or why it can't be.
    pub canonical: fn(&[u8]) -> Result<Vec<u8>, CliError>,
}

/// Checks a name: it names a file in the folder, and nothing outside it.
///
/// # Errors
///
/// The kind's `bad_name`.
pub fn checked_name<'a>(kind: &Kind, name: &'a str) -> Result<&'a str, CliError> {
    // Names Windows keeps for devices, whatever follows a dot.
    const DEVICES: [&str; 22] = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
        "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    let stem = name
        .split('.')
        .next()
        .unwrap_or(name)
        .trim_end()
        .to_ascii_lowercase();
    let fine = !name.is_empty()
        && name.len() <= 64
        && !name.starts_with(['.', ' '])
        && !name.ends_with([' ', '.'])
        && !DEVICES.contains(&stem.as_str())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
        && !name.contains("..");
    if fine {
        Ok(name)
    } else {
        Err(CliError::new(
            kind.bad_name,
            format!("{name:?} can't name a {}", kind.noun),
            "use up to 64 letters, digits, spaces, -, _ and ., not starting or ending with . or a space, and not a device name such as CON",
        ))
    }
}

/// The folder a view's files of this kind live in: `plant.layouts/` beside
/// `plant.tfdb` or `plant.xer`.
pub fn directory(kind: &Kind, view: &View) -> PathBuf {
    let served: &Path = match view.source() {
        Source::File(file) => file,
        Source::Store { path, .. } => path,
    };
    served.with_extension(kind.folder)
}

fn file_of(kind: &Kind, view: &View, name: &str) -> Result<PathBuf, CliError> {
    Ok(directory(kind, view).join(format!("{}.json", checked_name(kind, name)?)))
}

/// The file saved under `name`, compared without case, as filesystems on
/// Windows and macOS do, if there is one.
fn existing(kind: &Kind, view: &View, name: &str) -> Result<Option<PathBuf>, CliError> {
    checked_name(kind, name)?;
    let wanted = name.to_lowercase();
    Ok(list(kind, view)?
        .into_iter()
        .find(|saved| saved.to_lowercase() == wanted)
        .map(|saved| directory(kind, view).join(format!("{saved}.json"))))
}

fn io_error(kind: &Kind, what: &str, error: &std::io::Error) -> CliError {
    CliError::new(
        "serve-failed",
        format!("{what}: {error}"),
        format!(
            "check the {} directory beside the schedule can be read and written",
            kind.folder
        ),
    )
}

fn no_such(kind: &Kind, name: &str) -> CliError {
    CliError::new(
        kind.missing,
        format!("there's no {} {name:?}", kind.noun),
        format!("GET /api/1/{} lists them", kind.folder),
    )
}

/// The names saved, in order.
///
/// # Errors
///
/// `serve-failed` if the folder can't be read.
pub fn list(kind: &Kind, view: &View) -> Result<Vec<String>, CliError> {
    let dir = directory(kind, view);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(io_error(
                kind,
                &format!("the {} couldn't be listed", kind.folder),
                &error,
            ));
        }
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file = entry.file_name().into_string().ok()?;
            let name = file.strip_suffix(".json")?;
            checked_name(kind, name).ok().map(str::to_owned)
        })
        .collect();
    names.sort();
    Ok(names)
}

/// A file, as saved.
///
/// # Errors
///
/// The kind's `missing`, `bad_name` or `bad_file`, or `serve-failed`.
pub fn read(kind: &Kind, view: &View, name: &str) -> Result<Vec<u8>, CliError> {
    let file = existing(kind, view, name)?.ok_or_else(|| no_such(kind, name))?;
    let bytes = std::fs::read(&file).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            no_such(kind, name)
        } else {
            io_error(kind, &format!("the {} couldn't be read", kind.noun), &error)
        }
    })?;
    // A file edited by hand, or shared, is checked as a saved one is.
    (kind.check)(&bytes).map_err(|error| {
        CliError::new(
            kind.bad_file,
            format!(
                "the {} file {}: {}",
                kind.noun,
                file.display(),
                error.message
            ),
            format!(
                "fix the file or delete it; {} says what fits",
                kind.schema_file
            ),
        )
    })?;
    Ok(bytes)
}

/// Saves a file, replacing any of the same name, and returns it.
///
/// # Errors
///
/// The kind's `bad_name`, what its `canonical` refuses, or `serve-failed`.
pub fn write(kind: &Kind, view: &View, name: &str, body: &[u8]) -> Result<Vec<u8>, CliError> {
    static PARTIALS: AtomicU64 = AtomicU64::new(0);
    let noun = kind.noun;
    let file = file_of(kind, view, name)?;
    let bytes = (kind.canonical)(body)?;
    let dir = directory(kind, view);
    std::fs::create_dir_all(&dir).map_err(|e| {
        io_error(
            kind,
            &format!("the {} directory couldn't be made", kind.folder),
            &e,
        )
    })?;
    // The same name in another case is the same file, renamed.
    let before = existing(kind, view, name)?.filter(|old| *old != file);
    // Written whole to a file of its own, then moved into place, so a reader
    // never sees half of it; the partial file goes whatever happens.
    let partial = dir.join(format!(
        ".{}.{}.{}.partial",
        checked_name(kind, name)?,
        std::process::id(),
        PARTIALS.fetch_add(1, Ordering::Relaxed)
    ));
    // A file renamed only in case is moved aside first, to a name of its own
    // (on a filesystem that ignores case, the new name is the old one), and
    // moved back if the save fails; it goes only once the new file is in.
    let aside = before.as_ref().map(|_| {
        dir.join(format!(
            ".{}.{}.{}.aside",
            checked_name(kind, name).unwrap_or(noun),
            std::process::id(),
            PARTIALS.fetch_add(1, Ordering::Relaxed)
        ))
    });
    let saved = std::fs::write(&partial, &bytes)
        .map_err(|e| io_error(kind, &format!("the {noun} couldn't be written"), &e))
        .and_then(|()| {
            if let (Some(old), Some(aside)) = (&before, &aside) {
                std::fs::rename(old, aside).map_err(|e| {
                    io_error(
                        kind,
                        &format!("the {noun}'s old name couldn't be replaced"),
                        &e,
                    )
                })?;
            }
            std::fs::rename(&partial, &file).map_err(|e| {
                if let (Some(old), Some(aside)) = (&before, &aside) {
                    let _ = std::fs::rename(aside, old);
                }
                io_error(kind, &format!("the {noun} couldn't be saved"), &e)
            })
        });
    if saved.is_err() {
        let _ = std::fs::remove_file(&partial);
    } else if let Some(aside) = &aside {
        let _ = std::fs::remove_file(aside);
    }
    saved.map(|()| bytes)
}

/// Deletes a file.
///
/// # Errors
///
/// The kind's `missing` or `bad_name`, or `serve-failed`.
pub fn delete(kind: &Kind, view: &View, name: &str) -> Result<(), CliError> {
    let file = existing(kind, view, name)?.ok_or_else(|| no_such(kind, name))?;
    std::fs::remove_file(&file).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            no_such(kind, name)
        } else {
            io_error(
                kind,
                &format!("the {} couldn't be deleted", kind.noun),
                &error,
            )
        }
    })
}

/// A saved file's bytes as the answer.
pub fn bytes_response(bytes: Vec<u8>) -> Response {
    ([(CONTENT_TYPE, "application/json")], bytes).into_response()
}

/// Runs file work off the async threads.
pub async fn blocking<T: Send + 'static>(
    kind: &'static Kind,
    work: impl FnOnce() -> Result<T, CliError> + Send + 'static,
) -> Result<T, CliError> {
    tokio::task::spawn_blocking(work).await.unwrap_or_else(|_| {
        Err(CliError::new(
            "serve-failed",
            "the request stopped unexpectedly",
            format!("this is a bug; the {} are unaffected", kind.folder),
        ))
    })
}
