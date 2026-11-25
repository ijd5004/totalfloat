//! File snapshots: zlib-compressed, keyed by the SHA-256 of their bytes, so
//! the same file is stored once however many changesets hold it. Reading
//! checks zlib's own checksum and the size, which is cheap; `verify` also
//! checks the SHA-256, which on a CPU without SHA instructions costs about
//! as much as the rest of an edit's reading.

use std::fmt;
use std::io::{Read as _, Write as _};

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};

use crate::{Result, StoreError};

/// The SHA-256 of a file's bytes, as 64 lowercase hex digits.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlobHash(String);

impl BlobHash {
    /// The hash of these bytes.
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        Self(hex(&Sha256::digest(bytes)))
    }

    pub(crate) fn from_stored(text: String) -> Result<Self> {
        if is_digest(&text) {
            Ok(Self(text))
        } else {
            Err(StoreError::Damaged(format!("{text} isn't a file hash")))
        }
    }

    /// The hash as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BlobHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// True for 64 lowercase hex digits, as a SHA-256 is written.
pub(crate) fn is_digest(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Bytes as lowercase hex digits.
pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

fn io(error: &std::io::Error) -> StoreError {
    StoreError::Io(error.to_string())
}

/// Stores bytes unless the same bytes are already stored.
pub(crate) fn put(connection: &Connection, bytes: &[u8]) -> Result<BlobHash> {
    put_hashed(connection, bytes, BlobHash::of(bytes))
}

/// Stores bytes whose hash the caller already has.
pub(crate) fn put_hashed(
    connection: &Connection,
    bytes: &[u8],
    hash: BlobHash,
) -> Result<BlobHash> {
    let held: Option<i64> = connection
        .query_row(
            "SELECT size FROM blobs WHERE hash = ?1",
            [hash.as_str()],
            |row| row.get(0),
        )
        .optional()?;
    if held.is_some() {
        return Ok(hash);
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(bytes).map_err(|e| io(&e))?;
    let data = encoder.finish().map_err(|e| io(&e))?;
    let size = i64::try_from(bytes.len()).map_err(|_| StoreError::Io("file too large".into()))?;
    connection.execute(
        "INSERT INTO blobs (hash, size, data) VALUES (?1, ?2, ?3)",
        params![hash.as_str(), size, data],
    )?;
    Ok(hash)
}

/// The bytes stored under `hash`, checked by size and zlib's checksum.
/// Reading stops one byte past the stored size, so a damaged row can't make
/// it read without end.
pub(crate) fn get(connection: &Connection, hash: &BlobHash) -> Result<Vec<u8>> {
    let row: Option<(i64, Vec<u8>)> = connection
        .query_row(
            "SELECT size, data FROM blobs WHERE hash = ?1",
            [hash.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (size, data) = row.ok_or_else(|| StoreError::NotFound(hash.to_string()))?;
    let damaged = |why: String| StoreError::Damaged(format!("file {hash} {why}"));
    let size = u64::try_from(size).map_err(|_| damaged(format!("has size {size}")))?;
    let mut bytes = Vec::new();
    ZlibDecoder::new(data.as_slice())
        .take(size.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| damaged(format!("can't be read: {e}")))?;
    if u64::try_from(bytes.len()).ok() != Some(size) {
        return Err(damaged("isn't the size it was stored at".to_owned()));
    }
    Ok(bytes)
}

/// [`get`], also checked against the full hash.
pub(crate) fn get_verified(connection: &Connection, hash: &BlobHash) -> Result<Vec<u8>> {
    let bytes = get(connection, hash)?;
    if BlobHash::of(&bytes) != *hash {
        return Err(StoreError::Damaged(format!(
            "file {hash} doesn't match its hash"
        )));
    }
    Ok(bytes)
}

/// Every stored file's hash, as stored.
pub(crate) fn stored_hashes(connection: &Connection) -> Result<Vec<String>> {
    let mut statement = connection.prepare("SELECT hash FROM blobs ORDER BY hash")?;
    let rows = statement
        .query_map([], |row| row.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(rows)
}
