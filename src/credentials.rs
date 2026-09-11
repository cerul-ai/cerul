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

pub fn resolver(
    cli: &super::Cli,
    cancel: CancellationToken,
) -> cerul::providers::CredentialResolver {
    let interactive = !cli.json
        && !cli.quiet
        && !cli.yes
        && !cli.dry_run
        && std::io::stdin().is_terminal()
        && std::io::stderr().is_terminal();
    let keys = std::sync::Arc::new(tokio::sync::Mutex::new(None::<Keys>));
    std::sync::Arc::new(move |endpoint| {
        let keys = keys.clone();
        let cancel = cancel.clone();
        Box::pin(async move {
            let mut stored = keys.lock().await;
            *stored = Some(path().as_deref().map(read).transpose()?.unwrap_or_default());
            let keys = stored.as_mut().unwrap();
            let scope = credential_scope(&endpoint);
            if let Some(key) = keys.get(&scope) {
                return Ok(Some(key.clone()));
            }
            if !interactive
                || endpoint.kind != "gemini"
                || endpoint.base_url.trim_end_matches('/')
                    != "https://generativelanguage.googleapis.com/v1beta"
            {
                return Ok(None);
            }
            prompt(&endpoint, keys, cancel).await
        })
    })
}

/// Where the key for an endpoint would come from, for display.
pub fn state(endpoint: &Endpoint) -> crate::render::KeyState {
    let saved = path()
        .as_deref()
        .map(read)
        .and_then(Result::ok)
        .is_some_and(|keys| keys.contains_key(&credential_scope(endpoint)));
    crate::render::KeyState {
        provider: endpoint.kind.clone(),
        endpoint: endpoint.base_url.clone(),
        env: endpoint.api_key_env.clone(),
        env_set: std::env::var(&endpoint.api_key_env).is_ok_and(|v| !v.trim().is_empty()),
        saved,
        credentials_path: path(),
    }
}

/// Interactive replacement of the saved key, used by `cerul auth set`.
pub async fn set(endpoint: &Endpoint, cancel: CancellationToken) -> Result<()> {
    ensure!(
        std::io::stdin().is_terminal() && std::io::stderr().is_terminal(),
        "cerul auth set needs an interactive terminal; export {} instead",
        endpoint.api_key_env
    );
    let mut keys = path().as_deref().map(read).transpose()?.unwrap_or_default();
    prompt(endpoint, &mut keys, cancel)
        .await?
        .context("this endpoint does not support saved keys; export the configured environment variable instead")?;
    Ok(())
}

/// Deletes the saved key for an endpoint. Returns whether one existed.
pub fn remove(endpoint: &Endpoint) -> Result<bool> {
    let Some(path) = path() else {
        return Ok(false);
    };
    let mut keys = read(&path)?;
    let existed = keys.remove(&credential_scope(endpoint)).is_some();
    if existed {
        save(&path, &keys)?;
    }
    Ok(existed)
}

async fn prompt(
    endpoint: &Endpoint,
    keys: &mut Keys,
    cancel: CancellationToken,
) -> Result<Option<String>> {
    let scope = credential_scope(endpoint);
    let path = path();
    let palette = crate::render::Palette::new(console::colors_enabled_stderr());
    eprintln!();
    eprintln!(
        "{}",
        palette.bold(&format!(
            "Configure {} at {}",
            endpoint.model, endpoint.base_url
        ))
    );
    if endpoint.kind == "gemini" {
        eprintln!("  Get a key at https://aistudio.google.com/apikey");
    }
    eprintln!(
        "  {}",
        palette.dim("Stored privately in ~/.cerul/credentials.json. The selected service receives media during processing; API charges may apply.")
    );
    let key =
        rpassword::prompt_password("API key (hidden, blank to cancel): ").inspect_err(|error| {
            if error.kind() == std::io::ErrorKind::Interrupted {
                cancel.cancel();
            }
        })?;
    if key.trim().is_empty() {
        cancel.cancel();
        anyhow::bail!("setup cancelled");
    }
    eprintln!("  checking the configured endpoint…");
    let mut validation = endpoint.clone();
    if validation.kind == "gemini" && !validation.model.starts_with("gemini-3.5-transcribe") {
        validation.model = "gemini-embedding-2".into();
        validation.dims = Some(1536);
    }
    let embedding_check = validation.dims.is_some();
    let provider = Provider::new(validation, Some(key.trim().to_owned()), 1, None, cancel)?;
    let checked = if embedding_check {
        provider
            .embed(Input::Text("Cerul setup".into()), true)
            .await
            .map(|_| ())
    } else {
        let workspace = tempfile::tempdir()?;
        cerul::providers::probes::check(
            &provider,
            cerul::providers::probes::Capability::Transcription,
            workspace.path(),
            true,
        )
        .await
        .map(|_| ())
    };
    checked.context("endpoint validation failed; key was not saved")?;
    keys.insert(scope, key.trim().to_owned());
    save(
        path.as_deref()
            .context("HOME is required to save credentials")?,
        keys,
    )?;
    eprintln!("{} Key verified and saved", palette.ok("✓"));
    eprintln!();
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
