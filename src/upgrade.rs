//! Finding and installing the newest published release.
//!
//! Nothing here runs on its own: the command asks GitHub what the newest
//! release is, says what it found, and installs only when a person or an
//! explicit `--yes` says so. The script it runs is the one that release
//! published, so what gets installed is the version that was reported.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::io::Write;

/// Where releases are published. Named as a constant so the command can print
/// exactly where it will look before it looks.
const LATEST: &str = "https://api.github.com/repos/cerul-ai/cerul/releases/latest";
/// The installer each release publishes, and the one the documented install
/// command fetches.
const INSTALLER: &str = "cerul-installer.sh";
/// Executables the bundle installs together. Keeping them together is what
/// makes the media tools match the CLI that calls them.
pub const INSTALLED: [&str; 3] = ["cerul", "cerul-ffmpeg", "cerul-ffprobe"];

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
pub async fn check() -> Result<Available> {
    let response = client()?
        .get(LATEST)
        .send()
        .await
        .context("could not reach GitHub Releases")?;
    ensure!(
        response.status().is_success(),
        "GitHub Releases answered HTTP {}",
        response.status().as_u16()
    );
    let release: Release = response
        .json()
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

/// Runs the installer that release published. It writes the whole bundle to the
/// same place the documented install command does, so an upgrade and a fresh
/// install leave the machine in the same state.
pub async fn install(available: &Available) -> Result<()> {
    let response = client()?
        .get(&available.installer)
        .send()
        .await
        .context("could not download the installer")?;
    ensure!(
        response.status().is_success(),
        "downloading the installer answered HTTP {}",
        response.status().as_u16()
    );
    let script = response
        .text()
        .await
        .context("the installer did not read")?;
    // A redirect that lands on an error page is a page, not a script. Running it
    // would be running whatever the page happens to contain.
    ensure!(
        script.starts_with("#!"),
        "what was downloaded is not a shell script; install manually from {}",
        available.installer
    );
    let mut file = tempfile::Builder::new()
        .prefix("cerul-installer-")
        .suffix(".sh")
        .tempfile()
        .context("could not write the installer to a temporary file")?;
    file.write_all(script.as_bytes())?;
    file.flush()?;
    let status = std::process::Command::new("sh")
        .arg(file.path())
        .status()
        .context("could not run the installer")?;
    ensure!(
        status.success(),
        "the installer stopped with {}",
        status
            .code()
            .map(|code| format!("exit {code}"))
            .unwrap_or_else(|| "a signal".into())
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ordinal;

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
}
