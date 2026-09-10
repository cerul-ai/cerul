//! Finding and installing the newest published release.
//!
//! Nothing here runs on its own: the command asks GitHub what the newest
//! release is, says what it found, and installs only when a person or an
//! explicit `--yes` says so. The script it runs is the one that release
//! published, so what gets installed is the version that was reported.
use anyhow::{Context, Result, ensure};
use cerul::providers::{Failure, ProviderError};
use serde::Deserialize;
use std::{io::Write, path::PathBuf};
use tokio_util::sync::CancellationToken;

/// Where releases are published. Named as a constant so the command can print
/// exactly where it will look before it looks.
const LATEST: &str = "https://api.github.com/repos/cerul-ai/cerul/releases/latest";
/// The installer each release publishes, and the one the documented install
/// command fetches.
const INSTALLER: &str = "cerul-installer.sh";
/// Executables the bundle installs together. Keeping them together is what
/// makes the media tools match the CLI that calls them.
pub const INSTALLED: [&str; 3] = ["cerul", "cerul-ffmpeg", "cerul-ffprobe"];
/// Tells the installer to write the bundle straight into one directory and to
/// leave shell profiles alone: the directory a running Cerul came from is
/// already on PATH, and it is the one that has to be replaced.
const INSTALL_DIR: &str = "CERUL_UNMANAGED_INSTALL";

fn cancelled() -> anyhow::Error {
    ProviderError {
        kind: Failure::Cancelled,
        message: "operation cancelled".into(),
    }
    .into()
}

/// Awaits a request, or gives up the moment the run is cancelled. Without this
/// a Ctrl+C during a download is only noticed once the download finishes.
async fn interruptible<T>(
    cancel: &CancellationToken,
    work: impl std::future::Future<Output = reqwest::Result<T>>,
) -> Result<T> {
    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(cancelled()),
        result = work => Ok(result?),
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}
#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// What the newest release is, and how this build compares to it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Available {
    pub current: String,
    pub latest: String,
    /// True when the published release is newer than the running build.
    pub newer: bool,
    /// The exact script that would run, from the release being installed.
    pub installer: String,
}

/// Released versions compared number by number, so 0.0.10 is newer than 0.0.9
/// where a string comparison would say the opposite. A suffix is ignored rather
/// than guessed at; releases carry none.
fn ordinal(value: &str) -> Vec<u64> {
    value
        .trim()
        .trim_start_matches('v')
        .split('.')
        .map(|part| {
            part.chars()
                .take_while(|character| character.is_ascii_digit())
                .collect::<String>()
                .parse()
                .unwrap_or(0)
        })
        .collect()
}

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(concat!("cerul/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("could not start an HTTPS client")
}

/// Asks what the newest release is. Reads only; installs nothing.
pub async fn check(cancel: &CancellationToken) -> Result<Available> {
    let response = interruptible(cancel, client()?.get(LATEST).send())
        .await
        .context("could not reach GitHub Releases")?;
    ensure!(
        response.status().is_success(),
        "GitHub Releases answered HTTP {}",
        response.status().as_u16()
    );
    let release: Release = interruptible(cancel, response.json())
        .await
        .context("GitHub Releases returned something this version cannot read")?;
    let latest = release.tag_name.trim_start_matches('v').to_owned();
    let installer = release
        .assets
        .into_iter()
        .find(|asset| asset.name == INSTALLER)
        .map(|asset| asset.browser_download_url)
        .with_context(|| format!("release {latest} publishes no {INSTALLER}"))?;
    let current = env!("CARGO_PKG_VERSION").to_owned();
    Ok(Available {
        newer: ordinal(&latest) > ordinal(&current),
        current,
        latest,
        installer,
    })
}

/// Where the running Cerul lives, so an upgrade replaces it rather than adding a
/// copy somewhere else on PATH. `None` when the path cannot be resolved, which
/// leaves the installer to its own default and leaves the result unverifiable.
pub fn installed_at() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.canonicalize().ok())
}

/// Runs the installer that release published, into the directory the running
/// program came from. Its output is captured rather than printed: this process
/// owns its own streams, and a machine reading them is entitled to find only
/// what Cerul wrote there.
pub async fn install(available: &Available, cancel: &CancellationToken) -> Result<()> {
    let response = interruptible(cancel, client()?.get(&available.installer).send())
        .await
        .context("could not download the installer")?;
    ensure!(
        response.status().is_success(),
        "downloading the installer answered HTTP {}",
        response.status().as_u16()
    );
    let script = interruptible(cancel, response.text())
        .await
        .context("the installer did not read")?;
    // A redirect that lands on an error page is a page, not a script. Running it
    // would be running whatever the page happens to contain.
    ensure!(
        script.starts_with("#!"),
        "what was downloaded is not a shell script; install manually from {}",
        available.installer
    );
    // The last moment before anything is replaced. Someone who pressed Ctrl+C
    // while this was downloading has already said no.
    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    let executable = installed_at();
    let directory = executable.as_ref().and_then(|path| path.parent());
    run(&script, directory)?;
    // The installer can only report where it wrote. Whether the command a shell
    // will run next is the new one is a different question, and the only one
    // worth answering, so it is asked of the file itself.
    if let Some(path) = &executable {
        let reported = std::process::Command::new(path)
            .arg("--version")
            .output()
            .with_context(|| format!("could not run {} after installing", path.display()))?;
        let reported = String::from_utf8_lossy(&reported.stdout);
        let reported = reported.split_whitespace().nth(1).unwrap_or_default();
        ensure!(
            reported == available.latest,
            "the installer finished, but {} still answers {}; install manually from {}",
            path.display(),
            match reported.is_empty() {
                true => "nothing",
                false => reported,
            },
            available.installer
        );
    }
    Ok(())
}

/// Runs one installer script with its output captured, so a failure becomes an
/// error of Cerul's own rather than text on streams that belong to a result.
fn run(script: &str, directory: Option<&std::path::Path>) -> Result<()> {
    let mut file = tempfile::Builder::new()
        .prefix("cerul-installer-")
        .suffix(".sh")
        .tempfile()
        .context("could not write the installer to a temporary file")?;
    file.write_all(script.as_bytes())?;
    file.flush()?;
    let mut command = std::process::Command::new("sh");
    command.arg(file.path());
    if let Some(directory) = directory {
        command.env(INSTALL_DIR, directory);
    }
    let output = command.output().context("could not run the installer")?;
    if output.status.success() {
        return Ok(());
    }
    // What the installer said about its own failure is the useful part, and it
    // was captured, so it has to be carried into the error rather than lost.
    let said = [&output.stderr, &output.stdout]
        .iter()
        .map(|stream| String::from_utf8_lossy(stream).trim().to_owned())
        .find(|text| !text.is_empty())
        .unwrap_or_default();
    let ending = output
        .status
        .code()
        .map(|code| format!("exit {code}"))
        .unwrap_or_else(|| "a signal".into());
    let tail: Vec<&str> = said.lines().rev().take(3).collect();
    ensure!(
        said.is_empty(),
        "the installer stopped with {ending}: {}",
        tail.into_iter().rev().collect::<Vec<_>>().join(" · ")
    );
    anyhow::bail!("the installer stopped with {ending}")
}

#[cfg(test)]
mod tests {
    use super::{INSTALL_DIR, ordinal, run};

    #[test]
    fn versions_compare_by_number_rather_than_by_text() {
        assert!(ordinal("0.0.10") > ordinal("0.0.9"));
        assert!(ordinal("v0.1.0") > ordinal("0.0.99"));
        assert!(ordinal("0.0.7") > ordinal("0.0.4"));
        assert_eq!(ordinal("0.0.7"), ordinal("v0.0.7"));
        // Nothing published carries a suffix, so it is ignored rather than
        // ranked by a rule nobody agreed on.
        assert_eq!(ordinal("0.0.7-rc1"), ordinal("0.0.7"));
    }

    /// The installer is a subprocess, and this process owns its own streams: a
    /// machine reading them is entitled to find only what Cerul wrote there.
    #[test]
    fn the_installer_writes_to_no_stream_of_ours_and_its_failure_becomes_ours() {
        let directory = tempfile::tempdir().unwrap();
        let noisy = "#!/bin/sh\necho progress on stdout\necho detail on stderr >&2\n";
        // A silent success: nothing was printed here, and nothing failed.
        run(&format!("{noisy}exit 0\n"), None).unwrap();

        let error = run(&format!("{noisy}echo 'no space left' >&2\nexit 3\n"), None)
            .expect_err("a failing installer is a failure");
        let text = format!("{error:#}");
        assert!(text.contains("exit 3"), "{text}");
        // What the installer said about its own failure is the useful part.
        assert!(text.contains("no space left"), "{text}");

        // Where it writes is where the running program already lives, so an
        // upgrade replaces that copy instead of adding one somewhere else.
        let seen = directory.path().join("seen");
        run(
            &format!(
                "#!/bin/sh\nprintf '%s' \"${INSTALL_DIR}\" > {}\n",
                seen.display()
            ),
            Some(directory.path()),
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&seen).unwrap(),
            directory.path().to_string_lossy()
        );
    }
}
