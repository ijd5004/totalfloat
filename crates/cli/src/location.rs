//! Where a command reads a schedule from: an XER file, a history store's
//! checked-out branch (`plant.tfdb`), or any point in one
//! (`plant.tfdb@what-if`, `plant.tfdb@3f9a2c`).

use std::path::{Path, PathBuf};

use totalfloat_store::{StoreError, looks_like_store};
use totalfloat_xer::RawXer;

use crate::error::{CliError, read_xer, read_xer_bytes};
use crate::history::{open, snapshot_at};

/// A place a schedule is read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Located {
    /// A file, read as it is.
    File(PathBuf),
    /// A history store, at a named point or its checked-out branch.
    Store {
        /// The store file.
        path: PathBuf,
        /// A branch, baseline or changeset ID; `None` is the checked-out branch.
        at: Option<String>,
    },
}

/// Where `path` points. A path that exists is taken as written, so a file with
/// `@` in its name still opens; otherwise `STORE@POINT` names a point in a
/// store.
pub fn locate(path: &Path) -> Located {
    if path.exists() {
        return if looks_like_store(path) {
            Located::Store {
                path: path.to_owned(),
                at: None,
            }
        } else {
            Located::File(path.to_owned())
        };
    }
    // The store is the shortest prefix before an `@` that is one, so a point
    // whose name holds `@` still reads.
    if let Some(text) = path.to_str() {
        for (at, _) in text.match_indices('@') {
            let (store, point) = (Path::new(&text[..at]), &text[at + 1..]);
            if !point.is_empty() && looks_like_store(store) {
                return Located::Store {
                    path: store.to_owned(),
                    at: Some(point.to_owned()),
                };
            }
        }
    }
    Located::File(path.to_owned())
}

/// The file on disk behind a location, such as for its permissions.
pub fn file_of(path: &Path) -> PathBuf {
    match locate(path) {
        Located::File(file) | Located::Store { path: file, .. } => file,
    }
}

/// Reads the schedule a location names: a file, or a point in a store. A
/// SQLite file that isn't a store is refused as not an XER.
///
/// # Errors
///
/// `not-an-xer`, `no-such-ref`, or what the file or store refuses.
pub fn read(path: &Path) -> Result<RawXer, CliError> {
    let (path, at) = match locate(path) {
        Located::File(file) => return read_xer(&file),
        Located::Store { path, at } => (path, at),
    };
    let store = match totalfloat_store::Store::open(&path) {
        Ok(store) => store,
        Err(StoreError::NotAStore(_)) => {
            return Err(CliError::new(
                "not-an-xer",
                format!(
                    "{} is a database, not an XER file or a history store",
                    path.display()
                ),
                "pass a Primavera P6 .xer export, or a store from `totalfloat history init`",
            ));
        }
        Err(_) => open(&path)?,
    };
    let (_, bytes) = snapshot_at(&store, &path, at.as_deref())?;
    read_xer_bytes(bytes, &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_that_exists_is_taken_as_written() {
        let directory = tempfile::tempdir().unwrap();
        let odd = directory.path().join("plan@v2.xer");
        std::fs::write(&odd, "ERMHDR\n").unwrap();
        assert_eq!(locate(&odd), Located::File(odd.clone()));
        let missing = directory.path().join("nowhere.tfdb@main");
        assert_eq!(locate(&missing), Located::File(missing.clone()));
        let store = directory.path().join("plan.tfdb");
        drop(totalfloat_store::Store::create(&store).unwrap());
        assert_eq!(
            locate(&store),
            Located::Store {
                path: store.clone(),
                at: None
            }
        );
        let point = PathBuf::from(format!("{}@what-if", store.display()));
        assert_eq!(
            locate(&point),
            Located::Store {
                path: store.clone(),
                at: Some("what-if".to_owned())
            }
        );
        assert_eq!(file_of(&point), store);
        let odd_point = PathBuf::from(format!("{}@rev@2", store.display()));
        assert_eq!(
            locate(&odd_point),
            Located::Store {
                path: store.clone(),
                at: Some("rev@2".to_owned())
            }
        );
    }
}
