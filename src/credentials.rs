//! CLI-only credential storage and onboarding. Never included in the library API.
use anyhow::{Context, Result, ensure};
use cerul::{
    config::Endpoint,
    providers::{Input, Provider, credential_scope},
};
use std::{
    collections::BTreeMap,
    fs,
    io::{IsTerminal, Write},
    path::{Path, PathBuf},
};
use tokio_util::sync::CancellationToken;

type Keys = BTreeMap<String, String>;
fn path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".cerul/credentials.json"))
}
fn read(path: &Path) -> Result<Keys> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Keys::new()),
        Err(e) => return Err(e.into()),
    };
    ensure!(metadata.is_file(), "credential file must be a regular file");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        ensure!(
            metadata.permissions().mode() & 0o077 == 0,
            "credential file must be private: chmod 600 ~/.cerul/credentials.json"
        );
    }
    serde_json::from_slice(&fs::read(path)?).context("invalid credential file")
}
fn save(path: &Path, keys: &Keys) -> Result<()> {
    let parent = path.parent().context("credential path has no parent")?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    serde_json::to_writer(&mut file, keys)?;
    file.flush()?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}

pub async fn prepare(cli: &super::Cli, cancel: CancellationToken) -> Result<Keys> {
    if cli.dry_run
        || matches!(
            &cli.command,
            None | Some(super::Command::Clean(_))
                | Some(super::Command::Status {
                    providers: false,
                    ..
                })
        )
        || matches!(&cli.command, Some(super::Command::Search(args)) if args.text)
    {
        return Ok(Keys::new());
    }
    let path = path();
    let mut keys = path.as_deref().map(read).transpose()?.unwrap_or_default();
    let interactive = !cli.json
        && !cli.quiet
        && !cli.yes
        && !cli.dry_run
        && std::io::stdin().is_terminal()
        && std::io::stderr().is_terminal();
    if !interactive {
        return Ok(keys);
    }
    let config = super::config(cli)?;
    let endpoint: Option<&Endpoint> = match &cli.command {
        Some(super::Command::Index(args)) if args.paths.iter().all(|p| p.exists()) => {
            Some(&config.embedding)
        }
        Some(super::Command::Search(args))
            if !args.text && (args.query.is_some() || args.image.is_some()) =>
        {
            Some(&config.embedding)
        }
        Some(super::Command::Annotate(args))
            if args.semantic.is_some()
                && args.grounding.is_none()
                && args.world.is_none()
                && args.paths.iter().all(|p| p.exists()) =>
        {
            Some(&config.vision)
        }
        _ => None,
    };
    let Some(endpoint) = endpoint else {
        return Ok(keys);
    };
    let scope = credential_scope(endpoint);
    if std::env::var_os(&endpoint.api_key_env).is_some() || keys.contains_key(&scope) {
        return Ok(keys);
    }
    let url = url::Url::parse(&endpoint.base_url)?;
    if matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")) {
        return Ok(keys);
    }
    // Automatic onboarding is limited to the default Gemini service. Custom providers
    // keep their explicit environment-variable setup and capability requirements.
    if endpoint.kind != "gemini"
        || endpoint.base_url.trim_end_matches('/')
            != "https://generativelanguage.googleapis.com/v1beta"
    {
        return Ok(keys);
    }
    eprintln!(
        "First run: Cerul needs a Gemini API key. Get one at https://aistudio.google.com/apikey"
    );
    eprintln!(
        "The key is stored privately in ~/.cerul/credentials.json. Configured models receive media during processing and may charge usage. A small text request validates this key now."
    );
    let key = rpassword::prompt_password("Gemini API key (hidden; leave blank to cancel): ")
        .inspect_err(|error| {
            if error.kind() == std::io::ErrorKind::Interrupted {
                cancel.cancel();
            }
        })?;
    ensure!(
        !key.trim().is_empty(),
        "setup cancelled; set {} or run again in a terminal",
        endpoint.api_key_env
    );
    let validation = cerul::config::Config::default().embedding;
    let provider = Provider::new(validation, Some(key.trim().to_owned()), 1, None, cancel)?;
    provider
        .embed(Input::Text("Cerul setup".into()), true)
        .await
        .context("API key validation failed; key was not saved")?;
    keys.insert(scope, key.trim().to_owned());
    save(
        path.as_deref()
            .context("HOME is required to save credentials")?,
        &keys,
    )?;
    eprintln!("API key verified and saved. Continuing…");
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_round_trip_and_endpoint_scope() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("credentials.json");
        let mut endpoint = cerul::config::Config::default().embedding;
        let scope = credential_scope(&endpoint);
        let keys = Keys::from([(scope.clone(), "test-only-key".into())]);
        save(&file, &keys).unwrap();
        assert_eq!(read(&file).unwrap(), keys);
        endpoint.base_url = "https://another.example/v1".into();
        assert_ne!(scope, credential_scope(&endpoint));
        #[cfg(unix)]
        {
            use std::os::unix::fs::{PermissionsExt, symlink};
            fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
            assert!(read(&file).is_err());
            let link = dir.path().join("link");
            symlink(&file, &link).unwrap();
            assert!(read(&link).is_err());
        }
    }
}
