//! Local execution measurements; never retain media, prompts, keys or responses.
use crate::{providers::usage::RequestReport, storage};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    future::Future,
    path::Path,
    sync::{Arc, Mutex},
    time::Instant,
};

tokio::task_local! { static CURRENT: Context; }

#[derive(Clone)]
pub(crate) struct Context {
    data: Arc<Mutex<Report>>,
    stage: usize,
}
#[derive(Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Cache {
    pub hits: u64,
    pub misses: u64,
}
#[derive(Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Stage {
    pub name: String,
    pub episode: String,
    pub stream: String,
    pub elapsed_ms: u64,
    pub requests: u64,
    pub request_elapsed_ms: u64,
    pub request_max_ms: u64,
    pub retries: u64,
    pub caches: BTreeMap<String, Cache>,
}
#[derive(Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Report {
    pub schema: String,
    pub command: String,
    pub started_at_unix_ms: u64,
    pub elapsed_ms: u64,
    pub failed: bool,
    pub partial: bool,
    pub stages: Vec<Stage>,
}
pub(crate) fn current() -> Option<Context> {
    CURRENT.try_with(Clone::clone).ok()
}
pub(crate) fn sync<T>(context: Option<Context>, work: impl FnOnce() -> T) -> T {
    if let Some(context) = context {
        CURRENT.sync_scope(context, work)
    } else {
        work()
    }
}
pub(crate) fn request(context: Option<&Context>, request: &RequestReport) {
    if let Some(context) = context {
        let mut data = context.data.lock().unwrap();
        let stage = &mut data.stages[context.stage];
        stage.requests += 1;
        stage.request_elapsed_ms += request.elapsed_ms;
        stage.request_max_ms = stage.request_max_ms.max(request.elapsed_ms);
        stage.retries += u64::from(request.attempt > 1);
    }
}
pub(crate) fn cache(kind: &str, hit: bool) {
    if let Some(context) = current() {
        let mut data = context.data.lock().unwrap();
        let count = data.stages[context.stage]
            .caches
            .entry(kind.into())
            .or_default();
        if hit {
            count.hits += 1;
        } else {
            count.misses += 1;
        }
    }
}
pub(crate) fn partial(value: bool) {
    if let Some(context) = current() {
        context.data.lock().unwrap().partial |= value;
    }
}
struct Timer {
    context: Context,
    start: Instant,
}
impl Drop for Timer {
    fn drop(&mut self) {
        self.context.data.lock().unwrap().stages[self.context.stage].elapsed_ms =
            self.start.elapsed().as_millis() as u64;
    }
}
pub(crate) async fn stage<T>(
    name: &str,
    episode: &str,
    stream: &str,
    work: impl Future<Output = T>,
) -> T {
    let Some(mut context) = current() else {
        return work.await;
    };
    {
        let mut data = context.data.lock().unwrap();
        context.stage = data.stages.len();
        data.stages.push(Stage {
            name: name.into(),
            episode: episode.into(),
            stream: stream.into(),
            ..Default::default()
        });
    }
    let _timer = Timer {
        context: context.clone(),
        start: Instant::now(),
    };
    CURRENT.scope(context, work).await
}
pub(crate) async fn run<T>(
    workspace: &Path,
    command: &str,
    dry_run: bool,
    work: impl Future<Output = Result<T>>,
) -> Result<T> {
    if dry_run {
        return work.await;
    }
    let data = Arc::new(Mutex::new(Report {
        schema: "execution-diagnostics/1".into(),
        command: command.into(),
        started_at_unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        stages: vec![Stage {
            name: "invocation".into(),
            ..Default::default()
        }],
        ..Default::default()
    }));
    let start = Instant::now();
    let result = CURRENT
        .scope(
            Context {
                data: data.clone(),
                stage: 0,
            },
            work,
        )
        .await;
    let mut report = data.lock().unwrap();
    report.elapsed_ms = start.elapsed().as_millis() as u64;
    report.failed = result.is_err();
    // The root stage spans the invocation; child durations can overlap.
    report.stages[0].elapsed_ms = report.elapsed_ms;
    let path = workspace
        .join("runtime/diagnostics")
        .join(format!("{command}-latest.json"));
    // Diagnostics cannot turn a published index into a failed operation.
    let _ = storage::write_json(&path, &*report);
    result
}
pub fn read(workspace: &Path) -> Result<BTreeMap<String, Report>> {
    let mut reports = BTreeMap::new();
    for command in ["index", "analyze"] {
        let path = workspace
            .join("runtime/diagnostics")
            .join(format!("{command}-latest.json"));
        if path.is_file() {
            reports.insert(
                command.into(),
                serde_json::from_slice(&std::fs::read(path)?)?,
            );
        }
    }
    Ok(reports)
}
