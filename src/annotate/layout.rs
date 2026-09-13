//! Versioned semantic layout with legacy reads and recoverable file publication.
use crate::{annotations::AnnotationFile, storage};
use anyhow::Result;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub fn annotation(directory: &Path, name: &str) -> PathBuf {
    let current = directory
        .join(".internal/annotations")
        .join(format!("{name}.jsonl"));
    if current.is_file() || !directory.join(format!("{name}.jsonl")).is_file() {
        current
    } else {
        directory.join(format!("{name}.jsonl"))
    }
}

pub fn recovery(directory: &Path, name: &str) -> PathBuf {
    let current = directory
        .join(".internal/recovery")
        .join(format!("{name}.json"));
    if current.is_file() || !directory.join(format!("{name}.conflicts.json")).is_file() {
        current
    } else {
        directory.join(format!("{name}.conflicts.json"))
    }
}

/// Remove an old file only after its complete replacement is durably published.
/// On interruption readers prefer the new complete file; unrelated files stay put.
pub fn publish(
    directory: &Path,
    product: &super::semantic::Product,
    coverage: crate::episode::TimeRange,
    ontology: Option<&std::collections::BTreeSet<String>>,
) -> Result<()> {
    let name = &product.annotation.header.name;
    let recovery = directory
        .join(".internal/recovery")
        .join(format!("{name}.json"));
    storage::write_json(&recovery, product)?;
    let path = directory
        .join(".internal/annotations")
        .join(format!("{name}.jsonl"));
    product
        .annotation
        .publish_in_range(&path, coverage, ontology)?;
    for old in [
        directory.join(format!("{name}.jsonl")),
        directory.join(format!("{name}.conflicts.json")),
    ] {
        match fs::remove_file(&old) {
            Ok(()) => {
                fs::File::open(directory)?.sync_all()?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

/// Inventory includes index-produced and legacy annotations, preferring the
/// current semantic layout if both copies survived an interrupted migration.
pub fn files(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut files = BTreeMap::new();
    for root in [
        directory.to_owned(),
        directory.join(".internal/annotations"),
    ] {
        let entries = match fs::read_dir(root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_file()
                && path.extension().is_some_and(|ext| ext == "jsonl")
                && entry.file_name() != "log.jsonl"
            {
                files.insert(entry.file_name(), path);
            }
        }
    }
    Ok(files.into_values().collect())
}

pub fn publish_flags(
    directory: &Path,
    file: &AnnotationFile,
    coverage: crate::episode::TimeRange,
) -> Result<()> {
    // Conflict projection has its own input hash and does not replace the raw
    // model recovery product used to reconstruct it.
    file.publish_in_range(
        &directory.join(".internal/annotations/semantic.flag.jsonl"),
        coverage,
        None,
    )?;
    let legacy = directory.join("semantic.flag.jsonl");
    if legacy.is_file() {
        fs::remove_file(legacy)?;
        fs::File::open(directory)?.sync_all()?;
    }
    Ok(())
}
