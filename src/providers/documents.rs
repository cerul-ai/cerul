//! Content-addressed document vectors, with ordered locks and bounded batches.
use super::*;
use crate::storage::{self, Checkpoints};
use futures::{StreamExt, TryStreamExt};
use std::{collections::BTreeMap, path::Path};

#[derive(Default)]
pub(super) struct Documents {
    single_only: std::sync::atomic::AtomicBool,
    locks: std::sync::Mutex<BTreeMap<String, Arc<Mutex<()>>>>,
}

impl Provider {
    /// Reuse identical documents without merging their downstream time ranges.
    /// A recompute refreshes each distinct document in the supplied batch.
    pub async fn embed_documents(
        &self,
        sidecar: &Path,
        texts: &[String],
        recompute: bool,
    ) -> Result<Vec<Vec<f32>>> {
        let dims = self.endpoint.dims.context("missing embedding dimensions")?;
        let keys: Vec<String> = texts
            .iter()
            .map(|text| {
                storage::cache_key(&(
                    "document-vector/1",
                    &self.endpoint.kind,
                    &self.endpoint.base_url,
                    &self.endpoint.model,
                    dims,
                    self.endpoint.document_template(),
                    text,
                ))
            })
            .collect::<Result<_>>()?;
        let unique: BTreeMap<_, _> = keys
            .iter()
            .zip(texts)
            .map(|(k, t)| (k.clone(), t))
            .collect();
        let locks: Vec<_> = {
            let mut locks = self.documents.locks.lock().unwrap();
            unique
                .keys()
                .map(|key| locks.entry(key.clone()).or_default().clone())
                .collect()
        };
        let mut guards = Vec::new();
        // Global key order prevents deadlocks between overlapping document batches.
        for lock in locks {
            guards.push(tokio::select! { biased;
                _ = self.cancel.cancelled() => return Err(failure(Failure::Cancelled,"operation cancelled")),
                guard = lock.lock_owned() => guard,
            });
        }
        let checkpoints = Checkpoints::new(sidecar);
        let mut values = BTreeMap::new();
        let mut missing = Vec::new();
        for (key, text) in &unique {
            let cached = if !recompute {
                checkpoints.load::<Vec<f32>>(key)?
            } else {
                None
            };
            if let Some(vector) = cached.filter(|v| valid(v, dims)) {
                values.insert(key.clone(), vector);
            } else {
                missing.push((key.clone(), (*text).clone()));
            }
        }
        let width = if !self
            .documents
            .single_only
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            32
        } else {
            1
        };
        let checkpoints = &checkpoints;
        let work = futures::stream::iter(missing.chunks(width))
            .map(|batch| async move {
                let texts: Vec<_> = batch.iter().map(|(_, t)| t.clone()).collect();
                let vectors = self.document_batch(&texts).await?;
                for ((key, _), vector) in batch.iter().zip(&vectors) {
                    checkpoints.save(key, vector)?;
                }
                Ok::<_, anyhow::Error>(
                    batch
                        .iter()
                        .map(|(k, _)| k.clone())
                        .zip(vectors)
                        .collect::<Vec<_>>(),
                )
            })
            .buffer_unordered(self.concurrency());
        tokio::pin!(work);
        while let Some(batch) = work.try_next().await? {
            values.extend(batch);
        }

        keys.iter()
            .map(|key| values.get(key).cloned().context("missing document vector"))
            .collect()
    }

    async fn document_singles(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        futures::stream::iter(texts)
            .map(|text| self.embed(Input::Text(text.clone()), false))
            .buffered(self.concurrency())
            .try_collect()
            .await
    }

    async fn document_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.len() == 1
            || self
                .documents
                .single_only
                .load(std::sync::atomic::Ordering::Relaxed)
        {
            return self.document_singles(texts).await;
        }
        let dims = self.endpoint.dims.context("missing dimensions")?;
        let input: Vec<_> = texts
            .iter()
            .map(|text| {
                self.endpoint
                    .document_template()
                    .map(|template| template.replace("{text}", text))
                    .unwrap_or_else(|| text.clone())
            })
            .collect();
        let (action, body) = if self.endpoint.kind == "gemini" {
            let model = format!(
                "models/{}",
                self.endpoint.model.trim_start_matches("models/")
            );
            (
                "batchEmbedContents",
                json!({"requests":input.iter().map(|text|
                json!({"model":model,"content":{"parts":[{"text":text}]},"outputDimensionality":dims})
            ).collect::<Vec<_>>()}),
            )
        } else {
            (
                "embeddings",
                json!({"model":self.endpoint.model,"input":input,"dimensions":dims,"encoding_format":"float"}),
            )
        };
        let response = self.json(action, body).await;
        let response = match response {
            Ok(response) => response,
            Err(error)
                if error
                    .downcast_ref::<ProviderError>()
                    .is_some_and(|e| e.kind == Failure::Unsupported) =>
            {
                self.documents
                    .single_only
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                return self.document_singles(texts).await;
            }
            Err(error) => return Err(error),
        };
        if self.endpoint.kind == "gemini" {
            let embeddings = response["embeddings"]
                .as_array()
                .context("missing batch embeddings")?;
            anyhow::ensure!(
                embeddings.len() == texts.len(),
                "embedding batch count mismatch"
            );
            return embeddings
                .iter()
                .map(|item| {
                    let vector: Vec<f32> = serde_json::from_value(item["values"].clone())?;
                    anyhow::ensure!(valid(&vector, dims), "invalid document vector");
                    Ok(vector)
                })
                .collect();
        }
        let data = response["data"]
            .as_array()
            .context("missing batch embeddings")?;
        anyhow::ensure!(data.len() == texts.len(), "embedding batch count mismatch");
        let mut result = vec![None; texts.len()];
        for item in data {
            let i = item["index"].as_u64().context("missing batch index")? as usize;
            anyhow::ensure!(
                i < result.len() && result[i].is_none(),
                "invalid embedding batch index"
            );
            let vector: Vec<f32> = serde_json::from_value(item["embedding"].clone())?;
            anyhow::ensure!(valid(&vector, dims), "invalid document vector");
            result[i] = Some(vector);
        }
        result
            .into_iter()
            .map(|v| v.context("missing batch item"))
            .collect()
    }
}

fn valid(vector: &[f32], dims: usize) -> bool {
    vector.len() == dims && vector.iter().all(|v| v.is_finite()) && vector.iter().any(|v| *v != 0.)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn gemini_batch_keeps_documents_separate_and_sets_dimensions() {
        let (base, server) = super::super::tests::server(vec![(
            200,
            json!({"embeddings":[{"values":[1.,0.]},{"values":[0.,1.]}]}),
        )]);
        let mut endpoint = crate::config::Config::default().embedding;
        endpoint.base_url = base;
        endpoint.dims = Some(2);
        let provider = Provider::new(endpoint, None, 2, None, CancellationToken::new()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let values = provider
            .embed_documents(dir.path(), &["one".into(), "two".into()], false)
            .await
            .unwrap();
        assert_ne!(values[0], values[1]);
        let calls = server.join().unwrap();
        assert!(calls[0].0.contains("batchEmbedContents"));
        let requests = calls[0].1["requests"].as_array().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|r| r["outputDimensionality"] == 2
            && r["content"]["parts"].as_array().unwrap().len() == 1));
    }
    #[tokio::test]
    async fn unsupported_batches_fall_back_without_disabling_individual_embeddings() {
        let single = json!({"data":[{"index":0,"embedding":[1.,0.]}]});
        let (base, server) = super::super::tests::server(vec![
            (400, json!({})),
            (200, single.clone()),
            (200, single.clone()),
            (200, single.clone()),
            (200, single),
        ]);
        let mut endpoint = crate::config::Config::default().embedding;
        endpoint.kind = "openai".into();
        endpoint.model = "fixture".into();
        endpoint.base_url = base;
        endpoint.dims = Some(2);
        let provider = Provider::new(endpoint, None, 2, None, CancellationToken::new()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            provider
                .embed_documents(dir.path(), &["one".into(), "two".into()], false)
                .await
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            provider
                .embed_documents(dir.path(), &["three".into(), "four".into()], false)
                .await
                .unwrap()
                .len(),
            2
        );
        let calls = server.join().unwrap();
        assert!(calls[0].1["input"].is_array());
        assert!(calls[1..].iter().all(|(_, body)| body["input"].is_string()));
    }
    #[tokio::test]
    async fn batches_deduplicate_coalesce_and_restore_indexed_response_order() {
        let response =
            json!({"data":[{"index":1,"embedding":[0.,1.]},{"index":0,"embedding":[1.,0.]}]});
        let (base, server) = super::super::tests::server(vec![(200, response)]);
        let mut endpoint = crate::config::Config::default().embedding;
        endpoint.kind = "openai".into();
        endpoint.model = "fixture".into();
        endpoint.base_url = base;
        endpoint.dims = Some(2);
        let provider = Provider::new(endpoint, None, 4, None, CancellationToken::new()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let texts = vec!["one".to_owned(), "two".to_owned(), "one".to_owned()];
        let (a, b) = tokio::join!(
            provider.embed_documents(dir.path(), &texts, false),
            provider.embed_documents(dir.path(), &texts, false)
        );
        let a = a.unwrap();
        assert_eq!(a, b.unwrap());
        assert_eq!(a[0], a[2]);
        assert_ne!(a[0], a[1]);
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].1["input"].as_array().unwrap().len(), 2);
        assert_eq!(
            provider
                .embed_documents(dir.path(), &texts, false)
                .await
                .unwrap(),
            a
        );
    }
    #[tokio::test]
    async fn malformed_batch_cannot_publish_documents() {
        let (base, server) = super::super::tests::server(vec![(
            200,
            json!({"data":[{"index":0,"embedding":[1.,0.]},{"index":0,"embedding":[0.,1.]}]}),
        )]);
        let mut endpoint = crate::config::Config::default().embedding;
        endpoint.kind = "openai".into();
        endpoint.model = "fixture".into();
        endpoint.base_url = base;
        endpoint.dims = Some(2);
        let provider = Provider::new(endpoint, None, 2, None, CancellationToken::new()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        assert!(
            provider
                .embed_documents(dir.path(), &["one".into(), "two".into()], false)
                .await
                .is_err()
        );
        assert_eq!(server.join().unwrap().len(), 1);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
