//! Explicit cleanup of disposable caches and opt-in deletion of registered sidecars.
use crate::{
    episode::Episode,
    index::{
        discover::{RegistryEntry, read_registry_all as read_registry, register},
        lance::literal,
    },
    storage,
};
use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Default)]
pub struct Options {
    pub index: Option<String>,
    pub all_indexes: bool,
    pub cache: bool,
    pub compact: bool,
    pub sidecars: Option<PathBuf>,
    pub yes: bool,
    pub dry_run: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    RemoveIndex,
    RemoveCache,
    RemoveSidecar,
    Compact,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Item {
    pub action: Action,
    pub path: PathBuf,
    pub episode: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Report {
    pub dry_run: bool,
    pub items: Vec<Item>,
}
fn space_id(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn no_symlinks(path: &Path) -> Result<()> {
    for component in path.ancestors() {
        match fs::symlink_metadata(component) {
            Ok(meta) => ensure!(
                !meta.file_type().is_symlink(),
                "cleanup refuses symlink {}",
                component.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
fn indexes(workspace: &Path) -> Result<Vec<PathBuf>> {
    let root = workspace.join("index");
    no_symlinks(&root)?;
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry?;
        if space_id(&entry.file_name().to_string_lossy()) {
            no_symlinks(&entry.path())?;
            ensure!(
                entry.file_type()?.is_dir(),
                "index space must be a directory"
            );
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}
fn protect(path: &Path, entries: &[RegistryEntry], sidecar: bool) -> Result<()> {
    let path = match fs::canonicalize(path) {
        Ok(path) => path,
        Err(error) if sidecar && error.kind() == std::io::ErrorKind::NotFound => {
            missing_leaf(path)?
        }
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        ensure!(
            !entry.media.starts_with(&path),
            "cleanup target contains registered source media"
        );
        if !sidecar {
            ensure!(
                !entry.sidecar.starts_with(&path),
                "cache target contains authoritative sidecars"
            );
        }
    }
    Ok(())
}
fn missing_leaf(path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Ok(fs::canonicalize(parent)?.join(
        path.file_name()
            .context("cleanup target needs a filename")?,
    ))
}
pub fn plan(workspace: &Path, options: &Options) -> Result<Report> {
    let root = if workspace.exists() {
        fs::canonicalize(workspace)?
    } else {
        workspace.to_owned()
    };
    let workspace = root.as_path();
    ensure!(
        options.index.is_some()
            || options.all_indexes
            || options.cache
            || options.compact
            || options.sidecars.is_some(),
        "select an index, cache, compaction, or sidecar target"
    );
    ensure!(
        !(options.index.is_some() && options.all_indexes),
        "--index and --all-indexes conflict"
    );
    ensure!(
        !(options.compact && (options.index.is_some() || options.all_indexes)),
        "cannot compact and delete indexes together"
    );
    ensure!(
        options.sidecars.is_none() || options.yes || options.dry_run,
        "sidecar deletion requires --yes"
    );
    if let Some(space) = &options.index {
        ensure!(space_id(space), "invalid embedding space ID");
    }
    let entries = read_registry(workspace)?;
    let mut items = Vec::new();
    for path in indexes(workspace)? {
        if options.all_indexes
            || options
                .index
                .as_ref()
                .is_some_and(|id| path.file_name().is_some_and(|name| name == id.as_str()))
        {
            protect(&path, &entries, false)?;
            items.push(Item {
                action: Action::RemoveIndex,
                path,
                episode: None,
            });
        } else if options.compact {
            items.push(Item {
                action: Action::Compact,
                path,
                episode: None,
            });
        }
    }
    let cache = workspace.join("cache");
    if options.cache && cache.exists() {
        no_symlinks(&cache)?;
        protect(&cache, &entries, false)?;
        items.push(Item {
            action: Action::RemoveCache,
            path: cache,
            episode: None,
        });
    }
    if let Some(selection) = &options.sidecars {
        let selection = match fs::canonicalize(selection) {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let path = missing_leaf(selection)?;
                ensure!(
                    entries
                        .iter()
                        .any(|e| e.pending_deletion && e.sidecar == path),
                    "sidecar selection does not exist"
                );
                path
            }
            Err(error) => return Err(error.into()),
        };
        for entry in &entries {
            if !entry.media.starts_with(&selection) && !entry.sidecar.starts_with(&selection) {
                continue;
            }
            no_symlinks(&entry.sidecar)?;
            if !entry.pending_deletion {
                let episode: Episode =
                    serde_json::from_slice(&fs::read(entry.sidecar.join("episode.json"))?)?;
                episode.validate()?;
                ensure!(
                    episode.episode_id == entry.episode_id,
                    "sidecar identity mismatch"
                );
            }
            protect(&entry.sidecar, &entries, true)?;
            items.push(Item {
                action: Action::RemoveSidecar,
                path: entry.sidecar.clone(),
                episode: Some(entry.episode_id.clone()),
            });
        }
        ensure!(
            items
                .iter()
                .any(|item| matches!(item.action, Action::RemoveSidecar)),
            "no registered sidecars match selection"
        );
    }
    Ok(Report {
        dry_run: options.dry_run,
        items,
    })
}
async fn connection(path: &Path) -> Result<lancedb::Connection> {
    Ok(
        lancedb::connect(path.to_str().context("index path must be UTF-8")?)
            .execute()
            .await?,
    )
}
pub async fn run(workspace: &Path, options: &Options) -> Result<Report> {
    run_with_cancellation(
        workspace,
        options,
        tokio_util::sync::CancellationToken::new(),
    )
    .await
}

/// Stop scheduling cleanup work when cancelled, including pending index compaction.
pub async fn run_with_cancellation(
    workspace: &Path,
    options: &Options,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<Report> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(crate::providers::ProviderError {
            kind: crate::providers::Failure::Cancelled,
            message: "operation cancelled".into(),
        }.into()),
        result = run_inner(workspace, options, &cancel) => result,
    }
}

async fn run_inner(
    workspace: &Path,
    options: &Options,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<Report> {
    if options.dry_run {
        return plan(workspace, options);
    }
    // Validate before acquiring a lock, so invalid arguments do not create a workspace.
    plan(workspace, options)?;
    let _lock = storage::WorkspaceLock::acquire(workspace)?;
    let root = fs::canonicalize(workspace)?;
    let workspace = root.as_path();
    let report = plan(workspace, options)?;
    for item in &report.items {
        tokio::task::yield_now().await;
        if cancel.is_cancelled() {
            return Err(crate::providers::ProviderError {
                kind: crate::providers::Failure::Cancelled,
                message: "operation cancelled".into(),
            }
            .into());
        }
        no_symlinks(&item.path)?;
        match item.action {
            Action::RemoveIndex | Action::RemoveCache => fs::remove_dir_all(&item.path)?,
            Action::Compact => {
                let db = connection(&item.path).await?;
                for table in db.table_names().execute().await? {
                    tokio::task::yield_now().await;
                    db.open_table(&table)
                        .execute()
                        .await?
                        .optimize(lancedb::table::OptimizeAction::Compact {
                            options: Default::default(),
                            remap_options: None,
                        })
                        .await?;
                }
            }
            Action::RemoveSidecar => {
                let episode = item.episode.as_ref().unwrap();
                let mut entry = read_registry(workspace)?
                    .into_iter()
                    .find(|e| e.episode_id == *episode)
                    .context("sidecar is no longer registered")?;
                if !entry.pending_deletion {
                    entry.pending_deletion = true;
                    register(workspace, entry)?;
                }
                // Invalidate every projection before deleting its source. A
                // failure leaves the authoritative sidecar available to rebuild.
                for path in indexes(workspace)? {
                    let db = connection(&path).await?;
                    for table in db.table_names().execute().await? {
                        if matches!(table.as_str(), "chunks" | "records") {
                            db.open_table(&table)
                                .execute()
                                .await?
                                .delete(&format!("episode = {}", literal(episode)))
                                .await?;
                        }
                    }
                }
                match fs::remove_dir_all(&item.path) {
                    Ok(()) => fs::File::open(item.path.parent().context("sidecar has no parent")?)?
                        .sync_all()?,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
                let mut bytes = Vec::new();
                for entry in read_registry(workspace)?
                    .into_iter()
                    .filter(|entry| entry.episode_id != *episode)
                {
                    serde_json::to_writer(&mut bytes, &entry)?;
                    bytes.push(b'\n');
                }
                storage::atomic_write(&workspace.join("registry.jsonl"), &bytes)?;
            }
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancellation_prevents_cleanup_before_start_and_between_actions() {
        for cancelled_before_start in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let workspace = dir.path().join("workspace");
            let index = workspace.join("index").join("a".repeat(64));
            fs::create_dir_all(&index).unwrap();
            fs::write(index.join("sentinel"), "preserve").unwrap();
            let cancel = tokio_util::sync::CancellationToken::new();
            if cancelled_before_start {
                cancel.cancel();
            }
            let options = Options {
                all_indexes: true,
                ..Default::default()
            };
            let (result, ()) = tokio::join!(
                run_with_cancellation(&workspace, &options, cancel.clone()),
                async {
                    cancel.cancel();
                },
            );
            assert_eq!(
                result
                    .unwrap_err()
                    .downcast_ref::<crate::providers::ProviderError>()
                    .unwrap()
                    .kind,
                crate::providers::Failure::Cancelled
            );
            assert_eq!(
                fs::read_to_string(index.join("sentinel")).unwrap(),
                "preserve"
            );
            if cancelled_before_start {
                assert!(!workspace.join("runtime").exists());
            }
        }
    }

    #[tokio::test]
    async fn interrupted_sidecar_removal_resumes_for_partial_and_missing_directories() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let source = root.join("source.mp4");
        fs::write(&source, b"source remains untouched").unwrap();
        for partial in [false, true] {
            let workspace = root.join(format!("workspace-{partial}"));
            let sidecar = root.join(format!("sidecar-{partial}"));
            if partial {
                fs::create_dir_all(sidecar.join("embeddings")).unwrap();
                fs::write(sidecar.join("embeddings/vector.parquet"), b"remaining file").unwrap();
                // episode.json was already removed before interruption.
            }
            register(
                &workspace,
                RegistryEntry {
                    episode_id: "dataset/0".into(),
                    sha256: "source-hash".into(),
                    media: source.clone(),
                    sidecar: sidecar.clone(),
                    pending_deletion: true,
                },
            )
            .unwrap();
            let mut options = Options {
                sidecars: Some(sidecar.clone()),
                yes: true,
                dry_run: true,
                ..Default::default()
            };
            assert_eq!(run(&workspace, &options).await.unwrap().items.len(), 1);
            assert_eq!(read_registry(&workspace).unwrap().len(), 1);
            assert_eq!(sidecar.exists(), partial);
            options.dry_run = false;
            run(&workspace, &options).await.unwrap();
            assert!(!sidecar.exists());
            assert!(read_registry(&workspace).unwrap().is_empty());
            assert_eq!(fs::read(&source).unwrap(), b"source remains untouched");
        }
    }
    #[tokio::test]
    async fn compact_preserves_rows_and_explicit_sidecar_delete_removes_projection() {
        use crate::index::{
            discover,
            lance::VectorIndex,
            vectors::{Kind, VectorRow},
        };
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("workspace");
        let source = dir.path().join("source.mp4");
        crate::media::run(
            crate::media::command("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=64x64:rate=2:duration=2",
                    "-c:v",
                    "libx264",
                ])
                .arg(&source),
        )
        .unwrap();
        let episode = discover::ordinary_episode(&source).unwrap();
        let sidecar = discover::publish_episode(&workspace, &episode, None).unwrap();
        let space = "b".repeat(64);
        let row = VectorRow {
            id: "unit".into(),
            episode: episode.episode_id.clone(),
            stream: "primary".into(),
            kind: Kind::Video,
            start_us: 0,
            end_us: 2_000_000,
            vector: vec![0.5, 0.5],
            text: String::new(),
            still: false,
            space_id: space.clone(),
            params_hash: "test".into(),
        };
        let index = VectorIndex::open(&workspace, &space, 2, true)
            .await
            .unwrap();
        index
            .replace(&episode.episode_id, "primary", &[row])
            .await
            .unwrap();
        drop(index);
        run(
            &workspace,
            &Options {
                compact: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let index = VectorIndex::open(&workspace, &space, 2, false)
            .await
            .unwrap();
        assert_eq!(index.count().await.unwrap(), 1);
        drop(index);
        let options = Options {
            sidecars: Some(source.clone()),
            yes: true,
            ..Default::default()
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&sidecar, fs::Permissions::from_mode(0o555)).unwrap();
            let failed = run(&workspace, &options).await;
            fs::set_permissions(&sidecar, fs::Permissions::from_mode(0o755)).unwrap();
            assert!(failed.is_err());
            assert!(read_registry(&workspace).unwrap()[0].pending_deletion);
            assert!(source.exists());
        }
        run(&workspace, &options).await.unwrap();
        assert!(source.exists());
        assert!(!sidecar.exists());
        assert!(read_registry(&workspace).unwrap().is_empty());
        assert_eq!(
            VectorIndex::open(&workspace, &space, 2, false)
                .await
                .unwrap()
                .count()
                .await
                .unwrap(),
            0
        );
    }
    #[tokio::test]
    async fn cache_cleanup_preserves_sidecars_and_dry_run_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        let index = workspace.join("index").join("a".repeat(64));
        fs::create_dir_all(&index).unwrap();
        fs::create_dir_all(workspace.join("cache")).unwrap();
        fs::create_dir_all(workspace.join("sidecars/example/embeddings")).unwrap();
        fs::write(
            workspace.join("sidecars/example/embeddings/source.parquet"),
            b"authoritative",
        )
        .unwrap();
        let mut options = Options {
            all_indexes: true,
            cache: true,
            dry_run: true,
            ..Default::default()
        };
        assert_eq!(run(workspace, &options).await.unwrap().items.len(), 2);
        assert!(index.exists());
        assert!(!workspace.join("runtime").exists());
        options.dry_run = false;
        run(workspace, &options).await.unwrap();
        assert!(!index.exists());
        assert!(!workspace.join("cache").exists());
        assert_eq!(
            fs::read(workspace.join("sidecars/example/embeddings/source.parquet")).unwrap(),
            b"authoritative"
        );
        assert!(
            plan(
                workspace,
                &Options {
                    sidecars: Some(workspace.into()),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
    #[cfg(unix)]
    #[test]
    fn refuses_symlink_cache_and_path_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("cache")).unwrap();
        assert!(
            plan(
                dir.path(),
                &Options {
                    cache: true,
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            plan(
                dir.path(),
                &Options {
                    index: Some("../sidecars".into()),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}
