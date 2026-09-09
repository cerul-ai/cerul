mod credentials;
use anyhow::{Context, Result};
use cerul::{
    config::Config,
    events::Event,
    index::{embed, pipeline},
    providers::{Failure, ProviderError},
};
use clap::{Args, Parser, Subcommand};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio_util::sync::CancellationToken;

#[derive(Parser)]
#[command(
    name = "cerul",
    version,
    about = "Searchable video and annotation data"
)]
struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[arg(long, env = "CERUL_WORKSPACE", global = true)]
    workspace: Option<PathBuf>,
    #[arg(long, global = true)]
    recompute: bool,
    #[arg(long, global = true)]
    dry_run: bool,
    #[arg(long, global = true)]
    yes: bool,
    /// Override a configuration field, for example embedding.dims=1536.
    #[arg(long = "set", global = true, value_name = "KEY=TOML_VALUE")]
    overrides: Vec<String>,
    #[arg(short = 'q', long, global = true)]
    quiet: bool,
    #[arg(short='v', action=clap::ArgAction::Count, global=true)]
    verbose: u8,
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand)]
enum Command {
    /// Show local episode, annotation, and vector space inventory.
    Status {
        path: Option<PathBuf>,
        /// Probe configured endpoints, reusing successful checks for seven days.
        #[arg(long)]
        providers: bool,
    },
    /// Index videos using OCR, transcription, and multimodal embeddings.
    Index(IndexArgs),
    /// Remove disposable indexes/cache, or explicitly selected sidecars.
    Clean(CleanArgs),
    /// Search video, speech, screen text, or annotation intervals.
    Search(SearchArgs),
    /// Generate semantic video annotations.
    Annotate(AnnotateArgs),
}
#[derive(Args)]
struct AnnotateArgs {
    #[arg(required = true)]
    paths: Vec<PathBuf>,
    #[arg(long,num_args=0..=1,default_missing_value="default")]
    semantic: Option<String>,
    #[arg(long,num_args=0..=1,default_missing_value="default")]
    grounding: Option<String>,
    #[arg(long,num_args=0..=1,default_missing_value="default")]
    world: Option<String>,
    #[arg(long, default_value = "primary")]
    streams: String,
    #[arg(long)]
    only: Option<String>,
    #[arg(long)]
    ontology: Option<PathBuf>,
    #[arg(long,default_value="30s",value_parser=duration)]
    window: i64,
    #[arg(long, default_value_t = 2.)]
    fps: f64,
    #[arg(long, default_value_t = 4)]
    jobs: usize,
    #[arg(long)]
    rpm: Option<u32>,
    #[arg(long)]
    write_lerobot: bool,
    #[arg(long)]
    out: Option<PathBuf>,
}
#[derive(Args)]
struct SearchArgs {
    query: Option<String>,
    #[arg(long, conflicts_with = "query")]
    image: Option<PathBuf>,
    #[arg(long)]
    text: bool,
    #[arg(long = "filter")]
    filters: Vec<String>,
    #[arg(long = "in")]
    within: Option<PathBuf>,
    #[arg(long, default_value_t = 10)]
    limit: usize,
    #[arg(long)]
    threshold: Option<f32>,
    #[arg(long)]
    count: bool,
    #[arg(long)]
    save: Option<PathBuf>,
    #[arg(long,default_value="2s",value_parser=duration)]
    pad: i64,
}
#[derive(Args)]
struct CleanArgs {
    #[arg(long, conflicts_with_all=["all_indexes", "compact"])]
    index: Option<String>,
    #[arg(long, conflicts_with = "compact")]
    all_indexes: bool,
    #[arg(long)]
    cache: bool,
    #[arg(long)]
    compact: bool,
    #[arg(long)]
    sidecars: Option<PathBuf>,
}
#[derive(Args)]
struct IndexArgs {
    #[arg(required = true)]
    paths: Vec<PathBuf>,
    #[arg(long, default_value="30s", value_parser=duration)]
    chunk: i64,
    #[arg(long, default_value="5s", value_parser=duration)]
    overlap: i64,
    #[arg(long)]
    no_audio: bool,
    #[arg(long)]
    no_ocr: bool,
    #[arg(long)]
    skip_still: bool,
    #[arg(long, default_value = "primary")]
    streams: String,
    #[arg(long)]
    only: Option<String>,
    #[arg(long, default_value_t = 4)]
    jobs: usize,
    #[arg(long)]
    rpm: Option<u32>,
    #[arg(long)]
    sidecar_dir: Option<PathBuf>,
}
fn duration(raw: &str) -> std::result::Result<i64, String> {
    let (number, factor) = if let Some(value) = raw.strip_suffix("ms") {
        (value, 1_000i64)
    } else if let Some(value) = raw.strip_suffix("us") {
        (value, 1i64)
    } else if let Some(value) = raw.strip_suffix('s') {
        (value, 1_000_000i64)
    } else if let Some(value) = raw.strip_suffix('m') {
        (value, 60_000_000i64)
    } else {
        return Err("duration requires us, ms, s, or m".into());
    };
    let value = number.parse::<f64>().map_err(|_| "invalid duration")? * factor as f64;
    if !value.is_finite()
        || value < 0.
        || value >= i64::MAX as f64
        || value.fract().abs() > 0.000001
    {
        return Err("duration must be nonnegative whole microseconds".into());
    }
    Ok(value.round() as i64)
}
#[derive(Debug)]
struct CliError(u8, String);
impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.1)
    }
}
impl std::error::Error for CliError {}
fn category(code: u8, error: anyhow::Error) -> anyhow::Error {
    CliError(code, format!("{error:#}")).into()
}
fn exit_code(error: &anyhow::Error) -> u8 {
    if let Some(error) = error.downcast_ref::<CliError>() {
        return error.0;
    }
    match error.downcast_ref::<ProviderError>().map(|e| e.kind) {
        Some(Failure::Cancelled) => 5,
        Some(Failure::MissingKey) => 2,
        Some(Failure::Unsupported) => 3,
        _ => 4,
    }
}
fn event(json: bool, quiet: bool, value: Event) {
    let mut out = io::stderr().lock();
    if json {
        let _ = serde_json::to_writer(&mut out, &value);
        let _ = writeln!(out);
    } else if !quiet {
        match value {
            Event::Log { level, msg } => {
                let _ = writeln!(out, "{level}: {msg}");
            }
            Event::Progress {
                episode,
                station,
                done,
                total,
            } => {
                let _ = writeln!(out, "{episode} {station}: {done}/{total}");
            }
        }
    }
}
fn config(cli: &Cli) -> Result<Config> {
    let mut overlay = toml::Table::new();
    for item in &cli.overrides {
        let (key, value) = item
            .split_once('=')
            .context("--set requires KEY=TOML_VALUE")?;
        let (section, field) = key
            .split_once('.')
            .context("--set key requires section.field")?;
        anyhow::ensure!(!field.contains('.'), "--set supports endpoint fields only");
        let parsed: toml::Table = toml::from_str(&format!("value={value}"))?;
        let section = overlay
            .entry(section.to_owned())
            .or_insert_with(|| toml::Value::Table(Default::default()));
        section
            .as_table_mut()
            .unwrap()
            .insert(field.to_owned(), parsed["value"].clone());
    }
    let mut paths = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(PathBuf::from(home).join(".cerul/config.toml"));
    }
    paths.push(std::env::current_dir()?.join("cerul.toml"));
    let environment: BTreeMap<_, _> = std::env::vars()
        .filter(|(key, _)| key.starts_with("CERUL_"))
        .collect();
    Config::resolve(&paths, &environment, toml::Value::Table(overlay))
}
async fn execute(cli: &Cli, cancel: CancellationToken) -> Result<(Value, u8)> {
    let workspace = cli
        .workspace
        .clone()
        .map(Ok)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|p| p.join(".cerul"))
                .context("set --workspace or CERUL_WORKSPACE")
        })
        .map_err(|e| category(2, e))?;
    match &cli.command {
        Some(Command::Annotate(args)) => {
            if args.grounding.is_some() || args.world.is_some() {
                return Err(ProviderError {
                    kind: Failure::Unsupported,
                    message: "grounding and world annotations require M2 capabilities".into(),
                }
                .into());
            }
            anyhow::ensure!(
                args.semantic.is_some(),
                CliError(
                    2,
                    "select --semantic and optional comma-separated items".into()
                )
            );
            let config = config(cli).map_err(|e| category(2, e))?;
            let json = cli.json;
            let quiet = cli.quiet;
            let seen = Mutex::new(BTreeSet::new());
            let notice: cerul::providers::RequestNotice = Arc::new(move |endpoint| {
                if seen.lock().unwrap().insert(endpoint.base_url.clone()) {
                    event(
                        json,
                        false,
                        Event::Log {
                            level: "info".into(),
                            msg: format!(
                                "Sending model requests, including media when needed, to {}",
                                endpoint.base_url
                            ),
                        },
                    );
                }
            });
            let items = match args.semantic.as_deref() {
                Some("default") => Vec::new(),
                Some(value) => value.split(',').map(str::to_owned).collect(),
                None => unreachable!(),
            };
            let options = cerul::annotate::pipeline::Options {
                items,
                write_lerobot: args.write_lerobot,
                out: args.out.clone(),
                streams: args.streams.clone(),
                only: args.only.clone(),
                ontology: args.ontology.clone(),
                window_us: args.window,
                fps: args.fps,
                recompute: cli.recompute,
                dry_run: cli.dry_run,
                jobs: args.jobs,
                rpm: args.rpm,
                request_notice: (!cli.yes).then_some(notice),
            };
            options.validate().map_err(|e| category(2, e))?;
            cerul::media::check_dependencies().map_err(|e| category(3, e))?;
            let report = cerul::annotate::pipeline::run(
                &args.paths,
                &workspace,
                &config,
                &options,
                cancel,
                &mut |value| event(json, quiet, value),
            )
            .await?;
            let code = if report.partial { 6 } else { 0 };
            Ok((serde_json::to_value(report)?, code))
        }
        Some(Command::Search(args)) => {
            let config = config(cli).map_err(|e| category(2, e))?;
            let json = cli.json;
            let seen = Mutex::new(BTreeSet::new());
            let notice: cerul::providers::RequestNotice = Arc::new(move |endpoint| {
                if seen.lock().unwrap().insert(endpoint.base_url.clone()) {
                    event(
                        json,
                        false,
                        Event::Log {
                            level: "info".into(),
                            msg: format!(
                                "Sending model requests, including media when needed, to {}",
                                endpoint.base_url
                            ),
                        },
                    );
                }
            });
            let options = cerul::search::Options {
                query: args.query.clone(),
                image: args.image.clone(),
                text: args.text,
                filters: args.filters.clone(),
                within: args.within.clone(),
                limit: args.limit,
                threshold: args.threshold,
                count: args.count,
                save: args.save.clone(),
                pad_us: args.pad,
                dry_run: cli.dry_run,
                request_notice: (!cli.yes).then_some(notice),
            };
            options.validate().map_err(|e| category(2, e))?;
            Ok((
                serde_json::to_value(
                    cerul::search::run(&workspace, &config, &options, cancel).await?,
                )?,
                0,
            ))
        }
        Some(Command::Clean(args)) => {
            let options = cerul::clean::Options {
                index: args.index.clone(),
                all_indexes: args.all_indexes,
                cache: args.cache,
                compact: args.compact,
                sidecars: args.sidecars.clone(),
                yes: cli.yes,
                dry_run: cli.dry_run,
            };
            cerul::clean::plan(&workspace, &options).map_err(|e| category(2, e))?;
            Ok((
                serde_json::to_value(
                    cerul::clean::run_with_cancellation(&workspace, &options, cancel).await?,
                )?,
                0,
            ))
        }
        Some(Command::Status { path, providers }) => {
            let mut status = cerul::status::inspect(&workspace, path.as_deref())?;
            let mut partial = false;
            if *providers && !cli.dry_run {
                let config = config(cli).map_err(|e| category(2, e))?;
                let json = cli.json;
                let seen = Mutex::new(BTreeSet::new());
                let notice: cerul::providers::RequestNotice = Arc::new(move |endpoint| {
                    if seen.lock().unwrap().insert(endpoint.base_url.clone()) {
                        event(
                            json,
                            false,
                            Event::Log {
                                level: "info".into(),
                                msg: format!(
                                    "Checking endpoint capabilities with small test inputs: {}",
                                    endpoint.base_url
                                ),
                            },
                        );
                    }
                });
                partial = cerul::status::check_providers(
                    &mut status,
                    &config,
                    cancel,
                    (!cli.yes).then_some(notice),
                    cli.recompute,
                )
                .await?;
            }
            Ok((serde_json::to_value(status)?, if partial { 6 } else { 0 }))
        }
        None => Ok((
            serde_json::to_value(cerul::status::inspect(&workspace, None)?)?,
            0,
        )),
        Some(Command::Index(args)) => {
            let config = config(cli).map_err(|e| category(2, e))?;
            anyhow::ensure!(
                args.jobs > 0 && args.rpm != Some(0),
                CliError(2, "jobs and RPM must be positive".into())
            );
            cerul::media::chunks(1, args.chunk, args.overlap).map_err(|e| category(2, e))?;
            anyhow::ensure!(
                args.chunk <= 32_000_000,
                CliError(2, "chunk must be at most 32s".into())
            );
            cerul::media::check_dependencies().map_err(|e| category(3, e))?;
            let json = cli.json;
            let quiet = cli.quiet;
            let seen = Mutex::new(BTreeSet::new());
            let notice: cerul::providers::RequestNotice = Arc::new(move |endpoint| {
                if seen.lock().unwrap().insert(endpoint.base_url.clone()) {
                    event(
                        json,
                        false,
                        Event::Log {
                            level: "info".into(),
                            msg: format!(
                                "Sending model requests, including media when needed, to {}",
                                endpoint.base_url
                            ),
                        },
                    );
                }
            });
            let report = pipeline::run(
                &args.paths,
                &workspace,
                &config,
                &pipeline::Options {
                    embedding: embed::Options {
                        chunk_us: args.chunk,
                        overlap_us: args.overlap,
                        skip_still: args.skip_still,
                        recompute: cli.recompute,
                    },
                    no_audio: args.no_audio,
                    no_ocr: args.no_ocr,
                    streams: args.streams.clone(),
                    only: args.only.clone(),
                    jobs: args.jobs,
                    rpm: args.rpm,
                    sidecar_dir: args.sidecar_dir.clone(),
                    dry_run: cli.dry_run,
                    request_notice: (!cli.yes).then_some(notice),
                },
                cancel,
                &mut |value| event(json, quiet, value),
            )
            .await?;
            let code = if report.partial { 6 } else { 0 };
            Ok((serde_json::to_value(report)?, code))
        }
    }
}
fn output(value: &Value, json: bool, quiet: bool) -> io::Result<()> {
    if quiet && !json {
        return Ok(());
    }
    let mut out = io::stdout().lock();
    if json {
        serde_json::to_writer(&mut out, value).map_err(io::Error::other)?;
    } else {
        serde_json::to_writer_pretty(&mut out, value).map_err(io::Error::other)?;
    }
    writeln!(out)
}
#[tokio::main]
async fn main() -> std::process::ExitCode {
    let arguments: Vec<_> = std::env::args_os().collect();
    let json = arguments.iter().any(|arg| arg == "--json");
    let cli = match Cli::try_parse_from(arguments) {
        Ok(cli) => cli,
        Err(error) => {
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                let _ = error.print();
                return std::process::ExitCode::SUCCESS;
            }
            if json {
                let _ = output(
                    &json!({"error":{"code":"invalid_arguments","message":error.to_string()}}),
                    true,
                    false,
                );
            } else {
                let _ = error.print();
            }
            return std::process::ExitCode::from(2);
        }
    };
    let cancel = CancellationToken::new();
    let signal = cancel.clone();
    let listener = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            signal.cancel();
        }
    });
    let result = cerul::media::with_cancellation(cancel.clone(), async {
        let resolver = credentials::resolver(&cli, cancel.clone());
        cerul::providers::with_credential_resolver(resolver, execute(&cli, cancel.clone())).await
    })
    .await;
    listener.abort();
    let (value, code) = match result {
        Ok(result) if !cancel.is_cancelled() => result,
        Ok(_) => (
            json!({"error":{"code":"cancelled","message":"operation cancelled"}}),
            5,
        ),
        Err(error) => {
            let code = if cancel.is_cancelled() {
                5
            } else {
                exit_code(&error)
            };
            let name = match code {
                2 => "invalid_configuration",
                3 => "missing_capability",
                5 => "cancelled",
                _ => "execution_failed",
            };
            (
                json!({"error":{"code":name,"message":format!("{error:#}")}}),
                code,
            )
        }
    };
    match output(&value, cli.json, cli.quiet && code == 0) {
        Ok(()) => std::process::ExitCode::from(code),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => {
            std::process::ExitCode::from(code)
        }
        Err(_) => std::process::ExitCode::from(4),
    }
}
