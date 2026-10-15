//! Intent-driven short clips from one long video, selected on the transcript.
//!
//! See `docs/design/clip-command.md` for the behavioral baseline.
pub mod brief;
pub mod utterances;

use crate::providers::Provider;
use anyhow::Result;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// One structured generation, parsed and checked by the caller's rule. Invalid
/// output earns exactly one repair attempt that names what was wrong; a second
/// failure is the caller's error.
pub(crate) async fn generate_validated<T: DeserializeOwned>(
    provider: &Provider,
    prompt: &str,
    schema: &Value,
    validate: impl Fn(&T) -> Result<()>,
) -> Result<T> {
    let validate = &validate;
    let attempt = |prompt: String| async move {
        let value = provider.generate(&prompt, &[], schema.clone()).await?;
        let parsed: T = serde_json::from_value(value)?;
        validate(&parsed)?;
        Ok::<T, anyhow::Error>(parsed)
    };
    match attempt(prompt.to_owned()).await {
        Ok(value) => Ok(value),
        Err(error) => {
            if let Some(provider_error) = error.downcast_ref::<crate::providers::ProviderError>()
                && provider_error.kind != crate::providers::Failure::InvalidResponse
            {
                return Err(error);
            }
            crate::media::check_cancellation()?;
            let repair = format!(
                "{prompt}\n\nYour previous response was rejected: {error:#}. Return only valid output that follows every rule above."
            );
            attempt(repair).await
        }
    }
}
