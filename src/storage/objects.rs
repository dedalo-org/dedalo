//! `.dedalo/` as a repository in its own right.
//!
//! The layout is git's, because the problem is git's: a record that many
//! people must be able to check, that lives in the project rather than on a
//! server, and that must make an edit after the fact visible.
//!
//! ```text
//! .dedalo/
//! ├── HEAD                        ref: refs/ledger/main
//! ├── refs/ledger/main            the newest entry's id
//! └── objects/de/dc1f0a….json     content-addressed, one file per object
//! ```
//!
//! Two things are deliberately unlike git.
//!
//! **Objects are plain JSON, not compressed.** Dedalo's claim is that a round
//! is reviewable in a pull request; a zlib blob is not reviewable. The cost is
//! size, and a payout ledger is a few kilobytes a year.
//!
//! **The id is computed over a canonical encoding, not over the file.** JSON
//! has no canonical form — a re-serialisation with different spacing or field
//! order is the same value and a different byte string. Hashing the encoding
//! means reformatting a stored object cannot change its identity, and cannot
//! be used to hide a change either.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::{Error, Result};

/// File naming the branch `HEAD` follows.
pub const HEAD_FILE: &str = "HEAD";
/// The one branch that exists today.
pub const DEFAULT_REF: &str = "refs/ledger/main";
/// Directory holding content-addressed objects.
pub const OBJECTS_DIR: &str = "objects";

/// Number of hex digits in an object id, after its four-character tag.
///
/// Half of a SHA-256, which is what the id generators truncate to. Public
/// because it is part of the on-disk format: a tool that reads `.dedalo/`
/// without linking this crate still has to know how long an id is.
pub const ID_DIGITS: usize = 32;

/// Reject anything that is not an object id.
///
/// Ids arrive from the command line (`--plan <id>`) and from files on disk,
/// and they are turned into paths. Without this, `../../../../etc/passwd` is a
/// readable path and `a/b/c` a writable one. An id is an identifier; it must
/// never be allowed to act as a path.
///
/// # Errors
///
/// Returns [`Error::Config`] unless `id` is `tag` followed by exactly
/// [`ID_DIGITS`] lowercase hex digits.
pub fn validate_id(id: &str, tag: &str) -> Result<()> {
    let body = id
        .strip_prefix(tag)
        .ok_or_else(|| Error::config(format!("`{id}` is not an id: it must start with `{tag}`")))?;
    if body.len() != ID_DIGITS
        || !body
            .bytes()
            .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase() && b <= b'f')
    {
        return Err(Error::config(format!(
            "`{id}` is not an id: expected `{tag}` and {ID_DIGITS} lowercase hex digits"
        )));
    }
    Ok(())
}

/// Reads and writes the objects and refs under `.dedalo/`.
#[derive(Debug, Clone)]
pub struct ObjectStore {
    dir: PathBuf,
}

impl ObjectStore {
    /// Point at a `.dedalo` directory without creating it.
    ///
    /// Nothing is created until something is written, so `dedalo scan` works
    /// on a read-only checkout and on a repository mounted `:ro` into a
    /// container.
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// The directory this store lives in.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Where an object with this id is stored.
    ///
    /// Sharded on the first two characters, as git does: one flat directory
    /// with thousands of entries is slow to list on every filesystem that has
    /// ever shipped.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] if `id` is not a well-formed object id.
    pub fn path_of(&self, id: &str, tag: &str) -> Result<PathBuf> {
        validate_id(id, tag)?;
        let (shard, rest) = id.split_at(2);
        Ok(self
            .dir
            .join(OBJECTS_DIR)
            .join(shard)
            .join(format!("{rest}.json")))
    }

    /// Whether an object is already stored.
    pub fn contains(&self, id: &str, tag: &str) -> Result<bool> {
        Ok(self.path_of(id, tag)?.is_file())
    }

    /// Store an object under its id.
    ///
    /// Writing is idempotent: the id is the content, so a second write of the
    /// same object is the same bytes. Returns where it landed.
    pub fn write<T: Serialize>(&self, id: &str, tag: &str, value: &T) -> Result<PathBuf> {
        let path = self.path_of(id, tag)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let mut raw = serde_json::to_string_pretty(value)?;
        raw.push('\n');
        std::fs::write(&path, raw).map_err(|e| Error::io(&path, e))?;
        Ok(path)
    }

    /// Read an object back.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if it is not stored, and [`Error::Serde`] if the
    /// file is not the shape `T` expects.
    pub fn read<T: DeserializeOwned>(&self, id: &str, tag: &str) -> Result<T> {
        let path = self.path_of(id, tag)?;
        let raw = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
        Ok(serde_json::from_str(&raw)?)
    }

    /// The ref `HEAD` points at, defaulting to [`DEFAULT_REF`].
    ///
    /// A `HEAD` naming something outside `refs/` is refused rather than
    /// followed: it is a path, and it arrives from a file anyone can edit.
    pub fn head_ref(&self) -> Result<String> {
        let path = self.dir.join(HEAD_FILE);
        if !path.is_file() {
            return Ok(DEFAULT_REF.to_string());
        }
        let raw = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
        let name = raw
            .trim()
            .strip_prefix("ref:")
            .ok_or_else(|| Error::config(format!("{}: expected `ref: <name>`", path.display())))?
            .trim()
            .to_string();
        if !name.starts_with("refs/") || name.contains("..") || name.contains('\\') {
            return Err(Error::config(format!(
                "{}: `{name}` is not a ref under refs/",
                path.display()
            )));
        }
        Ok(name)
    }

    /// The id `HEAD` resolves to, or `None` when nothing has been recorded.
    pub fn head(&self) -> Result<Option<String>> {
        let path = self.dir.join(self.head_ref()?);
        if !path.is_file() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
        let id = raw.trim().to_string();
        if id.is_empty() {
            Ok(None)
        } else {
            Ok(Some(id))
        }
    }

    /// Move `HEAD`'s ref to `id`, creating `HEAD` and the ref if needed.
    pub fn set_head(&self, id: &str) -> Result<()> {
        let name = self.head_ref()?;
        let head_path = self.dir.join(HEAD_FILE);
        if !head_path.is_file() {
            std::fs::create_dir_all(&self.dir).map_err(|e| Error::io(&self.dir, e))?;
            std::fs::write(&head_path, format!("ref: {name}\n"))
                .map_err(|e| Error::io(&head_path, e))?;
        }
        let ref_path = self.dir.join(&name);
        if let Some(parent) = ref_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        std::fs::write(&ref_path, format!("{id}\n")).map_err(|e| Error::io(&ref_path, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(tag: &str) -> ObjectStore {
        let dir = std::env::temp_dir().join(format!(
            "dedalo-store-{tag}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        ObjectStore::at(dir)
    }

    const TAG: &str = "ded1";
    const ID: &str = "ded100112233445566778899aabbccddeeff";

    #[test]
    fn an_id_may_never_act_as_a_path() {
        for hostile in [
            "../../../../etc/passwd",
            "ded1../../../etc/passwd",
            "a/b/c",
            "ded1",
            "",
            // Upper case would collide with the lower-case form on a
            // case-insensitive filesystem, which is most of them.
            "ded100112233445566778899AABBCCDDEEFF",
            // Right shape, one digit short.
            "ded100112233445566778899aabbccddeef",
        ] {
            assert!(validate_id(hostile, TAG).is_err(), "accepted `{hostile}`");
        }
        assert!(validate_id(ID, TAG).is_ok());
    }

    #[test]
    fn objects_round_trip_and_shard_on_the_first_two_characters() {
        let store = store("roundtrip");
        assert!(!store.contains(ID, TAG).unwrap());

        let value = serde_json::json!({ "hello": "world" });
        let path = store.write(ID, TAG, &value).unwrap();
        assert!(path.ends_with("objects/de/d100112233445566778899aabbccddeeff.json"));
        assert!(store.contains(ID, TAG).unwrap());

        let read: serde_json::Value = store.read(ID, TAG).unwrap();
        assert_eq!(read, value);
    }

    #[test]
    fn head_defaults_to_the_one_branch_and_moves_when_set() {
        let store = store("head");
        assert_eq!(store.head_ref().unwrap(), DEFAULT_REF);
        assert_eq!(store.head().unwrap(), None);

        store.set_head(ID).unwrap();
        assert_eq!(store.head().unwrap().as_deref(), Some(ID));
        assert_eq!(
            std::fs::read_to_string(store.dir().join(HEAD_FILE)).unwrap(),
            format!("ref: {DEFAULT_REF}\n")
        );
    }

    #[test]
    fn a_head_pointing_outside_refs_is_refused_not_followed() {
        let store = store("escape");
        std::fs::create_dir_all(store.dir()).unwrap();
        for hostile in [
            "ref: ../../../../etc/passwd",
            "ref: refs/../../etc",
            "nonsense",
        ] {
            std::fs::write(store.dir().join(HEAD_FILE), format!("{hostile}\n")).unwrap();
            assert!(store.head_ref().is_err(), "followed `{hostile}`");
        }
    }
}
