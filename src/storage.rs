//! Durable single-file publication and recoverable station checkpoints.
use anyhow::{Context, Result, bail};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

/// Held for the lifetime of a writer. The OS releases the lock on process exit.
pub struct WorkspaceLock {
    _file: File,
}
impl Drop for WorkspaceLock {
    fn drop(&mut self) {
        // Release even if a concurrently spawned child briefly inherited the fd.
        let _ = self._file.unlock();
    }
}
impl WorkspaceLock {
    pub fn acquire(workspace: &Path) -> Result<Self> {
        let runtime = workspace.join("runtime");
        fs::create_dir_all(&runtime)?;
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(runtime.join("lock"))?;
        file.try_lock()
            .context("workspace is already being written by another process")?;
        Ok(Self { _file: file })
    }
}

/// The temporary file lives beside the destination so rename stays on one filesystem.
/// Sync the new contents before publication and the directory after publication.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("output has no parent")?;
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|error| error.error)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

/// Publish a new artifact atomically, without replacing an existing export.
pub fn atomic_write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist_noclobber(path).map_err(|error| error.error)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

pub fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    atomic_write(path, &bytes)
}

/// A cache key includes all source, timeline, station, model and parameter inputs.
/// The caller supplies a serializable tuple rather than an ambiguous joined string.
pub fn cache_key(identity: &impl Serialize) -> Result<String> {
    Ok(sha256_hex(serde_json::to_vec(identity)?))
}

pub fn sha256_hex(bytes: impl AsRef<[u8]>) -> String {
    hex(Sha256::digest(bytes))
}

pub fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub struct Checkpoints {
    directory: PathBuf,
}
impl Checkpoints {
    pub fn new(sidecar: &Path) -> Self {
        Self {
            directory: sidecar.join("staging/checkpoints"),
        }
    }
    fn path(&self, key: &str) -> Result<PathBuf> {
        if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("invalid checkpoint key");
        }
        Ok(self.directory.join(format!("{key}.json")))
    }
    pub fn save(&self, key: &str, value: &impl Serialize) -> Result<()> {
        write_json(&self.path(key)?, value)
    }
    pub fn load<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        match fs::read(self.path(key)?) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()), // Torn/corrupt checkpoints are recomputed.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_one_writer_and_lock_can_be_reacquired() {
        let dir = tempfile::tempdir().unwrap();
        let lock = WorkspaceLock::acquire(dir.path()).unwrap();
        assert!(WorkspaceLock::acquire(dir.path()).is_err());
        drop(lock);
        assert!(WorkspaceLock::acquire(dir.path()).is_ok());
    }
    #[test]
    fn failed_unpublished_write_preserves_previous_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("annotation.jsonl");
        atomic_write(&path, b"old\n").unwrap();
        {
            let mut interrupted = tempfile::NamedTempFile::new_in(dir.path()).unwrap();
            interrupted.write_all(b"partial").unwrap();
        }
        assert_eq!(fs::read(&path).unwrap(), b"old\n");
        atomic_write(&path, b"new\n").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new\n");
    }
    #[test]
    fn interrupted_export_can_retry_without_overwriting_completed_evidence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("candidates.jsonl");
        {
            let mut interrupted = tempfile::NamedTempFile::new_in(dir.path()).unwrap();
            interrupted.write_all(b"{partial").unwrap();
            interrupted.as_file().sync_all().unwrap();
            assert!(!path.exists());
        }
        atomic_write_new(&path, b"{\"complete\":true}\n").unwrap();
        assert!(atomic_write_new(&path, b"replacement\n").is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{\"complete\":true}\n");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    fn successful_unit_survives_restart_but_changed_source_does_not_reuse_it() {
        let dir = tempfile::tempdir().unwrap();
        let key = cache_key(&("dataset/12", "hash-a", [120, 150], "embed", "space", 1)).unwrap();
        Checkpoints::new(dir.path())
            .save(&key, &vec![1.0, 2.0])
            .unwrap();
        let reloaded = Checkpoints::new(dir.path());
        assert_eq!(
            reloaded.load::<Vec<f32>>(&key).unwrap(),
            Some(vec![1.0, 2.0])
        );
        let changed =
            cache_key(&("dataset/12", "hash-b", [120, 150], "embed", "space", 1)).unwrap();
        assert_eq!(reloaded.load::<Vec<f32>>(&changed).unwrap(), None);
        fs::write(reloaded.path(&key).unwrap(), b"{partial").unwrap();
        assert_eq!(reloaded.load::<Vec<f32>>(&key).unwrap(), None);
    }
}
