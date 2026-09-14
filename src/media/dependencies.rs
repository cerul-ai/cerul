//! Explicit CLI preflight and atomic repair of Cerul-owned media tools.
//! Compatible local tools never require a network request or a managed install.
use super::{Tools, check_cancellation, select_tools, with_sync_cancellation, with_sync_tools};
use crate::{
    events::{Event, EventSink},
    storage,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, BufReader, Read, Seek, Write},
    path::{Component, Path},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

const MAX_DOWNLOAD: u64 = 128 * 1024 * 1024;
const MAX_EXPANDED: u64 = 512 * 1024 * 1024;
const FILES: &[&str] = &[
    "cerul-ffmpeg",
    "cerul-ffprobe",
    "THIRD_PARTY_NOTICES.md",
    "licenses/FFmpeg-GPL-2.0.txt",
    "licenses/x264-GPL-2.0.txt",
    "licenses/zlib.txt",
];

#[derive(Clone)]
struct Distribution {
    archive: &'static str,
    sha256: String,
    url: String,
}
fn distribution() -> Result<Distribution> {
    let (archive, sha256) = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => (
            "cerul-aarch64-apple-darwin",
            "bd6366c21b781857ed03b914c6e712a40a7d3c838ffb71721acdf65a41537403",
        ),
        ("linux", "x86_64") => (
            "cerul-x86_64-unknown-linux-gnu",
            "36097eb8177b7cb030ac51dea46a71936cd0c35d8f882851bcded3016be79169",
        ),
        _ => anyhow::bail!(
            "automatic media repair supports macOS arm64 and Linux x86_64; configure compatible CERUL_FFMPEG and CERUL_FFPROBE executables"
        ),
    };
    Ok(Distribution {
        archive,
        sha256: sha256.into(),
        url: format!(
            "https://github.com/cerul-ai/cerul/releases/download/v0.0.10/{archive}.tar.xz"
        ),
    })
}

fn log(events: &mut dyn EventSink, msg: impl Into<String>) {
    events.emit(Event::Log {
        level: "info".into(),
        msg: msg.into(),
    });
}

/// Must run inside `media::with_cancellation`. Selection lasts only for that
/// operation; explicit overrides and system installations are never rewritten.
pub async fn prepare(
    workspace: &Path,
    automatic: bool,
    dry_run: bool,
    cancel: &CancellationToken,
    events: &mut dyn EventSink,
) -> Result<()> {
    let selected = Tools::discover(true);
    if dry_run {
        return with_sync_tools(Some(selected), super::check_dependencies);
    }
    let failure = match check_pair(&selected, cancel) {
        Ok(()) => return select_tools(selected),
        Err(error) => error,
    };
    check_cancellation()?;
    ensure!(
        automatic,
        "{failure:#}; automatic dependency repair is disabled"
    );
    // A stale custom override can often be repaired without downloading anything.
    let local = Tools::discover(false);
    if local != selected && check_pair(&local, cancel).is_ok() {
        log(events, "Using compatible local media tools");
        return select_tools(local);
    }
    check_cancellation()?;
    let spec = distribution()?;
    let root = workspace.join("runtime/media");
    if let Some(tools) = cached(&root, &spec, cancel)? {
        return select_tools(tools);
    }
    log(events, "Preparing compatible media tools before processing");
    let tools = install(&root, &spec, cancel, events).await
        .with_context(|| format!("media dependencies could not be prepared before processing; selected tools failed: {failure:#}"))?;
    select_tools(tools)
}

/// Check execution and the codecs/filter operations the CLI needs, not just a
/// version label. This catches builds without libx264/AAC before any model call.
fn check_pair(tools: &Tools, cancel: &CancellationToken) -> Result<()> {
    with_sync_tools(Some(tools.clone()), || {
        with_sync_cancellation(cancel.clone(), || {
            super::check_dependencies()?;
            let directory = tempfile::tempdir()?;
            let video = directory.path().join("preflight.mp4");
            super::run(
                super::command("ffmpeg")
                    .args([
                        "-v",
                        "error",
                        "-nostdin",
                        "-f",
                        "lavfi",
                        "-i",
                        "testsrc2=size=16x16:rate=2:duration=0.5",
                        "-f",
                        "lavfi",
                        "-i",
                        "anullsrc=r=16000:cl=mono",
                        "-t",
                        "0.5",
                        "-vf",
                        "scale=16:16,format=yuv420p",
                        "-c:v",
                        "libx264",
                        "-c:a",
                        "aac",
                    ])
                    .arg(&video),
            )?;
            let probe = super::probe(&video)?;
            ensure!(
                probe.width == 16 && probe.height == 16 && probe.has_audio,
                "media tools failed the codec preflight"
            );
            Ok(())
        })
    })
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Installed {
    directory: String,
    archive_sha256: String,
    source: String,
    files: BTreeMap<String, String>,
}
fn cached(root: &Path, spec: &Distribution, cancel: &CancellationToken) -> Result<Option<Tools>> {
    check_cancellation()?;
    let Ok(bytes) = fs::read(root.join("active.json")) else {
        return Ok(None);
    };
    let Ok(manifest) = serde_json::from_slice::<Installed>(&bytes) else {
        return Ok(None);
    };
    let mut components = Path::new(&manifest.directory).components();
    if !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
        || manifest.archive_sha256 != spec.sha256
        || manifest.files.len() != FILES.len()
    {
        return Ok(None);
    }
    let directory = root.join(&manifest.directory);
    for name in FILES {
        let path = directory.join(name);
        if !fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_file())
            || manifest.files.get(*name) != super::sha256(&path).ok().as_ref()
        {
            return Ok(None);
        }
    }
    let tools = Tools::in_directory(&directory);
    if check_pair(&tools, cancel).is_err() {
        check_cancellation()?;
        return Ok(None);
    }
    Ok(Some(tools))
}

async fn install(
    root: &Path,
    spec: &Distribution,
    cancel: &CancellationToken,
    events: &mut dyn EventSink,
) -> Result<Tools> {
    // Serialize first-use repairs, and recheck after waiting for another process.
    let started = Instant::now();
    let _lock = loop {
        check_cancellation()?;
        match storage::WorkspaceLock::acquire(root) {
            Ok(lock) => break lock,
            Err(error)
                if error.to_string().contains("already being written")
                    && started.elapsed() < Duration::from_secs(300) =>
            {
                tokio::select! { () = cancel.cancelled() => { check_cancellation()?; }, () = tokio::time::sleep(Duration::from_millis(100)) => {} }
            }
            Err(error) => return Err(error),
        }
    };
    if let Some(tools) = cached(root, spec, cancel)? {
        return Ok(tools);
    }
    let stage = tempfile::Builder::new()
        .prefix(".install-")
        .tempdir_in(root)?;
    let archive = stage.path().join("download.tar.xz");
    download(spec, &archive, cancel, events).await?;
    let destination = stage.path().join("tools");
    let (worker_archive, worker_destination, worker_spec, worker_cancel) = (
        archive.clone(),
        destination.clone(),
        spec.clone(),
        cancel.clone(),
    );
    log(events, "Verifying and unpacking media tools");
    let files = tokio::task::spawn_blocking(move || {
        with_sync_cancellation(worker_cancel.clone(), || {
            unpack(
                &worker_archive,
                &worker_destination,
                &worker_spec,
                &worker_cancel,
            )
        })
    })
    .await??;
    check_cancellation()?;
    let tools = Tools::in_directory(&destination);
    check_pair(&tools, cancel)?;
    check_cancellation()?;
    let generation = format!("{}-{}", &spec.sha256[..16], uuid::Uuid::new_v4());
    fs::rename(&destination, root.join(&generation))?;
    // No pointer changes until both tools and their codecs have been verified.
    storage::write_json(
        &root.join("active.json"),
        &Installed {
            directory: generation.clone(),
            archive_sha256: spec.sha256.clone(),
            source: spec.url.clone(),
            files,
        },
    )?;
    log(events, "Media tools ready; continuing the original command");
    Ok(Tools::in_directory(&root.join(generation)))
}

async fn download(
    spec: &Distribution,
    path: &Path,
    cancel: &CancellationToken,
    events: &mut dyn EventSink,
) -> Result<()> {
    let client = reqwest::Client::builder()
        .https_only(!cfg!(test))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .user_agent(concat!("cerul/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let mut response = tokio::select! {
        () = cancel.cancelled() => { check_cancellation()?; anyhow::bail!("dependency download cancelled") },
        response = client.get(&spec.url).send() => response?,
    }.error_for_status()?;
    let total = response.content_length();
    ensure!(
        total.is_none_or(|size| size <= MAX_DOWNLOAD),
        "media download exceeds its size limit"
    );
    let mut file = File::create(path)?;
    let mut bytes = 0u64;
    let mut last_percent = None;
    loop {
        let chunk = tokio::select! {
            () = cancel.cancelled() => { check_cancellation()?; anyhow::bail!("dependency download cancelled") },
            chunk = response.chunk() => chunk?,
        };
        let Some(chunk) = chunk else { break };
        bytes += chunk.len() as u64;
        ensure!(
            bytes <= MAX_DOWNLOAD,
            "media download exceeds its size limit"
        );
        file.write_all(&chunk)?;
        if let Some(total) = total.filter(|n| *n > 0) {
            let percent = (bytes * 100 / total).min(100) / 10 * 10;
            if last_percent != Some(percent) {
                log(events, format!("Downloading media tools · {percent}%"));
                last_percent = Some(percent);
            }
        }
    }
    file.sync_all()?;
    Ok(())
}

struct BoundedWriter<'a> {
    file: &'a mut File,
    size: u64,
    cancel: &'a CancellationToken,
}
impl Write for BoundedWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.cancel.is_cancelled() {
            return Err(io::Error::other("dependency extraction cancelled"));
        }
        if self.size + bytes.len() as u64 > MAX_EXPANDED {
            return Err(io::Error::other(
                "expanded media archive exceeds its size limit",
            ));
        }
        let written = self.file.write(bytes)?;
        self.size += written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}
fn unpack(
    archive: &Path,
    directory: &Path,
    spec: &Distribution,
    cancel: &CancellationToken,
) -> Result<BTreeMap<String, String>> {
    ensure!(
        super::sha256(archive)? == spec.sha256,
        "media archive SHA-256 mismatch"
    );
    let mut expanded = tempfile::tempfile()?;
    lzma_rs::xz_decompress(
        &mut BufReader::new(File::open(archive)?),
        &mut BoundedWriter {
            file: &mut expanded,
            size: 0,
            cancel,
        },
    )
    .context("unpack media XZ archive")?;
    expanded.rewind()?;
    extract_tar(expanded, directory, spec.archive)
}
fn extract_tar(
    reader: impl Read,
    directory: &Path,
    prefix: &str,
) -> Result<BTreeMap<String, String>> {
    fs::create_dir_all(directory.join("licenses"))?;
    let mut files = BTreeMap::new();
    for entry in tar::Archive::new(reader).entries()? {
        check_cancellation()?;
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        ensure!(
            path.components()
                .all(|p| matches!(p, Component::Normal(_) | Component::CurDir)),
            "unsafe path in media archive"
        );
        let Ok(relative) = path.strip_prefix(prefix) else {
            continue;
        };
        let Some(name) = relative.to_str().filter(|name| FILES.contains(name)) else {
            continue;
        };
        ensure!(
            entry.header().entry_type().is_file() && !files.contains_key(name),
            "invalid media archive entry {name}"
        );
        let target = directory.join(name);
        let mut file = File::create(&target)?;
        io::copy(&mut entry, &mut file)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(if name.starts_with("cerul-") {
                0o755
            } else {
                0o644
            }))?;
        }
        file.sync_all()?;
        files.insert(name.to_owned(), super::sha256(&target)?);
    }
    ensure!(
        FILES.iter().all(|name| files.contains_key(*name)),
        "media archive is missing required tools or licenses"
    );
    File::open(directory.join("licenses"))?.sync_all()?;
    File::open(directory)?.sync_all()?;
    Ok(files)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn tar_fixture(extra: Option<tar::EntryType>, omit: bool) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for name in FILES.iter().take(if omit { 1 } else { FILES.len() }) {
            let bytes: &[u8] = match *name {
                "cerul-ffmpeg" => b"#!/bin/sh\nprintf 'ffmpeg version 7.1.4\\n'\n",
                "cerul-ffprobe" => br##"#!/bin/sh
if [ "$1" = '-version' ]; then
  printf 'ffprobe version 7.1.4\n'
else
  printf '%s\n' '{"streams":[{"codec_type":"video","width":16,"height":16,"duration":"0.5"},{"codec_type":"audio"}]}'
fi
"##,
                _ => b"Fixture license\n",
            };
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, format!("fixture/{name}"), bytes)
                .unwrap();
        }
        if let Some(kind) = extra {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(kind);
            header.set_size(0);
            header.set_mode(0o755);
            if kind.is_symlink() {
                header.set_link_name("/tmp/untrusted").unwrap();
            }
            header.set_cksum();
            builder
                .append_data(&mut header, "fixture/cerul-ffmpeg", io::empty())
                .unwrap();
        }
        // A full CLI bundle must never install the CLI itself during media repair.
        let mut header = tar::Header::new_gnu();
        header.set_size(3);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, "fixture/cerul", &b"CLI"[..])
            .unwrap();
        builder.into_inner().unwrap()
    }
    fn fixture() -> (Vec<u8>, Distribution) {
        let mut bytes = Vec::new();
        lzma_rs::xz_compress(&mut io::Cursor::new(tar_fixture(None, false)), &mut bytes).unwrap();
        let sha256 = crate::storage::hex(Sha256::digest(&bytes));
        (
            bytes,
            Distribution {
                archive: "fixture",
                sha256,
                url: String::new(),
            },
        )
    }
    async fn serve(bytes: Vec<u8>, stall: bool) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            let mut received = 0;
            while received < request.len() {
                let count = stream.read(&mut request[received..]).await.unwrap();
                received += count;
                if count == 0
                    || request[..received]
                        .windows(4)
                        .any(|part| part == b"\r\n\r\n")
                {
                    break;
                }
            }
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        bytes.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            if stall {
                std::future::pending::<()>().await;
            }
            stream.write_all(&bytes).await.unwrap();
        });
        (format!("http://{address}/bundle.tar.xz"), handle)
    }

    #[test]
    fn extraction_only_publishes_allowlisted_regular_files() {
        let dir = tempfile::tempdir().unwrap();
        let files = extract_tar(
            io::Cursor::new(tar_fixture(None, false)),
            dir.path(),
            "fixture",
        )
        .unwrap();
        assert_eq!(files.len(), FILES.len());
        assert!(!dir.path().join("cerul").exists());
        for (extra, omit) in [
            (Some(tar::EntryType::Symlink), false),
            (Some(tar::EntryType::Regular), false),
            (None, true),
        ] {
            let dir = tempfile::tempdir().unwrap();
            assert!(
                extract_tar(
                    io::Cursor::new(tar_fixture(extra, omit)),
                    dir.path(),
                    "fixture"
                )
                .is_err()
            );
        }
    }

    #[test]
    fn checksum_mismatch_fails_before_extraction() {
        let dir = tempfile::tempdir().unwrap();
        let (mut bytes, spec) = fixture();
        bytes[0] ^= 1;
        let archive = dir.path().join("bundle.xz");
        fs::write(&archive, bytes).unwrap();
        let destination = dir.path().join("tools");
        let error = unpack(&archive, &destination, &spec, &CancellationToken::new()).unwrap_err();
        assert!(error.to_string().contains("SHA-256 mismatch"));
        assert!(!destination.exists());
    }

    #[tokio::test]
    async fn concurrent_installs_publish_once_and_reuse_offline_with_integrity_checks() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("runtime/media");
        let (bytes, mut spec) = fixture();
        let (url, server) = serve(bytes, false).await;
        spec.url = url;
        let cancel = CancellationToken::new();
        super::super::with_cancellation(cancel.clone(), async {
            let (mut left_events, mut right_events) = (|_: Event| {}, |_: Event| {});
            let (left, right) = tokio::join!(
                install(&root, &spec, &cancel, &mut left_events),
                install(&root, &spec, &cancel, &mut right_events)
            );
            let tools = left.unwrap();
            assert_eq!(tools, right.unwrap());
            server.await.unwrap();
            // Server is gone: reuse must not perform another download.
            assert_eq!(
                install(&root, &spec, &cancel, &mut left_events)
                    .await
                    .unwrap(),
                tools
            );
            let active = fs::read(root.join("active.json")).unwrap();
            fs::write(&tools.ffmpeg, b"corrupted").unwrap();
            assert!(cached(&root, &spec, &cancel).unwrap().is_none());
            assert!(
                install(&root, &spec, &cancel, &mut left_events)
                    .await
                    .is_err()
            );
            assert_eq!(active, fs::read(root.join("active.json")).unwrap());
            assert!(!tools.ffmpeg.parent().unwrap().join("cerul").exists());
        })
        .await;
    }

    #[tokio::test]
    async fn cancelled_download_never_publishes_or_leaves_staging_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("media");
        let (bytes, mut spec) = fixture();
        let (url, server) = serve(bytes, true).await;
        spec.url = url;
        let cancel = CancellationToken::new();
        super::super::with_cancellation(cancel.clone(), async {
            let trigger = async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                cancel.cancel();
            };
            let mut events = |_: Event| {};
            let (result, ()) = tokio::join!(install(&root, &spec, &cancel, &mut events), trigger);
            assert!(result.is_err());
            assert!(!root.join("active.json").exists());
            assert!(fs::read_dir(&root).unwrap().all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".install-")
            }));
        })
        .await;
        server.abort();
    }

    #[tokio::test]
    async fn selected_tools_survive_nested_scopes_and_cpu_workers() {
        let cancel = CancellationToken::new();
        let tools = Tools::in_directory(Path::new("/managed/tools"));
        super::super::with_cancellation(cancel.clone(), async {
            select_tools(tools.clone()).unwrap();
            super::super::with_cancellation(cancel, async {
                assert_eq!(super::super::current_tools(), Some(tools.clone()));
            })
            .await;
            let captured = super::super::current_tools();
            let actual = tokio::task::spawn_blocking(move || {
                with_sync_tools(captured, || {
                    super::super::command("ffmpeg").get_program().to_owned()
                })
            })
            .await
            .unwrap();
            assert_eq!(actual, tools.ffmpeg.as_os_str());
        })
        .await;
        assert!(super::super::current_tools().is_none());
    }
}
