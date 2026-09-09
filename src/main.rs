mod credentials;
mod render;
use anyhow::{Context, Result};
use cerul::{
    config::Config,
    events::Event,
    index::{discover, embed, pipeline},
    providers::{Failure, ProviderError},
};
use clap::{Args, CommandFactory, Parser, Subcommand};
use render::{Mode, Palette};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
};
use tokio_util::sync::CancellationToken;

const ADVANCED: &str = "Advanced";
const GLOBAL: &str = "Global";
const EXAMPLES: &str = "\
Examples:
  cerul index ./demo.mp4                 index a video (screen text, speech, search)
  cerul search \"a person holding a cup\"  find moments by description
  cerul search --text \"ERROR 500\"        exact words on screen or in speech
  cerul search \"...\" --save ./clips      export matching clips as MP4
  cerul open 2                           play the second result at its moment
  cerul status                           what is indexed and which models are used

Speech and semantic search use Gemini; run `cerul auth set` once to save a key.
Add --json to any command for machine-readable output.
Shell completion: cerul completions <shell>.";

#[derive(Parser)]
#[command(
    name = "cerul",
    version,
    about = "Search your videos with words",
    long_about = "Search your videos with words.\n\nCerul indexes screen text, speech, and visual content locally, then finds moments from a description and exports them as clips.",
    after_help = EXAMPLES,
    disable_help_subcommand = true
)]
struct Cli {
    /// Machine-readable output: final JSON on stdout, NDJSON events on stderr.
    #[arg(long, global = true, help_heading = GLOBAL)]
    json: bool,
    /// Where indexes and caches live (default ~/.cerul).
    #[arg(long, env = "CERUL_WORKSPACE", global = true, value_name = "DIR", help_heading = GLOBAL)]
    workspace: Option<PathBuf>,
    /// Show what would happen without writing anything or calling models.
    #[arg(long, global = true, help_heading = GLOBAL)]
    dry_run: bool,
    /// Skip confirmations and never prompt; fail instead of asking for a key.
    #[arg(short = 'y', long, global = true, help_heading = GLOBAL)]
    yes: bool,
    /// Only print errors.
    #[arg(short = 'q', long, global = true, help_heading = GLOBAL)]
    quiet: bool,
    /// Redo work even when a valid result already exists.
    #[arg(long, global = true, help_heading = ADVANCED)]
    recompute: bool,
    /// Override a configuration field, for example embedding.dims=1536.
    #[arg(
        long = "set",
        global = true,
        value_name = "KEY=TOML_VALUE",
        help_heading = ADVANCED
    )]
    overrides: Vec<String>,
    /// More diagnostic output (repeatable).
    #[arg(short='v', action=clap::ArgAction::Count, global=true, help_heading = ADVANCED)]
    verbose: u8,
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand)]
enum Command {
    /// Index videos so they can be searched (screen text, speech, visual search).
    Index(IndexArgs),
    /// Find moments by description, exact words, or a reference image.
    Search(SearchArgs),
    /// Show indexed videos, model configuration, and storage.
    Status {
        /// Only report videos under this path.
        path: Option<PathBuf>,
        /// Verify configured model endpoints with small test requests (cached for seven days).
        #[arg(long)]
        providers: bool,
    },
    /// Open a result from the last search in a video player, at its moment.
    Open {
        /// Result number shown by the last search.
        #[arg(default_value_t = 1, value_name = "N")]
        number: usize,
    },
    /// Manage the saved Gemini API key.
    Auth(AuthArgs),
    /// Generate semantic annotations (tasks, events, states) for videos.
    Annotate(AnnotateArgs),
    /// Remove indexed videos, or free the disk they and their caches use.
    Remove(RemoveArgs),
    /// Print a shell completion script, for example: cerul completions zsh.
    ///
    /// Hidden from the command list: it is run once when setting up a shell, and
    /// every day after that it would only be a line to skip past.
    #[command(hide = true)]
    Completions {
        /// Shell to generate for.
        shell: clap_complete::Shell,
    },
}
#[derive(Args)]
struct AuthArgs {
    #[command(subcommand)]
    action: Option<AuthAction>,
}
#[derive(Subcommand)]
enum AuthAction {
    /// Enter and verify a Gemini API key, replacing any saved one.
    Set,
    /// Delete the saved Gemini API key.
    Remove,
}
#[derive(Args)]
struct AnnotateArgs {
    /// Videos, directories, or LeRobot datasets.
    #[arg(required = true)]
    paths: Vec<PathBuf>,
    /// Semantic items to generate, comma-separated (default set when no value is given).
    #[arg(long,num_args=0..=1,default_missing_value="default", value_name = "ITEMS")]
    semantic: Option<String>,
    /// Write subtask annotations back into the LeRobot dataset.
    #[arg(long)]
    write_lerobot: bool,
    /// Directory for exported annotation files.
    #[arg(long, value_name = "DIR")]
    out: Option<PathBuf>,
    /// Custom ontology file.
    #[arg(long, value_name = "FILE", help_heading = ADVANCED)]
    ontology: Option<PathBuf>,
    /// Model window length, for example 30s.
    #[arg(long,default_value="30s",value_parser=duration, help_heading = ADVANCED)]
    window: i64,
    /// Frames per second sampled for the model.
    #[arg(long, default_value_t = 2., help_heading = ADVANCED)]
    fps: f64,
    /// Parallel model requests.
    #[arg(long, default_value_t = 4, help_heading = ADVANCED)]
    jobs: usize,
    /// Cap on model requests per minute.
    #[arg(long, help_heading = ADVANCED)]
    rpm: Option<u32>,
    /// Streams to annotate, comma-separated.
    #[arg(long, default_value = "primary", help_heading = ADVANCED)]
    streams: String,
    /// Only these episodes (ids or local indexes), comma-separated.
    #[arg(long, help_heading = ADVANCED)]
    only: Option<String>,
    #[arg(long,num_args=0..=1,default_missing_value="default", hide = true)]
    grounding: Option<String>,
    #[arg(long,num_args=0..=1,default_missing_value="default", hide = true)]
    world: Option<String>,
}
#[derive(Args)]
struct SearchArgs {
    /// What to look for, in plain words, wrapped in quotes.
    query: Option<String>,
    #[arg(hide = true, trailing_var_arg = true, num_args = 0..)]
    extra_words: Vec<String>,
    /// Search with a reference image instead of words.
    #[arg(long, conflicts_with = "query", value_name = "FILE")]
    image: Option<PathBuf>,
    /// Match the exact words in screen text or speech instead of by meaning.
    #[arg(long)]
    text: bool,
    /// Save each matching moment as an MP4 clip in this directory.
    #[arg(long, value_name = "DIR")]
    save: Option<PathBuf>,
    /// Show a still frame for each result (default when the terminal supports images).
    #[arg(long, conflicts_with = "no_preview")]
    preview: bool,
    /// Never show still frames.
    #[arg(long)]
    no_preview: bool,
    /// Maximum number of results.
    #[arg(long, default_value_t = 10, value_name = "N")]
    limit: usize,
    /// Only search videos under this path.
    #[arg(long = "in", value_name = "PATH")]
    within: Option<PathBuf>,
    /// Seconds of context added before and after each saved clip.
    #[arg(long,default_value="2s",value_parser=duration, value_name = "DURATION")]
    pad: i64,
    /// Restrict by annotation field, for example semantic.event.verb=pour.
    #[arg(long = "filter", value_name = "KEY=VALUE", help_heading = ADVANCED)]
    filters: Vec<String>,
    /// Minimum similarity score for semantic matches.
    #[arg(long, help_heading = ADVANCED)]
    threshold: Option<f32>,
    /// Count matching intervals instead of listing them.
    #[arg(long, help_heading = ADVANCED)]
    count: bool,
}
#[derive(Args)]
struct RemoveArgs {
    /// Videos or directories to forget. Their files are left where they are.
    paths: Vec<PathBuf>,
    /// Free cached previews, query vectors, and media proxies; all regenerated.
    #[arg(long)]
    cache: bool,
    /// Free every search index; each rebuilds from sidecars with no model calls.
    #[arg(long, conflicts_with = "compact")]
    all_indexes: bool,
    /// Free one search index by space id; rebuilt from sidecars on next use.
    #[arg(long, conflicts_with_all=["all_indexes", "compact"], value_name = "SPACE", help_heading = ADVANCED)]
    index: Option<String>,
    /// Reclaim space inside search indexes without dropping them.
    #[arg(long, help_heading = ADVANCED)]
    compact: bool,
}
#[derive(Args)]
struct IndexArgs {
    /// Videos, directories, or LeRobot datasets.
    #[arg(required = true)]
    paths: Vec<PathBuf>,
    /// Skip speech transcription.
    #[arg(long)]
    no_audio: bool,
    /// Skip screen text recognition.
    #[arg(long)]
    no_ocr: bool,
    /// Length of each searchable window, for example 30s.
    #[arg(long, default_value="30s", value_parser=duration, value_name = "DURATION", help_heading = ADVANCED)]
    chunk: i64,
    /// Overlap between windows.
    #[arg(long, default_value="5s", value_parser=duration, value_name = "DURATION", help_heading = ADVANCED)]
    overlap: i64,
    /// Skip windows where the picture does not change.
    #[arg(long, help_heading = ADVANCED)]
    skip_still: bool,
    /// Streams to index, comma-separated.
    #[arg(long, default_value = "primary", help_heading = ADVANCED)]
    streams: String,
    /// Only these episodes (ids or local indexes), comma-separated.
    #[arg(long, help_heading = ADVANCED)]
    only: Option<String>,
    /// Parallel model requests.
    #[arg(long, default_value_t = 4, help_heading = ADVANCED)]
    jobs: usize,
    /// Cap on model requests per minute.
    #[arg(long, help_heading = ADVANCED)]
    rpm: Option<u32>,
    /// Store sidecars here instead of beside the videos.
    #[arg(long, value_name = "DIR", help_heading = ADVANCED)]
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

/// Everything a command can produce. JSON mode serializes it; human mode renders it.
enum Outcome {
    Home(cerul::status::Status, render::ModelSummary),
    Status {
        status: cerul::status::Status,
        models: render::ModelSummary,
        probed: bool,
    },
    Auth(render::KeyState, Option<&'static str>),
    Remove(
        cerul::clean::Report,
        BTreeMap<String, PathBuf>,
        Vec<PathBuf>,
    ),
    Open {
        media: PathBuf,
        start_us: i64,
        player: String,
        seeks: bool,
        opened: bool,
    },
    Index(pipeline::Report, BTreeMap<String, PathBuf>),
    Search(cerul::search::Report, render::SearchContext),
    Annotate(cerul::annotate::pipeline::Report, BTreeMap<String, PathBuf>),
}
impl Outcome {
    fn json(&self) -> Result<Value> {
        Ok(match self {
            Outcome::Home(status, _) | Outcome::Status { status, .. } => {
                serde_json::to_value(status)?
            }
            Outcome::Auth(key, action) => {
                let mut value = serde_json::to_value(key)?;
                if let Some(action) = action {
                    value["action"] = json!(action);
                }
                value
            }
            Outcome::Open {
                media,
                start_us,
                player,
                seeks,
                opened,
            } => {
                json!({"media": media, "start_us": start_us, "player": player, "seeks": seeks, "opened": opened})
            }
            Outcome::Index(report, _) => serde_json::to_value(report)?,
            Outcome::Search(report, _) => serde_json::to_value(report)?,
            Outcome::Remove(report, _, _) => serde_json::to_value(report)?,
            Outcome::Annotate(report, _) => serde_json::to_value(report)?,
        })
    }
    fn render(&self, out: &mut dyn Write, palette: &Palette) -> io::Result<()> {
        match self {
            Outcome::Home(status, models) => render::home(out, palette, status, models),
            Outcome::Status {
                status,
                models,
                probed,
            } => render::status(out, palette, status, models, *probed),
            Outcome::Auth(key, action) => {
                match action {
                    Some("set") => {
                        writeln!(out, "{} Gemini key saved.", palette.ok("✓"))?;
                        writeln!(out)?;
                        writeln!(
                            out,
                            "Next: {}   {}",
                            palette.cmd("cerul index ./video.mp4"),
                            palette.dim("index a video, then search it")
                        )?;
                    }
                    Some("removed") => {
                        writeln!(out, "{} Saved Gemini key removed.", palette.ok("✓"))?
                    }
                    Some("absent") => writeln!(out, "No saved Gemini key to remove.")?,
                    _ => {}
                }
                if action.is_none() || matches!(action, Some("absent")) {
                    render::auth(out, palette, key)
                } else {
                    Ok(())
                }
            }
            Outcome::Open {
                media,
                start_us,
                player,
                seeks,
                opened,
            } => render::open(out, palette, media, *start_us, player, *seeks, *opened),
            Outcome::Index(report, names) => render::index(out, palette, report, names),
            Outcome::Search(report, context) => render::search(out, palette, report, context),
            Outcome::Remove(report, names, unknown) => {
                render::remove(out, palette, report, names, unknown)
            }
            Outcome::Annotate(report, names) => render::annotate(out, palette, report, names),
        }
    }
}

/// Shared event sink: NDJSON on stderr for `--json`, rendered progress otherwise.
#[derive(Clone)]
struct Sink {
    mode: Mode,
    progress: Arc<Mutex<render::Progress>>,
}
impl Sink {
    fn emit(&self, value: Event) {
        match self.mode {
            Mode::Json => {
                let mut out = io::stderr().lock();
                let _ = serde_json::to_writer(&mut out, &value);
                let _ = writeln!(out);
            }
            Mode::Human => self.progress.lock().unwrap().handle(value),
        }
    }
    fn finish(&self) {
        if self.mode == Mode::Human {
            self.progress.lock().unwrap().finish();
        }
    }
    fn spinner(&self, message: &str) {
        if self.mode == Mode::Human {
            self.progress.lock().unwrap().start_spinner(message);
        }
    }
    /// One privacy notice per endpoint before media leaves the machine.
    fn notice(&self, verb: &'static str) -> cerul::providers::RequestNotice {
        let sink = self.clone();
        let seen = Mutex::new(BTreeSet::new());
        Arc::new(move |endpoint| {
            if seen.lock().unwrap().insert(endpoint.base_url.clone()) {
                sink.emit(Event::Log {
                    level: "info".into(),
                    msg: format!("{verb} {}", endpoint.base_url),
                });
            }
        })
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
fn models(config: &Config) -> render::ModelSummary {
    render::ModelSummary {
        embedding: config.embedding.model.clone(),
        embedding_dims: config.embedding.dims,
        vision: config.vision.model.clone(),
        transcription: config.transcription.model.clone(),
        key: credentials::state(&config.embedding),
    }
}
const QUOTES: &[char] = &['"', '\'', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}'];
/// Drops stray straight or curly quotes that end up inside the argument.
fn query_words(query: &str) -> Option<String> {
    let cleaned = query.trim().trim_matches(QUOTES).trim();
    (!cleaned.is_empty()).then(|| cleaned.to_owned())
}
/// The last search, so `open N` can act on the numbers a person just read.
#[derive(Serialize, Deserialize)]
struct LastSearch {
    query: Option<String>,
    hits: Vec<LastHit>,
}
#[derive(Serialize, Deserialize)]
struct LastHit {
    media: Option<PathBuf>,
    start_us: i64,
    end_us: i64,
}
fn last_search_path(workspace: &Path) -> PathBuf {
    workspace.join("cache").join("last-search.json")
}

/// How a moment will be opened, and whether the player honours the timestamp.
struct Launch {
    program: PathBuf,
    arguments: Vec<String>,
    name: String,
    seeks: bool,
}
fn on_path(program: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|directory| directory.join(program))
        .find(|candidate| candidate.is_file())
}
/// Prefers players that can start at a timestamp, and says so when none can.
fn launch(media: &Path, start_us: i64) -> Launch {
    let seconds = format!("{:.3}", start_us.max(0) as f64 / 1_000_000.);
    let media = media.to_string_lossy().into_owned();
    let iina = PathBuf::from("/Applications/IINA.app/Contents/MacOS/iina-cli");
    let candidates: [(Option<PathBuf>, &str, Vec<String>); 4] = [
        (
            on_path("mpv"),
            "mpv",
            vec![format!("--start={seconds}"), media.clone()],
        ),
        (
            iina.is_file().then_some(iina),
            "IINA",
            vec![format!("--mpv-start={seconds}"), media.clone()],
        ),
        (
            on_path("vlc"),
            "VLC",
            vec![format!("--start-time={seconds}"), media.clone()],
        ),
        (
            on_path("ffplay"),
            "ffplay",
            vec![
                "-loglevel".into(),
                "error".into(),
                "-ss".into(),
                seconds.clone(),
                "-autoexit".into(),
                media.clone(),
            ],
        ),
    ];
    for (program, name, arguments) in candidates {
        if let Some(program) = program {
            return Launch {
                program,
                arguments,
                name: name.into(),
                seeks: true,
            };
        }
    }
    let fallback = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    Launch {
        program: on_path(fallback).unwrap_or_else(|| PathBuf::from(fallback)),
        arguments: vec![media],
        name: "the system player".into(),
        seeks: false,
    }
}

/// Distinguishes "this video was never indexed" from a real planning failure.
fn is_unindexed(error: &anyhow::Error) -> bool {
    format!("{error:#}").contains("no registered sidecars match selection")
}

/// Asks before an irreversible removal. A non-terminal caller must pass --yes,
/// so a script never destroys an index because nobody was there to answer.
fn confirm(action: &str, question: &str) -> Result<bool> {
    use std::io::{BufRead, IsTerminal};
    anyhow::ensure!(
        io::stdin().is_terminal() && io::stderr().is_terminal(),
        CliError(2, format!("{action} needs confirmation"))
    );
    eprint!("{question} [y/N] ");
    io::stderr().flush()?;
    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer)?;
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

/// Rejects unreadable inputs before any plan, model request, or progress output.
fn readable(paths: &[PathBuf]) -> Result<()> {
    for path in paths {
        anyhow::ensure!(
            path.exists(),
            CliError(2, format!("no such file or directory: {}", path.display()))
        );
    }
    Ok(())
}
fn names(workspace: &Path) -> BTreeMap<String, PathBuf> {
    discover::read_registry(workspace)
        .map(|entries| {
            entries
                .into_iter()
                .map(|entry| (entry.episode_id, entry.media))
                .collect()
        })
        .unwrap_or_default()
}
const MEDIA_NOTICE: &str = "Using Gemini (media may be sent, usage billed to your key):";

async fn execute(cli: &Cli, sink: &Sink, cancel: CancellationToken) -> Result<(Outcome, u8)> {
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
        None => {
            let status = cerul::status::inspect(&workspace, None)?;
            let config = config(cli).map_err(|e| category(2, e))?;
            Ok((Outcome::Home(status, models(&config)), 0))
        }
        Some(Command::Status { path, providers }) => {
            let mut status = cerul::status::inspect(&workspace, path.as_deref())?;
            let config = config(cli).map_err(|e| category(2, e))?;
            let mut partial = false;
            if *providers && !cli.dry_run {
                partial = cerul::status::check_providers(
                    &mut status,
                    &config,
                    cancel,
                    (!cli.yes).then(|| {
                        sink.notice("Checking endpoint capabilities with small test inputs:")
                    }),
                    cli.recompute,
                )
                .await?;
            }
            sink.finish();
            Ok((
                Outcome::Status {
                    status,
                    models: models(&config),
                    probed: *providers && !cli.dry_run,
                },
                if partial { 6 } else { 0 },
            ))
        }
        Some(Command::Open { number }) => {
            let path = last_search_path(&workspace);
            let last: LastSearch = fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .ok_or_else(|| {
                    CliError(2, "no recent search to open; run cerul search first".into())
                })?;
            anyhow::ensure!(
                (1..=last.hits.len()).contains(number),
                CliError(
                    2,
                    format!(
                        "the last search returned {} result{}",
                        last.hits.len(),
                        if last.hits.len() == 1 { "" } else { "s" }
                    )
                )
            );
            let hit = &last.hits[number - 1];
            let media = hit
                .media
                .clone()
                .context("that result has no video file")
                .map_err(|e| category(2, e))?;
            anyhow::ensure!(
                media.is_file(),
                CliError(2, format!("the video has moved: {}", media.display()))
            );
            let plan = launch(&media, hit.start_us);
            if !cli.dry_run {
                std::process::Command::new(&plan.program)
                    .args(&plan.arguments)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .with_context(|| format!("could not start {}", plan.name))
                    .map_err(|e| category(4, e))?;
            }
            Ok((
                Outcome::Open {
                    media,
                    start_us: hit.start_us,
                    player: plan.name,
                    seeks: plan.seeks,
                    opened: !cli.dry_run,
                },
                0,
            ))
        }
        Some(Command::Auth(args)) => {
            let config = config(cli).map_err(|e| category(2, e))?;
            let endpoint = &config.embedding;
            let action = match args.action {
                None => None,
                Some(AuthAction::Set) => {
                    anyhow::ensure!(
                        !cli.json && !cli.yes && !cli.dry_run,
                        CliError(
                            2,
                            format!(
                                "cerul auth set is interactive; export {} for scripts",
                                endpoint.api_key_env
                            )
                        )
                    );
                    credentials::set(endpoint, cancel)
                        .await
                        .map_err(|e| category(2, e))?;
                    Some("set")
                }
                Some(AuthAction::Remove) => {
                    if cli.dry_run {
                        None
                    } else if credentials::remove(endpoint).map_err(|e| category(2, e))? {
                        Some("removed")
                    } else {
                        Some("absent")
                    }
                }
            };
            Ok((Outcome::Auth(credentials::state(endpoint), action), 0))
        }
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
                request_notice: (!cli.yes).then(|| sink.notice(MEDIA_NOTICE)),
            };
            options.validate().map_err(|e| category(2, e))?;
            readable(&args.paths)?;
            cerul::media::check_dependencies().map_err(|e| category(3, e))?;
            let events = sink.clone();
            let report = cerul::annotate::pipeline::run(
                &args.paths,
                &workspace,
                &config,
                &options,
                cancel,
                &mut |value| events.emit(value),
            )
            .await?;
            sink.finish();
            let code = if report.partial { 6 } else { 0 };
            Ok((Outcome::Annotate(report, names(&workspace)), code))
        }
        Some(Command::Search(args)) => {
            let config = config(cli).map_err(|e| category(2, e))?;
            anyhow::ensure!(
                args.extra_words.is_empty(),
                CliError(
                    2,
                    format!(
                        "the query must be one quoted argument, for example: cerul search \"{} {}\"",
                        args.query.clone().unwrap_or_default().trim_matches(QUOTES),
                        args.extra_words.join(" ").trim_matches(QUOTES)
                    )
                )
            );
            let query = args.query.as_deref().and_then(query_words);
            let terminal = render::Terminal::detect(sink.mode);
            let preview = !args.no_preview
                && !cli.dry_run
                && (args.preview || (terminal.images.is_some() && !cli.quiet));
            let options = cerul::search::Options {
                query: query.clone(),
                image: args.image.clone(),
                text: args.text,
                filters: args.filters.clone(),
                within: args.within.clone(),
                limit: args.limit,
                threshold: args.threshold,
                count: args.count,
                save: args.save.clone(),
                preview,
                pad_us: args.pad,
                dry_run: cli.dry_run,
                request_notice: (!cli.yes).then(|| sink.notice(MEDIA_NOTICE)),
            };
            options.validate().map_err(|e| category(2, e))?;
            sink.spinner("Searching…");
            let report = cerul::search::run(&workspace, &config, &options, cancel).await;
            sink.finish();
            let report = report?;
            // Remembering the numbers a person just read is what makes `open N` work.
            if !cli.dry_run && !report.hits.is_empty() {
                let last = LastSearch {
                    query: query.clone(),
                    hits: report
                        .hits
                        .iter()
                        .map(|hit| LastHit {
                            media: hit.media.clone(),
                            start_us: hit.start_us,
                            end_us: hit.end_us,
                        })
                        .collect(),
                };
                let path = last_search_path(&workspace);
                if fs::create_dir_all(path.parent().expect("cache path has a parent")).is_ok() {
                    let _ = cerul::storage::write_json(&path, &last);
                }
            }
            Ok((
                Outcome::Search(
                    report,
                    render::SearchContext {
                        query,
                        image: args.image.clone(),
                        text: args.text,
                        saved_to: args.save.clone(),
                        terminal,
                        player: render::Player::detect(),
                    },
                ),
                0,
            ))
        }
        Some(Command::Completions { .. }) => {
            unreachable!("completions are printed before the runtime starts")
        }
        Some(Command::Remove(args)) => {
            anyhow::ensure!(
                !args.paths.is_empty()
                    || args.cache
                    || args.all_indexes
                    || args.index.is_some()
                    || args.compact,
                CliError(
                    2,
                    "name the videos to forget, or pass --cache or --all-indexes".into()
                )
            );
            readable(&args.paths)?;
            let names = names(&workspace);
            let wants_disposable =
                args.cache || args.all_indexes || args.index.is_some() || args.compact;
            // Plan every path first so one unknown video cannot leave a partial
            // removal behind, and so the question names what will actually go.
            let mut planned = Vec::new();
            let mut unknown = Vec::new();
            // No workspace means no registry, so nothing can be indexed yet.
            let anything_indexed = workspace.exists();
            for path in &args.paths {
                if !anything_indexed {
                    unknown.push(path.clone());
                    continue;
                }
                let options = cerul::clean::Options {
                    sidecars: Some(path.clone()),
                    yes: true,
                    dry_run: true,
                    ..Default::default()
                };
                match cerul::clean::plan(&workspace, &options) {
                    Ok(report) => planned.extend(report.items),
                    // Forgetting something already forgotten is not a failure.
                    Err(error) if is_unindexed(&error) => unknown.push(path.clone()),
                    Err(error) => return Err(category(2, error)),
                }
            }
            // Only losing a video needs a question; everything else grows back.
            if !planned.is_empty() && !cli.dry_run && !cli.yes {
                let listed = planned
                    .iter()
                    .filter_map(|item| item.episode.as_ref())
                    .filter_map(|episode| names.get(episode))
                    .map(|media| render::file_name(media))
                    .collect::<Vec<_>>();
                let subject = if listed.is_empty() {
                    format!("{} episodes", planned.len())
                } else {
                    listed.join(", ")
                };
                let single = listed.len() == 1;
                let action = format!("removing the index for {subject}");
                let question = format!(
                    "Remove the index for {subject}? The video file{} stay{} where {} {}.",
                    if single { "" } else { "s" },
                    if single { "s" } else { "" },
                    if single { "it" } else { "they" },
                    if single { "is" } else { "are" }
                );
                if !confirm(&action, &question).map_err(|e| category(2, e))? {
                    return Ok((
                        Outcome::Remove(
                            cerul::clean::Report {
                                dry_run: true,
                                items: Vec::new(),
                            },
                            names,
                            unknown,
                        ),
                        0,
                    ));
                }
            }
            let mut items = Vec::new();
            if wants_disposable {
                let options = cerul::clean::Options {
                    index: args.index.clone(),
                    all_indexes: args.all_indexes,
                    cache: args.cache,
                    compact: args.compact,
                    sidecars: None,
                    yes: true,
                    dry_run: cli.dry_run,
                };
                cerul::clean::plan(&workspace, &options).map_err(|e| category(2, e))?;
                items.extend(
                    cerul::clean::run_with_cancellation(&workspace, &options, cancel.clone())
                        .await?
                        .items,
                );
            }
            if cli.dry_run {
                items.extend(planned);
            } else {
                for path in &args.paths {
                    if unknown.contains(path) {
                        continue;
                    }
                    let options = cerul::clean::Options {
                        sidecars: Some(path.clone()),
                        yes: true,
                        dry_run: false,
                        ..Default::default()
                    };
                    items.extend(
                        cerul::clean::run_with_cancellation(&workspace, &options, cancel.clone())
                            .await?
                            .items,
                    );
                }
            }
            Ok((
                Outcome::Remove(
                    cerul::clean::Report {
                        dry_run: cli.dry_run,
                        items,
                    },
                    names,
                    unknown,
                ),
                0,
            ))
        }
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
            readable(&args.paths)?;
            cerul::media::check_dependencies().map_err(|e| category(3, e))?;
            if sink.mode == Mode::Human && !cli.quiet && !cli.dry_run {
                let palette = Palette::new(console::colors_enabled_stderr());
                let _ = render::index_plan(
                    &mut io::stderr(),
                    &palette,
                    &args.paths,
                    args.no_ocr,
                    args.no_audio,
                );
            }
            let events = sink.clone();
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
                    request_notice: (!cli.yes).then(|| sink.notice(MEDIA_NOTICE)),
                },
                cancel,
                &mut |value| events.emit(value),
            )
            .await?;
            sink.finish();
            let code = if report.partial { 6 } else { 0 };
            Ok((Outcome::Index(report, names(&workspace)), code))
        }
    }
}

fn write_json(value: &Value) -> io::Result<()> {
    let mut out = io::stdout().lock();
    serde_json::to_writer(&mut out, value).map_err(io::Error::other)?;
    writeln!(out)
}

/// A recovery hint for people; the JSON protocol carries only code and message.
fn hint(code: u8, error: &anyhow::Error, env: &str) -> Option<String> {
    let message = format!("{error:#}").to_lowercase();
    if matches!(
        error.downcast_ref::<ProviderError>().map(|e| e.kind),
        Some(Failure::MissingKey)
    ) || message.contains("api key")
    {
        return Some(format!(
            "run `cerul auth set` to save a Gemini key, or export {env}"
        ));
    }
    if message.contains("ffmpeg") || message.contains("ffprobe") {
        return Some(
            "reinstall with `curl -fsSL https://cerul.ai/install.sh | sh`, or put ffmpeg 6+ on PATH"
                .into(),
        );
    }
    if message.contains("no usable saved vectors") {
        return Some("index the videos with this model first: cerul index ./video.mp4".into());
    }
    if message.contains("no such file") || message.contains("not found") {
        return Some("check the path; run `cerul status` to see what is indexed".into());
    }
    // A command that refused for a missing selection should name a working one.
    if message.contains("needs confirmation") {
        return Some("add --yes to confirm without being asked".into());
    }
    if message.contains("select --semantic") {
        return Some("cerul annotate ./video.mp4 --semantic".into());
    }
    if message.contains("--count requires") {
        return Some("cerul search --filter 'semantic.event.verb=place' --count".into());
    }
    if message.contains("select an index, cache, compaction, or sidecar target")
        || message.contains("name the videos to forget")
    {
        return Some("cerul remove ./video.mp4, or cerul remove --cache".into());
    }
    if message.contains("query, image, or filter is required") {
        return Some("cerul search \"describe the moment\"".into());
    }
    match code {
        2 => Some("see `cerul --help` for usage and `cerul status` for configuration".into()),
        3 => Some("this build cannot do that yet; see `cerul status --providers`".into()),
        _ => None,
    }
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
                let _ = write_json(
                    &json!({"error":{"code":"invalid_arguments","message":error.to_string()}}),
                );
            } else {
                let _ = error.print();
            }
            return std::process::ExitCode::from(2);
        }
    };
    if let Some(Command::Completions { shell }) = cli.command {
        let mut command = Cli::command();
        let name = command.get_name().to_string();
        // The generator panics on any write error, so it never touches stdout
        // directly: piping the script into `head` is ordinary shell use.
        let mut script = Vec::new();
        clap_complete::generate(shell, &mut command, name, &mut script);
        return match io::stdout().write_all(&script) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(error) if error.kind() == io::ErrorKind::BrokenPipe => {
                std::process::ExitCode::SUCCESS
            }
            Err(_) => std::process::ExitCode::from(4),
        };
    }
    let mode = if cli.json { Mode::Json } else { Mode::Human };
    let palette = Palette::new(mode == Mode::Human && console::colors_enabled());
    let workspace_hint = cli
        .workspace
        .clone()
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cerul")))
        .unwrap_or_default();
    let sink = Sink {
        mode,
        progress: Arc::new(Mutex::new(render::Progress::new(
            &workspace_hint,
            cli.quiet,
            Palette::new(mode == Mode::Human && console::colors_enabled_stderr()),
        ))),
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
        cerul::providers::with_credential_resolver(resolver, execute(&cli, &sink, cancel.clone()))
            .await
    })
    .await;
    listener.abort();
    sink.finish();
    let failure = |code: u8, message: String| {
        let name = match code {
            2 => "invalid_configuration",
            3 => "missing_capability",
            5 => "cancelled",
            _ => "execution_failed",
        };
        (name, message)
    };
    let (outcome, code, error) = match result {
        Ok((outcome, code)) if !cancel.is_cancelled() => (Some(outcome), code, None),
        Ok(_) => (None, 5, Some(failure(5, "operation cancelled".into()))),
        Err(error) => {
            let code = if cancel.is_cancelled() {
                5
            } else {
                exit_code(&error)
            };
            let env = config(&cli)
                .map(|c| c.embedding.api_key_env)
                .unwrap_or_else(|_| "GEMINI_API_KEY".into());
            let hint = if code == 5 {
                None
            } else {
                hint(code, &error, &env)
            };
            let mut named = failure(code, format!("{error:#}"));
            if mode == Mode::Human {
                eprintln!(
                    "{}",
                    render::error(&palette, code, &named.1, hint.as_deref())
                );
            }
            named.1 = format!("{error:#}");
            (None, code, Some(named))
        }
    };
    let written = match (mode, outcome, error) {
        (Mode::Json, Some(outcome), _) => match outcome.json() {
            Ok(value) => write_json(&value),
            Err(error) => write_json(
                &json!({"error":{"code":"execution_failed","message":error.to_string()}}),
            ),
        },
        (Mode::Json, None, Some((name, message))) => {
            write_json(&json!({"error":{"code":name,"message":message}}))
        }
        (Mode::Human, Some(outcome), _) if !cli.quiet || code != 0 => {
            let mut out = io::stdout().lock();
            outcome.render(&mut out, &palette).and_then(|_| out.flush())
        }
        _ => Ok(()),
    };
    match written {
        Ok(()) => std::process::ExitCode::from(code),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => {
            std::process::ExitCode::from(code)
        }
        Err(_) => std::process::ExitCode::from(4),
    }
}
