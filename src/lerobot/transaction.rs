//! Recoverable replacement of a dataset's existing shards and feature metadata.
use super::writer::{Assignment, write_out_locked};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
};
const DIRECTORY: &str = ".cerul/writeback";
#[derive(Serialize, Deserialize)]
struct Entry {
    path: PathBuf,
    before: String,
    after: String,
}
#[derive(Serialize, Deserialize)]
struct Journal {
    terminal: bool,
    entries: Vec<Entry>,
}
fn hash(path: &Path) -> Result<String> {
    let mut input = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
fn safe_file(root: &Path, relative: &Path) -> Result<PathBuf> {
    ensure!(
        relative
            .components()
            .all(|c| matches!(c, Component::Normal(_))),
        "invalid writeback path"
    );
    ensure!(
        relative == Path::new("meta/info.json")
            || (relative.starts_with("data")
                && relative.extension().is_some_and(|e| e == "parquet")),
        "invalid writeback target"
    );
    let path = root.join(relative);
    ensure!(
        !fs::symlink_metadata(&path)?.file_type().is_symlink()
            && fs::canonicalize(&path)?.starts_with(root),
        "writeback target escapes dataset"
    );
    Ok(path)
}
fn copy_synced(source: &Path, target: &Path) -> Result<()> {
    fs::copy(source, target)?;
    File::open(target)?.sync_all()?;
    Ok(())
}
// Lock the dataset directory itself so separate workspaces coordinate without
// creating lock files in read-only source datasets. Supported M1 systems are POSIX.
pub(super) struct DatasetLock(File);
impl Drop for DatasetLock {
    fn drop(&mut self) {
        // Explicit unlock also releases descriptors transiently inherited by a
        // concurrently spawned media child before that child reaches exec.
        let _ = self.0.unlock();
    }
}
pub(super) fn read_lock(root: &Path) -> Result<DatasetLock> {
    let file = File::open(root)?;
    file.try_lock_shared()
        .context("dataset is being written by another process")?;
    let guard = DatasetLock(file);
    ensure_readable(root)?;
    Ok(guard)
}
fn lock(root: &Path) -> Result<DatasetLock> {
    let root = fs::canonicalize(root)?;
    let file = File::open(&root)?;
    file.try_lock()
        .context("dataset is being read or written by another process")?;
    let guard = DatasetLock(file);
    let sidecar = root.join(".cerul");
    fs::create_dir_all(&sidecar)?;
    ensure!(
        fs::canonicalize(&sidecar)?.starts_with(&root),
        "dataset sidecar escapes root"
    );
    Ok(guard)
}
fn recover_locked(root: &Path) -> Result<bool> {
    let dir = root.join(DIRECTORY);
    if !dir.exists() {
        return Ok(false);
    }
    ensure!(
        !fs::symlink_metadata(&dir)?.file_type().is_symlink(),
        "invalid transaction directory"
    );
    let manifest = dir.join("journal.json");
    if !manifest.exists() {
        // No replacements can start before the durable manifest exists.
        fs::remove_dir_all(&dir)?;
        return Ok(false);
    }
    let mut journal: Journal = serde_json::from_slice(&fs::read(&manifest)?)?;
    if !journal.terminal {
        // Check every file before restoring any; never overwrite a foreign edit.
        for (i, entry) in journal.entries.iter().enumerate() {
            let path = safe_file(root, &entry.path)?;
            let current = hash(&path)?;
            ensure!(
                current == entry.before || current == entry.after,
                "dataset changed outside writeback: {}",
                entry.path.display()
            );
            ensure!(
                hash(&dir.join(format!("{i}.old")))? == entry.before,
                "invalid writeback backup"
            );
        }
        for (i, entry) in journal.entries.iter().enumerate() {
            let path = safe_file(root, &entry.path)?;
            let temporary = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
            copy_synced(&dir.join(format!("{i}.old")), temporary.path())?;
            temporary.persist(&path).map_err(|e| e.error)?;
            File::open(path.parent().unwrap())?.sync_all()?;
        }
        journal.terminal = true;
        crate::storage::write_json(&manifest, &journal)?;
    }
    fs::remove_dir_all(&dir)?;
    File::open(root.join(".cerul"))?.sync_all()?;
    Ok(true)
}
/// Restore an interrupted transaction, or finish cleaning a committed one.
pub fn recover(root: &Path) -> Result<bool> {
    let root = fs::canonicalize(root)?;
    let _lock = lock(&root)?;
    recover_locked(&root)
}
/// Readers must not observe a partially replaced group of shared shards.
pub fn ensure_readable(root: &Path) -> Result<()> {
    ensure!(
        !root.join(DIRECTORY).join("journal.json").exists(),
        "dataset has an unfinished writeback; resume annotate --write-lerobot to recover it"
    );
    Ok(())
}
fn prepare(root: &Path, output: &Path, originals: &[(PathBuf, String)]) -> Result<Journal> {
    let dir = root.join(DIRECTORY);
    fs::create_dir(&dir)?;
    let mut entries = Vec::new();
    for (path, before) in originals {
        let original = safe_file(root, path)?;
        ensure!(
            hash(&original)? == *before,
            "dataset changed while writeback was staged"
        );
        let replacement = output.join(path);
        let after = hash(&replacement)?;
        if after == *before {
            continue;
        }
        let i = entries.len();
        copy_synced(&original, &dir.join(format!("{i}.old")))?;
        copy_synced(&replacement, &dir.join(format!("{i}.new")))?;
        entries.push(Entry {
            path: path.clone(),
            before: before.clone(),
            after,
        });
    }
    let journal = Journal {
        terminal: false,
        entries,
    };
    crate::storage::write_json(&dir.join("journal.json"), &journal)?;
    File::open(root.join(".cerul"))?.sync_all()?;
    Ok(journal)
}
fn commit(
    root: &Path,
    mut journal: Journal,
    mut after_replace: impl FnMut(usize) -> Result<()>,
) -> Result<()> {
    let dir = root.join(DIRECTORY);
    for (i, entry) in journal.entries.iter().enumerate() {
        let path = safe_file(root, &entry.path)?;
        ensure!(
            hash(&path)? == entry.before,
            "dataset changed during writeback"
        );
        fs::rename(dir.join(format!("{i}.new")), &path)?;
        File::open(path.parent().unwrap())?.sync_all()?;
        after_replace(i)?;
    }
    journal.terminal = true;
    crate::storage::write_json(&dir.join("journal.json"), &journal)?;
    recover_locked(root)?;
    Ok(())
}
/// Validate a complete staged dataset, then replace files with durable rollback copies.
pub fn write_in_place(
    root: &Path,
    assignments: &[Assignment<'_>],
    validate: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    let root = fs::canonicalize(root)?;
    let _lock = lock(&root)?;
    recover_locked(&root)?;
    let mut paths = super::files(&root.join("data"))?;
    paths.push(root.join("meta/info.json"));
    let originals = paths
        .iter()
        .map(|p| Ok((p.strip_prefix(&root)?.to_path_buf(), hash(p)?)))
        .collect::<Result<Vec<_>>>()?;
    let stage = tempfile::tempdir()?;
    let output = stage.path().join("dataset");
    write_out_locked(&root, &output, assignments, validate)?;
    let journal = prepare(&root, &output, &originals).context("preparing writeback")?;
    let result = commit(&root, journal, |_| Ok(()));
    if result.is_err() {
        recover_locked(&root).context("writeback failed and rollback needs attention")?;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dataset_readers_and_writers_coordinate_without_a_workspace() {
        let root = tempfile::tempdir().unwrap();
        let first = read_lock(root.path()).unwrap();
        let second = read_lock(root.path()).unwrap();
        assert!(!root.path().join(".cerul").exists());
        assert!(lock(root.path()).is_err());
        drop(first);
        assert!(lock(root.path()).is_err());
        drop(second);
        let writer = lock(root.path()).unwrap();
        assert!(read_lock(root.path()).is_err());
        assert!(lock(root.path()).is_err());
        drop(writer);
        assert!(read_lock(root.path()).is_ok());
    }
    #[test]
    fn interrupted_group_restores_all_files_and_foreign_edits_are_not_overwritten() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("dataset");
        fs::create_dir_all(root.join("data")).unwrap();
        fs::create_dir_all(root.join("meta")).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let output = temp.path().join("output");
        fs::create_dir_all(output.join("data")).unwrap();
        fs::create_dir_all(output.join("meta")).unwrap();
        let paths = ["data/one.parquet", "data/two.parquet", "meta/info.json"];
        for path in paths {
            fs::write(root.join(path), format!("original {path}")).unwrap();
            fs::write(output.join(path), format!("replacement {path}")).unwrap();
        }
        let originals: Vec<_> = paths
            .iter()
            .map(|p| (PathBuf::from(p), hash(&root.join(p)).unwrap()))
            .collect();
        {
            let _lock = lock(&root).unwrap();
            let journal = prepare(&root, &output, &originals).unwrap();
            assert!(
                commit(&root, journal, |_| anyhow::bail!(
                    "simulated process interruption"
                ))
                .is_err()
            );
        }
        assert!(ensure_readable(&root).is_err());
        assert!(
            fs::read_to_string(root.join(paths[0]))
                .unwrap()
                .starts_with("replacement")
        );
        assert!(recover(&root).unwrap());
        for (path, before) in &originals {
            assert_eq!(hash(&root.join(path)).unwrap(), *before);
        }
        assert!(ensure_readable(&root).is_ok());
        assert!(!recover(&root).unwrap());
        let journal = prepare(&root, &output, &originals).unwrap();
        assert!(commit(&root, journal, |_| anyhow::bail!("interrupted")).is_err());
        fs::write(root.join(paths[1]), "foreign edit").unwrap();
        assert!(recover(&root).is_err());
        assert_eq!(
            fs::read_to_string(root.join(paths[1])).unwrap(),
            "foreign edit"
        );
        assert!(
            fs::read_to_string(root.join(paths[0]))
                .unwrap()
                .starts_with("replacement")
        );
        fs::write(root.join(paths[1]), format!("original {}", paths[1])).unwrap();
        recover(&root).unwrap();
        let journal = prepare(&root, &output, &originals).unwrap();
        commit(&root, journal, |_| Ok(())).unwrap();
        for path in paths {
            assert_eq!(
                fs::read(root.join(path)).unwrap(),
                fs::read(output.join(path)).unwrap()
            );
        }
        assert!(!root.join(DIRECTORY).exists());
    }
}
