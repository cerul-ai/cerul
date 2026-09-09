//! Explicit live checks of the default Gemini endpoints using synthetic inputs only.
use anyhow::{Context, Result, ensure};
use cerul::{
    config::Config,
    providers::{Provider, probes},
};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<()> {
    ensure!(
        std::env::var("GEMINI_API_KEY").is_ok_and(|key| !key.trim().is_empty()),
        "GEMINI_API_KEY is required for live acceptance; no requests were sent"
    );
    let config = Config::default();
    let workspace = tempfile::tempdir()?;
    for (endpoint, capability) in [
        (config.embedding, probes::Capability::Embedding),
        (config.vision, probes::Capability::Vision),
        (config.transcription, probes::Capability::Transcription),
    ] {
        let provider = Provider::from_env(endpoint, 1, Some(30), CancellationToken::new())?;
        let result = probes::check(&provider, capability, workspace.path(), true)
            .await
            .with_context(|| format!("live {capability:?} capability check failed"))?;
        println!("{}", serde_json::to_string(&result)?);
    }
    Ok(())
}
