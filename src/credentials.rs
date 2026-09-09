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

pub async fn prepare(cli: &super::Cli) -> Result<Keys> {
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
    path()
        .as_deref()
        .map(read)
        .transpose()
        .map(Option::unwrap_or_default)
}

pub fn resolver(
    cli: &super::Cli,
    keys: Keys,
    cancel: CancellationToken,
) -> cerul::providers::CredentialResolver {
    let interactive = !cli.json
        && !cli.quiet
        && !cli.yes
        && !cli.dry_run
        && std::io::stdin().is_terminal()
        && std::io::stderr().is_terminal();
    let keys = std::sync::Arc::new(tokio::sync::Mutex::new(keys));
    std::sync::Arc::new(move |endpoint| {
        let keys = keys.clone();
        let cancel = cancel.clone();
        Box::pin(async move {
            if !interactive {
                return Ok(None);
            }
            let mut keys = keys.lock().await;
            let scope = credential_scope(&endpoint);
            if let Some(key) = keys.get(&scope) {
                return Ok(Some(key.clone()));
            }
            prompt(&endpoint, &mut keys, cancel).await
        })
    })
}

async fn prompt(
    endpoint: &Endpoint,
    keys: &mut Keys,
    cancel: CancellationToken,
) -> Result<Option<String>> {
    if endpoint.kind != "gemini"
        || endpoint.base_url.trim_end_matches('/')
            != "https://generativelanguage.googleapis.com/v1beta"
    {
        return Ok(None);
    }
    let scope = credential_scope(endpoint);
    let path = path();
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
        keys,
    )?;
    eprintln!("API key verified and saved. Continuing…");
    Ok(Some(key.trim().to_owned()))
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
