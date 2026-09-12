//! Human-readable terminal rendering for the CLI.
//!
//! Everything here is presentation only. The library emits structured values and
//! events; this module turns them into text for people. `--json` bypasses it.
use base64::{Engine, engine::general_purpose::STANDARD};
use cerul::{
    annotate, clean,
    events::Event,
    index::{self, discover, vectors::Kind},
    search,
    status::Status,
};
use console::{Style, measure_text_width, truncate_str};
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Optional terminal features. Both stay off unless the terminal advertises
/// support, so ordinary terminals and redirected output never see escape codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Terminal {
    pub hyperlinks: bool,
    pub images: Option<Images>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Images {
    /// iTerm2 inline image protocol, also implemented by WezTerm.
    Iterm,
    /// Kitty graphics protocol, also implemented by Ghostty.
    Kitty,
}
/// A media player that can start at a timestamp, when one is installed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Player {
    /// IINA's URL scheme forwards mpv options, so a link can carry a start time.
    Iina,
    /// The system handler opens the file, always from the beginning.
    System,
}
impl Player {
    pub fn detect() -> Self {
        if cfg!(target_os = "macos") && Path::new("/Applications/IINA.app").is_dir() {
            Self::Iina
        } else {
            Self::System
        }
    }
    /// A label and URL that open the moment itself where the player allows it.
    pub fn open(&self, path: &Path, start_us: i64) -> (String, String) {
        let file = url(path.to_string_lossy().as_ref(), b"/-_.~");
        match self {
            Self::Iina => (
                format!("Open at {}", clock(start_us)),
                format!(
                    "iina://open?url={}&mpv_start={:.3}",
                    url(&format!("file://{file}"), b"-_.~"),
                    start_us.max(0) as f64 / 1_000_000.
                ),
            ),
            Self::System => ("Open video".into(), format!("file://{file}")),
        }
    }
}
/// Percent-encodes everything outside the unreserved set given in `keep`.
fn url(value: &str, keep: &[u8]) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || keep.contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}
impl Terminal {
    pub const fn none() -> Self {
        Self {
            hyperlinks: false,
            images: None,
        }
    }
    pub fn detect(mode: Mode) -> Self {
        if mode == Mode::Json || !io::stdout().is_terminal() {
            return Self::none();
        }
        let program = std::env::var("TERM_PROGRAM").unwrap_or_default();
        let term = std::env::var("TERM").unwrap_or_default();
        let kitty = term.contains("kitty")
            || std::env::var_os("KITTY_WINDOW_ID").is_some()
            || program == "ghostty";
        let iterm = program == "iTerm.app" || program == "WezTerm";
        let images = if kitty {
            Some(Images::Kitty)
        } else if iterm {
            Some(Images::Iterm)
        } else {
            None
        };
        // VTE 0.50 and newer render OSC 8; older versions print the payload.
        let vte = std::env::var("VTE_VERSION")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .is_some_and(|version| version >= 5000);
        let hyperlinks = images.is_some()
            || vte
            || matches!(program.as_str(), "vscode" | "Hyper" | "Tabby" | "rio");
        Self { hyperlinks, images }
    }
    /// OSC 8 hyperlink, or the plain label when the terminal cannot render one.
    pub fn link(&self, label: &str, target: &str) -> String {
        if !self.hyperlinks {
            return label.to_owned();
        }
        format!("\u{1b}]8;;{target}\u{7}{label}\u{1b}]8;;\u{7}")
    }
    pub fn file_link(&self, label: &str, path: &Path) -> String {
        self.link(
            label,
            &format!("file://{}", url(path.to_string_lossy().as_ref(), b"/-_.~")),
        )
    }
    /// Writes an inline image sized to fit `columns` by `rows` character cells.
    fn image(&self, out: &mut dyn Write, path: &Path, columns: u32, rows: u32) -> io::Result<bool> {
        let Some(protocol) = self.images else {
            return Ok(false);
        };
        let Ok(bytes) = std::fs::read(path) else {
            return Ok(false);
        };
        let payload = STANDARD.encode(&bytes);
        match protocol {
            Images::Iterm => write!(
                out,
                "\u{1b}]1337;File=inline=1;width={columns};height={rows};preserveAspectRatio=1;size={}:{payload}\u{7}",
                bytes.len()
            )?,
            Images::Kitty => {
                // The protocol caps each escape at 4096 base64 characters.
                let chunks: Vec<&str> = payload
                    .as_bytes()
                    .chunks(4096)
                    .map(|chunk| std::str::from_utf8(chunk).expect("base64 is ascii"))
                    .collect();
                for (index, chunk) in chunks.iter().enumerate() {
                    let more = u8::from(index + 1 < chunks.len());
                    if index == 0 {
                        write!(
                            out,
                            "\u{1b}_Ga=T,f=100,c={columns},r={rows},m={more};{chunk}\u{1b}\\"
                        )?;
                    } else {
                        write!(out, "\u{1b}_Gm={more};{chunk}\u{1b}\\")?;
                    }
                }
            }
        }
        Ok(true)
    }
}

/// How the process talks to the person or program that started it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Final JSON on stdout, NDJSON events on stderr. Stable protocol for agents.
    Json,
    /// Text for people. Colour and live progress only when attached to a terminal.
    Human,
}

/// Colour palette that degrades to plain text when colours are disabled.
#[derive(Clone, Copy)]
pub struct Palette {
    enabled: bool,
}
impl Palette {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }
    fn paint(&self, style: Style, text: &str) -> String {
        if self.enabled {
            style.apply_to(text).to_string()
        } else {
            text.to_owned()
        }
    }
    pub fn bold(&self, text: &str) -> String {
        self.paint(Style::new().bold(), text)
    }
    pub fn dim(&self, text: &str) -> String {
        self.paint(Style::new().dim(), text)
    }
    pub fn ok(&self, text: &str) -> String {
        self.paint(Style::new().green(), text)
    }
    pub fn warn(&self, text: &str) -> String {
        self.paint(Style::new().yellow(), text)
    }
    pub fn err(&self, text: &str) -> String {
        self.paint(Style::new().red().bold(), text)
    }
    pub fn cmd(&self, text: &str) -> String {
        self.paint(Style::new().cyan(), text)
    }
}

/// What the person has configured for model access.
#[derive(Clone, Debug, serde::Serialize)]
pub struct KeyState {
    pub provider: String,
    pub endpoint: String,
    pub env: String,
    pub env_set: bool,
    pub saved: bool,
    pub credentials_path: Option<PathBuf>,
}
impl KeyState {
    pub fn available(&self) -> bool {
        self.env_set || self.saved
    }
    fn summary(&self, palette: &Palette) -> String {
        if self.env_set {
            palette.ok(&format!("key from ${}", self.env))
        } else if self.saved {
            palette.ok("key saved")
        } else {
            palette.warn("key not set")
        }
    }
}

/// Models the CLI would use, resolved from configuration for display.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ModelSummary {
    pub embedding: String,
    pub embedding_dims: Option<usize>,
    pub vision: String,
    pub transcription: String,
    pub key: KeyState,
}

pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn short_name(name: &str) -> String {
    truncate_str(name, 48, "…").into_owned()
}

/// A stage estimate needs measured work; cached initial positions are not throughput.
fn remaining(done: u64, total: u64, baseline: u64, elapsed: Duration) -> String {
    let measured = done.saturating_sub(baseline);
    if done >= total && total > 0 {
        return String::new();
    }
    if measured < 2 || elapsed.as_secs() < 2 {
        return "estimating…".into();
    }
    let seconds =
        (elapsed.as_secs_f64() / measured as f64 * total.saturating_sub(done) as f64).ceil() as u64;
    if seconds < 60 {
        format!("~{seconds}s left")
    } else {
        format!("~{}m left", seconds.div_ceil(60))
    }
}

pub fn clock(us: i64) -> String {
    let total = us.max(0) / 1_000_000;
    let (hours, minutes, seconds) = (total / 3600, (total % 3600) / 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

fn station_label(station: &str) -> String {
    match station {
        "screen_text" => "Screen text".into(),
        "transcript" => "Speech".into(),
        "embed" => "Search index".into(),
        "understanding" => "Understanding".into(),
        "description" => "Descriptions".into(),
        other => {
            let name = other.strip_prefix("semantic.").unwrap_or(other);
            let mut chars = name.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => other.into(),
            }
        }
    }
}
fn station_unit(station: &str, count: u64) -> String {
    let (one, many) = match station {
        "screen_text" => ("frame", "frames"),
        "transcript" => ("segment", "segments"),
        "embed" => ("batch", "batches"),
        "understanding" => ("step", "steps"),
        _ => ("window", "windows"),
    };
    format!("{count} {}", if count == 1 { one } else { many })
}

/// Renders progress events. Live bars on a terminal, one line per finished
/// station otherwise, so redirected output never contains cursor movement.
pub struct Progress {
    palette: Palette,
    quiet: bool,
    workspace: PathBuf,
    multi: Option<MultiProgress>,
    bars: HashMap<(String, String), ProgressBar>,
    estimates: HashMap<(String, String), (Instant, u64)>,
    /// Finished station summaries per episode, in completion order.
    finished: HashMap<String, Vec<(String, String)>>,
    /// The episode whose stations are currently on screen.
    active: Option<String>,
    spinner: Option<ProgressBar>,
    names: HashMap<String, String>,
    /// Episode ids already looked up, so a refresh happens once per new video.
    resolved: std::collections::HashSet<String>,
}
impl Progress {
    pub fn new(workspace: &Path, quiet: bool, palette: Palette) -> Self {
        let animate = !quiet && io::stderr().is_terminal();
        let multi = animate.then(|| MultiProgress::with_draw_target(ProgressDrawTarget::stderr()));
        Self {
            palette,
            quiet,
            workspace: workspace.to_owned(),
            multi,
            bars: HashMap::new(),
            estimates: HashMap::new(),
            finished: HashMap::new(),
            active: None,
            spinner: None,
            names: HashMap::new(),
            resolved: std::collections::HashSet::new(),
        }
    }
    fn name(&mut self, episode: &str) -> String {
        if let Some(name) = self.names.get(episode) {
            return short_name(name);
        }
        // A video joins the registry as its turn begins, so an unknown id means
        // the cached copy predates it. Refresh once per id, never per event.
        if self.resolved.insert(episode.to_owned())
            && let Ok(entries) = discover::read_registry(&self.workspace)
        {
            for entry in entries {
                self.names
                    .insert(entry.episode_id.clone(), file_name(&entry.media));
            }
        }
        self.names
            .get(episode)
            .cloned()
            .map(|name| short_name(&name))
            .unwrap_or_else(|| short_name(episode))
    }
    /// Replaces one episode's station bars with a single line, so indexing many
    /// videos keeps the live area the size of the video being worked on.
    fn collapse(&mut self, episode: &str) {
        self.estimates.retain(|(id, _), _| id != episode);
        let Some(multi) = self.multi.clone() else {
            return;
        };
        let mut removed = false;
        self.bars.retain(|(id, _), bar| {
            if id == episode {
                multi.remove(bar);
                removed = true;
                return false;
            }
            true
        });
        let summaries = self.finished.remove(episode).unwrap_or_default();
        // The final index report already states which videos are searchable.
        if summaries.iter().all(|(station, _)| {
            matches!(
                station.as_str(),
                "screen_text" | "transcript" | "embed" | "understanding" | "description"
            )
        }) {
            return;
        }
        if !removed || summaries.is_empty() {
            return;
        }
        let name = self
            .names
            .get(episode)
            .cloned()
            .unwrap_or_else(|| episode.to_owned());
        let line = summaries
            .iter()
            .map(|(_, text)| text.clone())
            .collect::<Vec<_>>()
            .join(&self.palette.dim("  ·  "));
        let summary = format!(
            "{} {}   {line}",
            self.palette.ok("✓"),
            self.palette.bold(&name)
        );
        // Printing above live bars relies on their redraw to end the line, so the
        // last summary of a run must write its own newline instead.
        if self.bars.is_empty() {
            eprintln!("{summary}");
        } else {
            let _ = multi.println(summary);
        }
    }

    /// Marks work whose duration cannot be predicted, such as one model request.
    pub fn start_spinner(&mut self, message: &str) {
        let Some(multi) = &self.multi else { return };
        let bar = multi.add(ProgressBar::new_spinner());
        bar.set_style(
            ProgressStyle::with_template("  {spinner:.cyan} {msg} {elapsed}")
                .expect("static template"),
        );
        bar.set_message(self.palette.dim(message));
        bar.enable_steady_tick(Duration::from_millis(100));
        self.spinner = Some(bar);
    }
    pub fn stop_spinner(&mut self) {
        if let Some(bar) = self.spinner.take() {
            bar.finish_and_clear();
        }
    }
    pub fn println(&self, line: &str) {
        if self.quiet {
            return;
        }
        match &self.multi {
            Some(multi) if !self.bars.is_empty() || self.spinner.is_some() => {
                let _ = multi.println(line);
            }
            _ => eprintln!("{line}"),
        }
    }
    pub fn handle(&mut self, event: Event) {
        match event {
            Event::Log { level, msg } => {
                let line = match level.as_str() {
                    "warn" | "warning" => format!("{} {msg}", self.palette.warn("!")),
                    "error" => format!("{} {msg}", self.palette.err("✗")),
                    _ => self.palette.dim(&format!("→ {msg}")),
                };
                self.println(&line);
            }
            // A checkpoint moves no counter of its own: the progress event for the
            // same window already drew it. It exists so a machine reader can tell
            // durable work from work still only in memory.
            Event::Checkpoint { .. } | Event::ModelRequest { .. } => {}
            Event::Published {
                episode,
                annotation,
                records,
                ..
            } => {
                if self.quiet {
                    return;
                }
                let label = station_label(&annotation);
                let unit = format!("{records} record{}", if records == 1 { "" } else { "s" });
                {
                    let summary = format!("{} {unit}", label.to_lowercase());
                    let summaries = self.finished.entry(episode.clone()).or_default();
                    match summaries.iter_mut().find(|(name, _)| name == &annotation) {
                        Some(slot) => slot.1 = summary,
                        None => summaries.push((annotation.clone(), summary)),
                    }
                }
                let message = format!(
                    "{} {:<13} {}",
                    self.palette.ok("✓"),
                    label,
                    self.palette.dim(&format!("{unit} · published"))
                );
                match self.bars.get(&(episode.clone(), annotation.clone())) {
                    Some(bar) => {
                        bar.set_style(
                            ProgressStyle::with_template("    {prefix:<20!} {msg}")
                                .expect("static template"),
                        );
                        bar.set_message(message);
                        bar.finish();
                    }
                    None => {
                        let name = self.name(&episode);
                        self.println(&format!(
                            "  {} {name}  {label}  {unit} · published",
                            self.palette.ok("✓")
                        ));
                    }
                }
            }
            Event::Progress {
                episode,
                station,
                done,
                total,
            } => {
                if self.quiet {
                    return;
                }
                self.stop_spinner();
                // Episodes are indexed one at a time, so the previous one is done.
                if self.active.as_deref() != Some(episode.as_str()) {
                    if let Some(previous) = self.active.take() {
                        self.collapse(&previous);
                    }
                    self.active = Some(episode.clone());
                }
                let name = self.name(&episode);
                let label = station_label(&station);
                match &self.multi {
                    Some(multi) => {
                        let key = (episode.clone(), station.clone());
                        if done == 0 {
                            self.estimates.remove(&key);
                        }
                        let (started, baseline) = self
                            .estimates
                            .entry(key.clone())
                            .or_insert((Instant::now(), done));
                        let eta = remaining(done, total, *baseline, started.elapsed());
                        let bar = self.bars.entry(key).or_insert_with(|| {
                            let bar = multi.add(ProgressBar::new(total.max(1)));
                            bar.set_style(
                                ProgressStyle::with_template(
                                    "  {spinner:.cyan} {prefix:<13} {bar:20.cyan/black} {pos}/{len} {wide_msg}",
                                )
                                .expect("static template")
                                .progress_chars("━╸─"),
                            );
                            bar.set_prefix(label.clone());
                            bar.enable_steady_tick(Duration::from_millis(100));
                            bar
                        });
                        if done == 0 {
                            bar.reset();
                        }
                        bar.set_message(eta);
                        bar.set_length(total.max(1));
                        bar.set_position(done.min(total.max(1)));
                        if done >= total {
                            let unit = station_unit(&station, total);
                            let summaries = self.finished.entry(episode.clone()).or_default();
                            if !summaries.iter().any(|(name, _)| name == &station) {
                                summaries.push((
                                    station.clone(),
                                    format!("{} {unit}", label.to_lowercase()),
                                ));
                            }
                            bar.finish_and_clear();
                        }
                    }
                    None => {
                        if done >= total {
                            eprintln!("  ✓ {name}  {label}  {}", station_unit(&station, total));
                        }
                    }
                }
            }
        }
    }
    pub fn finish(&mut self) {
        self.stop_spinner();
        if let Some(active) = self.active.take() {
            self.collapse(&active);
        }
        for bar in self.bars.values() {
            if !bar.is_finished() {
                bar.abandon();
            }
        }
        if let Some(multi) = &self.multi {
            let _ = multi.clear();
        }
    }
}

fn next_steps(out: &mut dyn Write, palette: &Palette, items: &[(&str, &str)]) -> io::Result<()> {
    let width = items
        .iter()
        .map(|(cmd, _)| measure_text_width(cmd))
        .max()
        .unwrap_or(0);
    for (cmd, note) in items {
        let pad = " ".repeat(width - measure_text_width(cmd) + 4);
        writeln!(
            out,
            "  {}{pad}{}",
            palette.cmd(cmd),
            palette.dim(note).trim_end()
        )?;
    }
    Ok(())
}

/// The closing block of a result: what to run now that this finished. Three
/// commands is the most anyone reads, so callers pass their best three.
fn next_block(out: &mut dyn Write, palette: &Palette, items: &[(&str, &str)]) -> io::Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    writeln!(out)?;
    writeln!(out, "{}", palette.bold("Next"))?;
    next_steps(out, palette, items)
}

/// Aligned `label  value` rows: the facts a screen states before its result.
fn facts(out: &mut dyn Write, palette: &Palette, rows: &[(&str, String)]) -> io::Result<()> {
    let width = rows
        .iter()
        .map(|(label, _)| measure_text_width(label))
        .max()
        .unwrap_or(0);
    for (label, value) in rows {
        if value.is_empty() {
            continue;
        }
        let pad = " ".repeat(width - measure_text_width(label) + 3);
        writeln!(out, "  {}{pad}{value}", palette.dim(label))?;
    }
    Ok(())
}

/// The `--semantic` value a person types, from the annotation's internal name.
fn item_name(annotation: &str) -> &str {
    let item = annotation.rsplit('/').next().unwrap_or(annotation);
    item.strip_prefix("semantic.").unwrap_or(item)
}

/// Quotes one argument so a copied command survives spaces, quotes, and globs.
pub fn shell_quote(value: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "-_./=:,@+".contains(c);
    if !value.is_empty() && value.chars().all(safe) {
        return value.to_owned();
    }
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// One shell line that runs `argv` exactly, whatever the paths contain.
pub fn shell_command(argv: &[String]) -> String {
    argv.iter()
        .map(|argument| shell_quote(argument))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A path as a person would type it: `~` while that stays unambiguous, and the
/// full quoted path once the characters would otherwise change what runs.
pub fn shell_path(path: &Path) -> String {
    let display = tilde(path);
    let safe = |c: char| c.is_ascii_alphanumeric() || "-_./=:,@+~".contains(c);
    if display.chars().all(safe) {
        display
    } else {
        shell_quote(&path.to_string_lossy())
    }
}

/// A compact launch pad; detailed configuration and inventories live in status.
pub fn home(
    out: &mut dyn Write,
    palette: &Palette,
    status: &Status,
    models: &ModelSummary,
) -> io::Result<()> {
    writeln!(
        out,
        "{}  {}",
        palette.bold(&format!("cerul {}", status.version)),
        palette.dim("· Search and annotate video")
    )?;
    if status.episodes.is_empty() {
        writeln!(out, "\n{}", palette.bold("Get started"))?;
    } else {
        let count = status.episodes.len();
        writeln!(
            out,
            "{} video{} indexed · {}",
            count,
            if count == 1 { "" } else { "s" },
            models.key.summary(palette)
        )?;
    }
    if !models.key.available() {
        writeln!(
            out,
            "{}",
            palette.dim(
                "Index asks for your Gemini key when needed; cerul auth set saves it ahead of time."
            )
        )?;
    }
    writeln!(out)?;
    next_steps(
        out,
        palette,
        &[
            ("cerul index ./video.mp4", "make searchable"),
            ("cerul search \"a person holding a cup\"", "find moments"),
            (
                "cerul annotate ./video.mp4 --semantic",
                "label actions · no index step needed",
            ),
        ],
    )?;
    writeln!(
        out,
        "\n{}",
        palette.dim("cerul status · cerul config · cerul annotate --help · cerul --help")
    )
}

pub fn tilde(path: &Path) -> String {
    if let Some(rest) = std::env::var_os("HOME").and_then(|home| path.strip_prefix(home).ok()) {
        return format!("~/{}", rest.display());
    }
    path.display().to_string()
}

/// The workspace overview. One row per video with a column per capability, so
/// "indexed" stops standing for three different facts. `detail` adds the files
/// a person needs when they asked about one path.
pub fn status(
    out: &mut dyn Write,
    palette: &Palette,
    status: &Status,
    models: &ModelSummary,
    probed: bool,
    detail: bool,
) -> io::Result<()> {
    writeln!(
        out,
        "Workspace {}   {}",
        tilde(&status.workspace),
        palette.dim(&format!("·   cerul {}", status.version))
    )?;
    writeln!(out)?;
    let mark = |ready: bool| {
        if ready {
            palette.ok("✓")
        } else {
            palette.dim("–")
        }
    };
    let search_state = |episode: &cerul::status::EpisodeStatus| -> (String, String) {
        if episode.embeddings.iter().any(|state| state.complete) {
            ("ready".into(), palette.ok("ready"))
        } else if episode.embeddings.is_empty() {
            ("–".into(), palette.dim("–"))
        } else {
            ("partial".into(), palette.warn("partial"))
        }
    };
    let items = |episode: &cerul::status::EpisodeStatus| -> Vec<String> {
        let mut names: Vec<String> = episode
            .annotations
            .iter()
            .filter(|name| name.contains("semantic."))
            .map(|name| item_name(name).to_owned())
            .collect();
        names.sort();
        names.dedup();
        names
    };
    let station = |episode: &cerul::status::EpisodeStatus, name: &str| {
        episode
            .annotations
            .iter()
            .any(|annotation| annotation == name || annotation.ends_with(&format!("/{name}")))
    };
    if status.episodes.is_empty() {
        writeln!(
            out,
            "  {}",
            palette.dim("no videos yet   ·   add one with: cerul index ./video.mp4")
        )?;
    } else if console::Term::stdout().size().1 < 80 {
        // Below 80 columns the row becomes a block: a wrapped table is unreadable
        // and a truncated path is useless.
        for episode in &status.episodes {
            let (plain, styled) = search_state(episode);
            let _ = plain;
            writeln!(
                out,
                "  {}   {}",
                palette.bold(&file_name(&episode.media)),
                palette.dim(&episode.duration_us.map(clock).unwrap_or_default())
            )?;
            writeln!(
                out,
                "    search {styled}   screen text {}   speech {}",
                mark(station(episode, "screen_text")),
                mark(station(episode, "transcript"))
            )?;
            let labels = items(episode);
            if !labels.is_empty() {
                writeln!(
                    out,
                    "    {} {}",
                    palette.dim("annotations"),
                    labels.join(" ")
                )?;
            }
            if detail {
                writeln!(out, "    {}", palette.dim(&tilde(&episode.sidecar)))?;
            }
        }
    } else {
        let name_of = |episode: &cerul::status::EpisodeStatus| {
            truncate_str(&file_name(&episode.media), 28, "…").to_string()
        };
        let width = status
            .episodes
            .iter()
            .map(|episode| measure_text_width(&name_of(episode)))
            .chain(std::iter::once(5))
            .max()
            .unwrap_or(5);
        let pad = |text: &str, styled: &str, to: usize| {
            format!(
                "{styled}{}",
                " ".repeat(to.saturating_sub(measure_text_width(text)))
            )
        };
        writeln!(
            out,
            "  {}",
            palette.dim(&format!(
                "{:<width$}   {:<8} {:<9} {:<6} {:<8} {}",
                "Video", "Length", "Search", "Text", "Speech", "Annotations"
            ))
        )?;
        for episode in &status.episodes {
            let name = short_name(&name_of(episode));
            let length = episode.duration_us.map(clock).unwrap_or_else(|| "–".into());
            let (plain, styled) = search_state(episode);
            let labels = items(episode);
            let annotations = match labels.len() {
                0 => palette.dim("–"),
                1..=3 => labels.join(" "),
                _ => format!(
                    "{} {}",
                    labels[..3].join(" "),
                    palette.dim(&format!("+{}", labels.len() - 3))
                ),
            };
            writeln!(
                out,
                "  {}   {:<8} {} {} {} {annotations}",
                pad(&name, &palette.bold(&name), width),
                length,
                pad(&plain, &styled, 9),
                pad("✓", &mark(station(episode, "screen_text")), 6),
                pad("✓", &mark(station(episode, "transcript")), 8),
            )?;
            if detail {
                writeln!(out, "    {}", palette.dim(&tilde(&episode.sidecar)))?;
            }
        }
    }
    let missing: Vec<&cerul::status::EpisodeStatus> = status
        .episodes
        .iter()
        .filter(|episode| !episode.media_present)
        .collect();
    if !missing.is_empty() {
        writeln!(
            out,
            "  {} {} video file{} moved away; their annotations are still here",
            palette.warn("!"),
            missing.len(),
            if missing.len() == 1 { "" } else { "s" }
        )?;
    }
    if let Some(failed) = status
        .episodes
        .iter()
        .flat_map(|episode| &episode.embeddings)
        .find(|state| state.error.is_some())
    {
        writeln!(
            out,
            "  {} processing issue: {}",
            palette.warn("!"),
            failed.error.as_deref().unwrap_or_default()
        )?;
    }
    writeln!(out)?;
    let dims = models
        .embedding_dims
        .map(|d| format!(" ({d}d)"))
        .unwrap_or_default();
    let check = if probed {
        let reachable = |name: &str| match status.capabilities.get(name) {
            Some(Some(true)) => palette.ok("ok"),
            Some(Some(false)) => palette.warn("unsupported"),
            _ => palette.dim("not checked"),
        };
        format!(
            "search {} · speech {} · vision {}",
            reachable("embedding"),
            reachable("transcription"),
            reachable("vision")
        )
    } else {
        palette.dim("endpoints not checked · cerul status --providers verifies them")
    };
    let ocr = match status.capabilities.get("ocr") {
        Some(Some(true)) => "OCR runs locally",
        _ => "OCR unavailable",
    };
    facts(
        out,
        palette,
        &[
            (
                "Models",
                format!(
                    "{}   {}{dims} · {} · {}",
                    models.key.provider,
                    models.embedding,
                    models.vision,
                    models.key.summary(palette)
                ),
            ),
            ("", check),
            (
                "Storage",
                format!(
                    "sidecars beside each video · {} vector space{} · {ocr}",
                    status.spaces.len(),
                    if status.spaces.len() == 1 { "" } else { "s" }
                ),
            ),
        ],
    )?;
    for (name, provider) in &status.providers {
        if let Some(error) = &provider.error {
            writeln!(out, "  {} {name}: {error}", palette.warn("!"))?;
        }
    }
    if !status.episodes.is_empty() && !detail {
        writeln!(out)?;
        writeln!(
            out,
            "{}",
            palette.dim("cerul status <path> for files · add --timeline to read the annotations")
        )?;
    }
    Ok(())
}

/// What replacing the program will do, before anybody is asked to agree to it.
pub fn upgrade_plan(
    out: &mut dyn Write,
    palette: &Palette,
    available: &crate::upgrade::Available,
) -> io::Result<()> {
    writeln!(
        out,
        "{} {}",
        palette.bold(&format!(
            "cerul {} → {}",
            available.current, available.latest
        )),
        palette.dim("· the newest published release")
    )?;
    writeln!(out)?;
    facts(
        out,
        palette,
        &[
            ("Runs", available.installer.clone()),
            ("Replaces", crate::upgrade::INSTALLED.join(", ")),
            (
                "Keeps",
                "your workspace, saved key, and annotation files".into(),
            ),
        ],
    )?;
    writeln!(out)
}

/// The upgrade result: what is installed now, and what to do about it.
pub fn upgrade(
    out: &mut dyn Write,
    palette: &Palette,
    available: &crate::upgrade::Available,
    upgraded: bool,
) -> io::Result<()> {
    if upgraded {
        writeln!(
            out,
            "{} {}",
            palette.ok("✓"),
            palette.bold(&format!("Upgraded to cerul {}", available.latest))
        )?;
        return next_block(
            out,
            palette,
            &[
                ("cerul --version", "confirm which build answers now"),
                ("cerul", "what is indexed and what to do next"),
            ],
        );
    }
    if !available.newer {
        return writeln!(
            out,
            "{} {}   {}",
            palette.ok("✓"),
            palette.bold(&format!(
                "cerul {} is the newest release",
                available.current
            )),
            palette.dim("nothing to install")
        );
    }
    writeln!(
        out,
        "{} {}",
        palette.warn("!"),
        palette.bold(&format!(
            "cerul {} is behind {}",
            available.current, available.latest
        ))
    )?;
    writeln!(out)?;
    facts(out, palette, &[("Installer", available.installer.clone())])?;
    next_block(
        out,
        palette,
        &[(
            "cerul upgrade --yes",
            "install it without being asked again",
        )],
    )
}

/// Where the agent skill can go, or where it just went. Read-only until asked.
pub fn skill(
    out: &mut dyn Write,
    palette: &Palette,
    installed: Option<&Path>,
    planned: Option<&Path>,
    targets: &BTreeMap<String, PathBuf>,
) -> io::Result<()> {
    if let Some(path) = installed {
        writeln!(
            out,
            "{} Wrote {}   {}",
            palette.ok("✓"),
            tilde(path),
            palette.dim(&format!("cerul {}", env!("CARGO_PKG_VERSION")))
        )?;
        writeln!(out)?;
        return writeln!(
            out,
            "{}",
            palette.dim("Start a new agent session so it picks the skill up.")
        );
    }
    if let Some(path) = planned {
        writeln!(
            out,
            "{} would write {}",
            palette.bold("Dry run:"),
            tilde(path)
        )?;
        return Ok(());
    }
    writeln!(
        out,
        "{}   {}",
        palette.bold("Agent skill"),
        palette.dim("teaches a coding agent to drive cerul through --json")
    )?;
    writeln!(out)?;
    if targets.is_empty() {
        writeln!(
            out,
            "  {}",
            palette.dim("HOME is not set, so no agent's own directory is known here.")
        )?;
        return next_block(
            out,
            palette,
            &[
                ("cerul skill --dir ./skills", "write it to a directory"),
                ("cerul skill --print", "read it, or pipe it somewhere"),
            ],
        );
    }
    let width = targets
        .keys()
        .map(|name| measure_text_width(name))
        .max()
        .unwrap_or(0);
    for (name, path) in targets {
        let pad = " ".repeat(width - measure_text_width(name) + 3);
        let state = if path.is_file() {
            palette.ok("installed")
        } else {
            palette.dim("not installed")
        };
        writeln!(out, "  {name}{pad}{state}   {}", palette.dim(&tilde(path)))?;
    }
    next_block(
        out,
        palette,
        &[
            ("cerul skill --install claude", "write it for that agent"),
            ("cerul skill --dir ./skills", "write it anywhere else"),
            ("cerul skill --print", "read it, or pipe it somewhere"),
        ],
    )
}

pub fn auth(out: &mut dyn Write, palette: &Palette, key: &KeyState) -> io::Result<()> {
    writeln!(
        out,
        "{}   {}",
        palette.bold("Gemini"),
        palette.dim(&key.endpoint)
    )?;
    if key.env_set {
        writeln!(out, "  using ${} from the environment", key.env)?;
    }
    if key.saved {
        writeln!(
            out,
            "  key saved in {}",
            key.credentials_path
                .as_deref()
                .map(tilde)
                .unwrap_or_else(|| "~/.cerul/credentials.json".into())
        )?;
    }
    if !key.available() {
        writeln!(out, "  {}", palette.warn("no key configured"))?;
        writeln!(out)?;
        writeln!(
            out,
            "Get a key at https://aistudio.google.com/apikey, then run {} or export ${}.",
            palette.cmd("cerul auth set"),
            key.env
        )?;
    } else {
        writeln!(out)?;
        writeln!(
            out,
            "{}",
            palette
                .dim("cerul auth set replaces the key · cerul auth remove deletes the saved one")
        )?;
    }
    Ok(())
}

/// Announces what a run will do before any model request is made.
pub fn open(
    out: &mut dyn Write,
    palette: &Palette,
    media: &Path,
    start_us: i64,
    player: &str,
    seeks: bool,
    opened: bool,
) -> io::Result<()> {
    writeln!(
        out,
        "{} {} at {} with {player}",
        if opened {
            "▶ Opening".to_owned()
        } else {
            palette.bold("Dry run:") + " would open"
        },
        palette.bold(&file_name(media)),
        clock(start_us)
    )?;
    if !seeks {
        writeln!(
            out,
            "  {}",
            palette.dim(&format!(
                "This player starts from the beginning; the moment is at {}. Install mpv or IINA to jump straight to it.",
                clock(start_us)
            ))
        )?;
    }
    Ok(())
}

pub fn index_plan(out: &mut dyn Write, palette: &Palette, paths: &[PathBuf]) -> io::Result<()> {
    let subject = if paths.len() == 1 {
        short_name(&file_name(&paths[0]))
    } else {
        format!("{} inputs", paths.len())
    };
    writeln!(out, "{} {subject}", palette.bold("Indexing"))
}

pub struct IndexContext {
    pub names: BTreeMap<String, PathBuf>,
    pub retry: Vec<String>,
    pub search_prefix: Vec<String>,
}

pub fn index(
    out: &mut dyn Write,
    palette: &Palette,
    report: &index::pipeline::Report,
    context: &IndexContext,
) -> io::Result<()> {
    let name_of = |episode: &index::pipeline::EpisodeResult| {
        context
            .names
            .get(&episode.episode_id)
            .map(|p| file_name(p))
            .unwrap_or_else(|| {
                episode
                    .sidecar
                    .file_name()
                    .map(|n| n.to_string_lossy().trim_end_matches(".cerul").to_owned())
                    .unwrap_or_else(|| episode.episode_id.clone())
            })
    };
    if report.dry_run {
        writeln!(out, "{} nothing was written.", palette.bold("Dry run:"))?;
        for episode in &report.episodes {
            writeln!(
                out,
                "  would index {}  {}",
                name_of(episode),
                palette.dim(&format!("→ {}", tilde(&episode.sidecar)))
            )?;
        }
        return Ok(());
    }
    if report.episodes.is_empty() {
        writeln!(out, "{} no videos were indexed.", palette.warn("!"))?;
        return Ok(());
    }
    let mut ready = 0;
    let compact = report.episodes.len() > 1
        && report.episodes.iter().all(|e| {
            e.streams.iter().all(|s| s.errors.is_empty()) && e.streams.iter().any(|s| s.indexed)
        });
    for episode in &report.episodes {
        let name = short_name(&episode.title.clone().unwrap_or_else(|| name_of(episode)));
        let errors: Vec<&String> = episode.streams.iter().flat_map(|s| &s.errors).collect();
        let indexed = episode.streams.iter().any(|s| s.indexed);
        if errors.is_empty() && indexed {
            ready += 1;
            if compact {
                continue;
            }
            writeln!(
                out,
                "{} {} ready to search",
                palette.ok("✓"),
                palette.bold(&name)
            )?;
        } else if indexed {
            ready += 1;
            writeln!(
                out,
                "{} {} partially indexed",
                palette.warn("!"),
                palette.bold(&name)
            )?;
            for error in errors {
                writeln!(out, "    {error}")?;
            }
            writeln!(
                out,
                "    {}",
                palette.dim("completed work is searchable and will be reused on retry")
            )?;
        } else {
            writeln!(out, "{} {} failed", palette.err("✗"), palette.bold(&name))?;
            for error in errors {
                writeln!(out, "    {error}")?;
            }
        }
    }
    if compact && ready > 1 {
        writeln!(
            out,
            "{} {} ready to search   {}",
            palette.ok("✓"),
            palette.bold(&format!("{ready} videos")),
            palette.dim("indexes saved beside your videos")
        )?;
    }
    if report.partial {
        writeln!(
            out,
            "\nRetry: {}",
            palette.cmd(&shell_command(&context.retry))
        )?;
    }
    if ready > 0 {
        let mut examples: Vec<(String, String)> = Vec::new();
        for episode in &report.episodes {
            for suggestion in &episode.suggestions {
                let mut argv = context.search_prefix.clone();
                if suggestion.exact {
                    argv.push("--text".into());
                }
                argv.push(suggestion.query.clone());
                if let Some(path) = context.names.get(&episode.episode_id) {
                    argv.extend(["--in".into(), path.to_string_lossy().into_owned()]);
                }
                let source = match suggestion.source.as_str() {
                    "transcript" => "speech",
                    "screen_text" => "screen text",
                    "semantic.scene" => "visual",
                    _ => "annotation",
                };
                examples.push((
                    shell_command(&argv),
                    format!("{source} · {}", clock(suggestion.start_us)),
                ));
                if examples.len() == 3 {
                    break;
                }
            }
            if examples.len() == 3 {
                break;
            }
        }
        if examples.is_empty() {
            let mut argv = context.search_prefix.clone();
            argv.push("describe a visual moment".into());
            writeln!(out, "\n{}", palette.cmd(&shell_command(&argv)))?;
        } else {
            writeln!(out, "\n{}", palette.bold("Try searching"))?;
            for (command, evidence) in examples {
                writeln!(out, "  {}", palette.dim(&evidence))?;
                writeln!(out, "  {}", palette.cmd(&command))?;
            }
        }
        let speech: Vec<_> = report.episodes.iter().flat_map(|e| &e.streams).collect();
        if speech.iter().all(|s| {
            matches!(
                s.speech,
                index::pipeline::SpeechStatus::NoAudio | index::pipeline::SpeechStatus::NoSpeech
            )
        }) {
            writeln!(
                out,
                "{}",
                palette.dim("No speech found; search still uses video and available screen text.")
            )?;
        } else if speech
            .iter()
            .any(|s| matches!(s.speech, index::pipeline::SpeechStatus::NotConfigured))
        {
            writeln!(
                out,
                "{}",
                palette
                    .dim("Speech skipped: no transcription key. Configure it with cerul config.")
            )?;
        }
    }
    Ok(())
}

pub struct SearchContext {
    pub query: Option<String>,
    pub image: Option<PathBuf>,
    pub text: bool,
    pub saved_to: Option<PathBuf>,
    pub terminal: Terminal,
    pub player: Player,
}

/// Wraps text to a column width on word boundaries.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if !current.is_empty()
            && measure_text_width(&current) + 1 + measure_text_width(word) > width
        {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}
fn kind_label(kind: Option<Kind>) -> &'static str {
    match kind {
        Some(Kind::Video) => "visual",
        Some(Kind::Speech) => "speech",
        Some(Kind::Screen) => "screen text",
        Some(Kind::Description) => "visual description",
        None => "annotation",
    }
}

/// One card per result: title, match metadata, still frame, excerpt, and a link
/// to the file. Modelled on the layout people already recognise from Cerul.
pub fn search(
    out: &mut dyn Write,
    palette: &Palette,
    report: &search::Report,
    context: &SearchContext,
) -> io::Result<()> {
    let subject = match (&context.query, &context.image) {
        (Some(query), _) => format!("\"{query}\""),
        (None, Some(image)) => format!("image {}", file_name(image)),
        (None, None) => "all indexed intervals".into(),
    };
    if report.dry_run {
        writeln!(
            out,
            "{} search for {subject} was not run.",
            palette.bold("Dry run:")
        )?;
        return Ok(());
    }
    if let Some(counts) = &report.counts {
        writeln!(
            out,
            "{} matching interval{} across {} video{}",
            palette.bold(&counts.records.to_string()),
            if counts.records == 1 { "" } else { "s" },
            counts.episodes,
            if counts.episodes == 1 { "" } else { "s" }
        )?;
        return Ok(());
    }
    if report.hits.is_empty() {
        writeln!(out, "No matches for {subject}.")?;
        writeln!(out)?;
        let mut hints = vec![("cerul status", "check which videos are indexed")];
        if context.query.is_some() && !context.text {
            hints.push((
                "cerul search --text \"exact words\"",
                "match screen text or speech exactly",
            ));
        }
        if context.text {
            hints.push((
                "cerul search \"describe the moment\"",
                "semantic search instead of exact text",
            ));
        }
        next_steps(out, palette, &hints)?;
        return Ok(());
    }
    let columns = console::Term::stdout().size().1.clamp(60, 110) as usize;
    let body = columns.saturating_sub(12);
    // One block per video: the same recording matched at several moments is one
    // thing a person is looking at, not several unrelated results.
    let mut groups: Vec<(String, Vec<(usize, &search::Hit)>)> = Vec::new();
    for (index, hit) in report.hits.iter().enumerate() {
        let key = hit
            .media
            .as_deref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| hit.episode.clone());
        match groups.iter_mut().find(|(existing, _)| *existing == key) {
            Some((_, moments)) => moments.push((index, hit)),
            None => groups.push((key, vec![(index, hit)])),
        }
    }
    writeln!(
        out,
        "{} for {subject}   {}",
        palette.bold(&format!(
            "{} moment{}",
            report.hits.len(),
            if report.hits.len() == 1 { "" } else { "s" }
        )),
        palette.dim(&format!(
            "in {} video{}",
            groups.len(),
            if groups.len() == 1 { "" } else { "s" }
        ))
    )?;
    writeln!(out)?;
    for (_, moments) in &groups {
        let first = moments[0].1;
        let name = first
            .media
            .as_deref()
            .map(file_name)
            .unwrap_or_else(|| first.episode.clone());
        let title = match first.media.as_deref() {
            Some(path) => context.terminal.file_link(&palette.bold(&name), path),
            None => palette.bold(&name),
        };
        writeln!(out, "  {title}")?;
        for (index, hit) in moments {
            let mut meta = vec![format!("{} → {}", clock(hit.start_us), clock(hit.end_us))];
            // A similarity is a ranking score, not a probability that the moment
            // is the one asked for, so it is never labelled a match percentage.
            if let Some(score) = hit.score {
                meta.push(format!("similarity {:.0}%", (score * 100.).clamp(0., 100.)));
            } else if context.text {
                meta.push("exact".into());
            }
            meta.push(kind_label(hit.matched).to_owned());
            writeln!(
                out,
                "  {}  {}",
                palette.cmd(&format!("[{}]", index + 1)),
                palette.dim(&meta.join("   "))
            )?;
            if let Some(preview) = hit
                .preview
                .as_deref()
                .filter(|_| context.terminal.images.is_some())
            {
                write!(out, "       ")?;
                if context.terminal.image(out, preview, 30, 8)? {
                    writeln!(out)?;
                }
            }
            let excerpt = hit.excerpt.split_whitespace().collect::<Vec<_>>().join(" ");
            if !excerpt.is_empty() {
                let mut lines = wrap(&excerpt, body);
                let truncated = lines.len() > 4;
                lines.truncate(4);
                if truncated && let Some(last) = lines.last_mut() {
                    last.push('…');
                }
                for line in lines {
                    writeln!(out, "       {line}")?;
                }
            }
            if let Some(path) = hit.media.as_deref() {
                let (label, target) = context.player.open(path, hit.start_us);
                if context.terminal.hyperlinks {
                    writeln!(
                        out,
                        "       {}",
                        context.terminal.link(&palette.cmd(&label), &target)
                    )?;
                }
            }
            if let Some(clip) = hit.clip.as_deref() {
                writeln!(
                    out,
                    "       {}   {}",
                    context.terminal.file_link(&palette.cmd("Saved clip"), clip),
                    palette.dim(&tilde(clip))
                )?;
            }
        }
        writeln!(out)?;
    }
    if context.terminal.images.is_none() && report.hits.iter().any(|hit| hit.preview.is_some()) {
        writeln!(
            out,
            "{}",
            palette.dim(
                "Still frames were saved but this terminal cannot draw them; iTerm2, Ghostty, Kitty, and WezTerm can."
            )
        )?;
    }
    if report
        .hits
        .iter()
        .any(|hit| matches!(hit.matched, Some(Kind::Video)))
    {
        writeln!(
            out,
            "{}",
            palette.dim("Visual matches cover an index window (30s by default); re-index with --chunk 10s for finer moments.")
        )?;
    }
    let mut steps: Vec<(String, &str)> = Vec::new();
    if let Some((number, _)) = report
        .hits
        .iter()
        .enumerate()
        .find(|(_, hit)| hit.media.is_some())
    {
        steps.push((
            format!("cerul open {}", number + 1),
            "play that moment in your player",
        ));
    }
    match (&context.saved_to, &context.query) {
        (Some(directory), _) => {
            let saved = report.hits.iter().filter(|hit| hit.clip.is_some()).count();
            writeln!(out)?;
            writeln!(
                out,
                "{} clip{} saved to {}",
                saved,
                if saved == 1 { "" } else { "s" },
                tilde(directory)
            )?;
        }
        (None, Some(query)) => {
            let flag = if context.text { " --text" } else { "" };
            steps.push((
                format!("cerul search{flag} {} --save ./clips", shell_quote(query)),
                "save these moments as clips",
            ));
        }
        (None, None) => {}
    }
    let steps: Vec<(&str, &str)> = steps
        .iter()
        .map(|(command, note)| (command.as_str(), *note))
        .collect();
    next_block(out, palette, &steps)?;
    Ok(())
}

pub fn remove(
    out: &mut dyn Write,
    palette: &Palette,
    report: &clean::Report,
    names: &BTreeMap<String, PathBuf>,
    unknown: &[PathBuf],
) -> io::Result<()> {
    for path in unknown {
        writeln!(
            out,
            "{} {} is not indexed, so there was nothing to remove",
            palette.dim("·"),
            palette.bold(&file_name(path))
        )?;
    }
    if report.items.is_empty() {
        if unknown.is_empty() {
            writeln!(
                out,
                "Nothing was removed.   {}",
                palette.dim("run cerul status to see what is indexed")
            )?;
        }
        return Ok(());
    }
    let mut lost_a_video = false;
    for item in &report.items {
        let subject = match item.action {
            clean::Action::RemoveSidecar => {
                lost_a_video = true;
                let name = item
                    .episode
                    .as_ref()
                    .and_then(|episode| names.get(episode))
                    .map(|media| file_name(media))
                    .unwrap_or_else(|| file_name(&item.path));
                format!("the index for {}", palette.bold(&name))
            }
            clean::Action::RemoveCache => "cached previews, query vectors, and proxies".into(),
            clean::Action::RemoveIndex => "a search index".into(),
            clean::Action::Compact => "space inside the search indexes".into(),
        };
        let verb = match (report.dry_run, &item.action) {
            (true, clean::Action::RemoveSidecar) => "would remove",
            (false, clean::Action::RemoveSidecar) => "Removed",
            (true, _) => "would free",
            (false, _) => "Freed",
        };
        if report.dry_run {
            writeln!(
                out,
                "  {} {verb} {subject}   {}",
                palette.dim("·"),
                palette.dim(&tilde(&item.path))
            )?;
        } else {
            writeln!(out, "{} {verb} {subject}", palette.ok("✓"))?;
        }
    }
    writeln!(out)?;
    let note = match (report.dry_run, lost_a_video) {
        (true, _) => "Dry run: nothing was changed. Video files are never touched.",
        (false, true) => "The video files were not touched. Index them again with cerul index.",
        (false, false) => "Nothing was lost: all of it is rebuilt on demand, with no model calls.",
    };
    writeln!(out, "{}", palette.dim(note))
}

/// The annotation receipt: what exists now, where it is, and what to run next.
/// A checkpoint on disk is not an annotation, so only published modules get a
/// record count and a path here.
pub fn annotate(
    out: &mut dyn Write,
    palette: &Palette,
    report: &annotate::pipeline::Report,
    names: &BTreeMap<String, PathBuf>,
    retry: Option<&str>,
) -> io::Result<()> {
    if report.modules.is_empty() && report.writebacks.is_empty() {
        writeln!(out, "Nothing to annotate.")?;
        return Ok(());
    }
    if report.dry_run {
        writeln!(
            out,
            "{} nothing was written and no model was called.",
            palette.bold("Dry run:")
        )?;
        writeln!(out)?;
    }
    // One block per input: the same recording labelled four ways is one thing a
    // person is looking at, not four unrelated results.
    let mut groups: Vec<(&str, Vec<&annotate::pipeline::ModuleResult>)> = Vec::new();
    for module in &report.modules {
        match groups
            .iter_mut()
            .find(|(episode, _)| *episode == module.episode.as_str())
        {
            Some((_, modules)) => modules.push(module),
            None => groups.push((module.episode.as_str(), vec![module])),
        }
    }
    let mut written = false;
    for (episode, modules) in &groups {
        // Episodes of one dataset share their MP4 shards, so a media file name
        // would label two different results identically. The dataset and the
        // episode index are what tell them apart.
        let source = &modules[0].source;
        let name = match modules[0].dataset {
            true => format!(
                "{} · episode {}",
                file_name(source),
                episode.rsplit('/').next().unwrap_or(episode)
            ),
            false if source.as_os_str().is_empty() => names
                .get(*episode)
                .map(|media| file_name(media))
                .unwrap_or_else(|| (*episode).to_string()),
            false => file_name(source),
        };
        if written {
            writeln!(out)?;
        }
        written = true;
        if report.dry_run {
            let items = modules
                .iter()
                .map(|module| item_name(&module.annotation))
                .collect::<Vec<_>>()
                .join(" · ");
            writeln!(out, "  {}   {}", palette.bold(&name), palette.dim(&items))?;
            continue;
        }
        let published = modules.iter().filter(|module| module.complete).count();
        let (mark, title) = match (published, published == modules.len()) {
            (_, true) => (palette.ok("✓"), format!("Annotated {name}")),
            (0, _) => (palette.err("✗"), format!("Could not annotate {name}")),
            _ => (palette.warn("!"), format!("Annotated {name} partially")),
        };
        writeln!(out, "{mark} {}", palette.bold(&title))?;
        writeln!(out)?;
        // Cameras of one episode share a file name, so the stream has to appear
        // whenever more than one of them was annotated.
        let streams: BTreeSet<&str> = modules.iter().map(|m| m.stream.as_str()).collect();
        let label = |module: &annotate::pipeline::ModuleResult| {
            if streams.len() > 1 {
                format!("{} · {}", module.stream, item_name(&module.annotation))
            } else {
                item_name(&module.annotation).to_owned()
            }
        };
        let width = modules
            .iter()
            .map(|module| measure_text_width(&label(module)))
            .max()
            .unwrap_or(0);
        let counts: Vec<String> = modules
            .iter()
            .map(|module| {
                if module.complete {
                    format!(
                        "{} record{}",
                        module.records,
                        if module.records == 1 { "" } else { "s" }
                    )
                } else {
                    String::new()
                }
            })
            .collect();
        let count_width = counts
            .iter()
            .map(|count| measure_text_width(count))
            .max()
            .unwrap_or(0);
        let mixed = published != modules.len();
        for (module, count) in modules.iter().zip(&counts) {
            let glyph = if mixed {
                let mark = match (&module.error, module.complete) {
                    (_, true) => palette.ok("✓"),
                    (Some(_), _) => palette.err("✗"),
                    _ => palette.dim("·"),
                };
                format!("{mark} ")
            } else {
                String::new()
            };
            let text = label(module);
            let pad = " ".repeat(width - measure_text_width(&text) + 3);
            match (&module.error, module.complete) {
                (_, true) => {
                    let gap = " ".repeat(count_width - measure_text_width(count) + 3);
                    let path = module
                        .path
                        .as_deref()
                        .map(tilde)
                        .unwrap_or_else(|| "published".into());
                    writeln!(
                        out,
                        "  {glyph}{text}{pad}{count}{gap}{}",
                        palette.dim(&path)
                    )?;
                }
                (Some(error), _) => writeln!(
                    out,
                    "  {glyph}{text}{pad}{}",
                    palette.warn(&format!("stopped · {error}"))
                )?,
                (None, false) => {
                    writeln!(out, "  {glyph}{text}{pad}{}", palette.dim("not started"))?
                }
            }
        }
    }
    for writeback in &report.writebacks {
        if written {
            writeln!(out)?;
        }
        written = true;
        match &writeback.error {
            None => {
                writeln!(
                    out,
                    "{} {}",
                    palette.ok("✓"),
                    palette.bold(&format!(
                        "Wrote LeRobot dataset {}",
                        file_name(&writeback.destination)
                    ))
                )?;
                writeln!(out, "  {}", palette.dim(&tilde(&writeback.destination)))?;
            }
            Some(error) => {
                writeln!(
                    out,
                    "{} {}",
                    palette.err("✗"),
                    palette.bold(&format!(
                        "LeRobot writeback failed for {}",
                        file_name(&writeback.source)
                    ))
                )?;
                writeln!(out, "  {error}")?;
            }
        }
    }
    if !report.dry_run {
        // A dataset's episodes share their video shards, so pointing at one
        // shard would read back a fraction of what this run produced. The input
        // the person named is what covers all of it.
        let media = report
            .modules
            .iter()
            .find(|module| module.complete)
            .map(|module| match module.dataset {
                true => Some(&module.source),
                false => names.get(&module.episode).or(Some(&module.source)),
            });
        let mut steps: Vec<(String, &str)> = Vec::new();
        if let Some(Some(path)) = media {
            steps.push((
                format!("cerul status {} --timeline", shell_path(path)),
                "read the labels in order",
            ));
        }
        if report
            .modules
            .iter()
            .any(|module| module.complete && item_name(&module.annotation) == "event")
        {
            steps.push((
                "cerul search --filter 'semantic.event.verb=grasp'".into(),
                "find one action across videos",
            ));
        }
        let steps: Vec<(&str, &str)> = steps
            .iter()
            .map(|(command, note)| (command.as_str(), *note))
            .collect();
        next_block(out, palette, &steps)?;
    }
    if report.partial {
        writeln!(out)?;
        let anything = report.modules.iter().any(|module| module.complete);
        match retry {
            Some(command) => {
                // Promising that finished work is reused is only reassuring when
                // something actually finished.
                writeln!(
                    out,
                    "{}",
                    palette.dim(match anything {
                        true => "Finished windows are saved and will be reused. Continue with:",
                        false => "Nothing was published. After fixing the problem above:",
                    })
                )?;
                writeln!(out, "  {}", palette.cmd(command))?;
            }
            None => writeln!(
                out,
                "{}",
                palette
                    .dim("Finished windows are saved; re-run the same command to resume the rest.")
            )?,
        }
    }
    if !report.dry_run && report.modules.iter().any(|module| module.complete) {
        writeln!(out)?;
        writeln!(
            out,
            "{}",
            palette.dim("Labels are model-generated; review them before training on them.")
        )?;
    }
    Ok(())
}

/// Published annotation records in time order: the cheapest way to judge whether
/// a run produced anything worth keeping, without opening a JSONL file.
pub fn timeline(
    out: &mut dyn Write,
    palette: &Palette,
    timeline: &cerul::status::Timeline,
    kind: Option<&str>,
) -> io::Result<()> {
    if timeline.episodes.is_empty() {
        // Screen text and speech are annotations too, but a timeline is for the
        // labels a person asked a model for, so say which kind is missing.
        writeln!(out, "No semantic annotations here yet.")?;
        return next_block(
            out,
            palette,
            &[(
                "cerul annotate ./video.mp4 --semantic subtask,event,interaction,state",
                "label actions in a video",
            )],
        );
    }
    for (position, episode) in timeline.episodes.iter().enumerate() {
        if position > 0 {
            writeln!(out)?;
        }
        let items = episode
            .annotations
            .iter()
            .map(|name| item_name(name))
            .collect::<Vec<_>>()
            .join(" ");
        let length = episode
            .duration_us
            .map(|us| format!("  {}", clock(us)))
            .unwrap_or_default();
        writeln!(
            out,
            "{}{length}  {}",
            palette.bold(&file_name(&episode.media)),
            palette.dim(&format!("·  {items}  ·  {}", tilde(&episode.sidecar)))
        )?;
        writeln!(out)?;
        if episode.entries.is_empty() {
            writeln!(
                out,
                "  {}",
                palette.dim(&match kind {
                    Some(kind) => format!("no {kind} records"),
                    None => "no records".into(),
                })
            )?;
            continue;
        }
        let times: Vec<String> = episode
            .entries
            .iter()
            .map(|entry| {
                // A moment and a span are different facts; only a span gets a range.
                if entry.end_us > entry.start_us {
                    format!("{} – {}", clock(entry.start_us), clock(entry.end_us))
                } else {
                    clock(entry.start_us)
                }
            })
            .collect();
        let time_width = times
            .iter()
            .map(|t| measure_text_width(t))
            .max()
            .unwrap_or(0);
        // Cameras of one episode produce the same kinds of record at the same
        // times, so the stream has to appear once more than one was annotated.
        let streams: BTreeSet<&str> = episode
            .entries
            .iter()
            .map(|entry| entry.stream.as_str())
            .collect();
        let label = |entry: &cerul::status::TimelineEntry| match streams.len() > 1 {
            true => format!("{} · {}", entry.stream, item_name(&entry.annotation)),
            false => item_name(&entry.annotation).to_owned(),
        };
        let name_width = episode
            .entries
            .iter()
            .map(|entry| measure_text_width(&label(entry)))
            .max()
            .unwrap_or(0);
        for (entry, time) in episode.entries.iter().zip(&times) {
            let name = label(entry);
            writeln!(
                out,
                "  {time:>time_width$}   {}{}{}",
                palette.cmd(&name),
                " ".repeat(name_width - measure_text_width(&name) + 3),
                entry.summary
            )?;
        }
        if episode.total > episode.entries.len() {
            writeln!(out)?;
            writeln!(
                out,
                "{}",
                palette.dim(&format!(
                    "showing {} of {} records · --limit {} for more{}",
                    episode.entries.len(),
                    episode.total,
                    episode.total.min(1000),
                    if kind.is_some() {
                        ""
                    } else {
                        " · --type event for one kind"
                    }
                ))
            )?;
        }
    }
    Ok(())
}

/// Human error line plus a hint on how to recover.
pub fn error(palette: &Palette, code: u8, message: &str, hint: Option<&str>) -> String {
    // What went wrong, then how to get out of it, then the code a script reads.
    // Cancelling is a choice somebody made, so it is not painted as a failure.
    let category = match code {
        2 => "invalid arguments or configuration",
        3 => "unavailable dependency or capability",
        5 => "cancelled",
        6 => "partial result",
        _ => "execution failure",
    };
    let mark = if code == 5 {
        palette.warn("!")
    } else {
        palette.err("✗")
    };
    let mut text = format!("{mark} {message}");
    if let Some(hint) = hint {
        text.push_str(&format!("\n  {}", palette.cmd(hint)));
    }
    text.push_str(&format!(
        "\n  {}",
        palette.dim(&format!("exit {code} · {category}"))
    ));
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use cerul::search::{Hit, Report};

    #[test]
    fn eta_requires_measured_work_and_names_stay_bounded() {
        assert_eq!(remaining(1, 10, 0, Duration::from_secs(5)), "estimating…");
        assert_eq!(
            remaining(80, 100, 80, Duration::from_secs(10)),
            "estimating…"
        );
        assert_eq!(remaining(4, 10, 0, Duration::from_secs(8)), "~12s left");
        assert_eq!(remaining(10, 10, 0, Duration::from_secs(20)), "");
        assert!(measure_text_width(&short_name(&"示例".repeat(80))) <= 48);
    }

    #[test]
    fn index_examples_are_scoped_quoted_and_partial_results_offer_retry() {
        let report: index::pipeline::Report = serde_json::from_value(serde_json::json!({
            "episodes": [{"episode_id":"example", "sidecar":"/media/example.cerul",
                "streams":[{"stream":"video", "indexed":true, "speech":"failed", "vector_rows":3, "errors":["speech unavailable"]}],
                "suggestions":[{"query":"worker's cup $(touch danger)","exact":false,"source":"semantic.subtask","stream":"video","record_id":"step-1","start_us":2000000,"end_us":5000000,"input_hash":"current"}]
            }], "partial":true,"dry_run":false
        })).unwrap();
        let context = IndexContext {
            names: BTreeMap::from([(
                "example".into(),
                PathBuf::from(format!("/media/{}.mp4", "long-name-".repeat(20))),
            )]),
            retry: vec![
                "cerul".into(),
                "index".into(),
                "video with spaces.mp4".into(),
                "--no-ocr".into(),
            ],
            search_prefix: vec![
                "cerul".into(),
                "--workspace".into(),
                "/tmp/my workspace".into(),
                "search".into(),
            ],
        };
        let mut output = Vec::new();
        index(&mut output, &Palette::new(false), &report, &context).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("partially indexed"));
        assert!(
            !output
                .lines()
                .next()
                .unwrap()
                .contains(&"long-name-".repeat(8))
        );
        assert!(output.contains("Retry: cerul index 'video with spaces.mp4' --no-ocr"));
        assert!(output.contains("cerul --workspace '/tmp/my workspace' search"));
        assert!(output.contains(&shell_quote("worker's cup $(touch danger)")));
        assert!(output.contains("--in /media/"));
        assert!(output.contains("annotation · 00:02"));
        assert!(!output.contains("describe a visual moment"));
    }

    #[test]
    fn configured_home_keeps_annotation_visible_and_status_locates_sidecars() {
        let dir = tempfile::tempdir().unwrap();
        let mut status = cerul::status::inspect(dir.path(), None).unwrap();
        status.episodes.push(cerul::status::EpisodeStatus {
            understanding: Default::default(),
            duration_us: Some(150_000_000),
            episode_id: "demo/0".into(),
            media: PathBuf::from("/videos/demo.mp4"),
            sidecar: PathBuf::from("/videos/demo.mp4.cerul"),
            media_present: true,
            annotations: vec!["semantic.subtask".into()],
            embedding_spaces: Vec::new(),
            embeddings: Vec::new(),
        });
        let models = ModelSummary {
            embedding: "test".into(),
            embedding_dims: None,
            vision: "test".into(),
            transcription: "test".into(),
            key: KeyState {
                provider: "test".into(),
                endpoint: "http://localhost".into(),
                env: "TEST_KEY".into(),
                env_set: false,
                saved: true,
                credentials_path: None,
            },
        };
        let palette = Palette::new(false);
        let mut out = Vec::new();
        home(&mut out, &palette, &status, &models).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("1 video indexed"));
        assert!(text.contains("key saved"));
        assert!(text.contains("cerul annotate ./video.mp4 --semantic"));
        assert!(text.contains("cerul annotate --help"));
        // The overview is a table; the files belong to the view a person asked
        // for by naming a path.
        let mut out = Vec::new();
        super::status(&mut out, &palette, &status, &models, false, false).unwrap();
        let overview = String::from_utf8(out).unwrap();
        assert!(overview.contains("demo.mp4"), "{overview}");
        assert!(!overview.contains("/videos/demo.mp4.cerul"), "{overview}");
        let mut out = Vec::new();
        super::status(&mut out, &palette, &status, &models, false, true).unwrap();
        assert!(
            String::from_utf8(out)
                .unwrap()
                .contains("/videos/demo.mp4.cerul")
        );
    }

    fn hit() -> Hit {
        Hit {
            evidence_scores: Vec::new(),
            episode: "b52ef6450d471af5/0".into(),
            stream: "primary".into(),
            start_us: 12_000_000,
            end_us: 19_500_000,
            frame_range: None,
            score: Some(0.7412),
            matched: Some(Kind::Speech),
            excerpt: "we really spent almost a year perfecting this harness".into(),
            annotations: Vec::new(),
            clip: None,
            media: Some(PathBuf::from("/videos/a demo.mp4")),
            preview: None,
        }
    }

    #[test]
    fn cards_name_the_file_and_stay_free_of_escapes_on_plain_terminals() {
        let report = Report {
            hits: vec![hit()],
            counts: None,
            dry_run: false,
        };
        let context = SearchContext {
            query: Some("a person holding a cup".into()),
            image: None,
            text: false,
            saved_to: None,
            terminal: Terminal::none(),
            player: Player::System,
        };
        let mut out = Vec::new();
        search(&mut out, &Palette::new(false), &report, &context).unwrap();
        let text = String::from_utf8(out).unwrap();
        // The episode hash and raw score never reach a person.
        assert!(!text.contains("b52ef6450d471af5"), "{text}");
        assert!(!text.contains("0.74"), "{text}");
        assert!(text.contains("a demo.mp4"), "{text}");
        assert!(text.contains("similarity 74%"), "{text}");
        assert!(text.contains("00:12 → 00:19"), "{text}");
        assert!(text.contains("speech"), "{text}");
        assert!(text.contains("perfecting this harness"), "{text}");
        // Without hyperlink support a label with no URL behind it is noise; the
        // numbered result and `cerul open` are what actually play the moment.
        assert!(!text.contains("Open video"), "{text}");
        assert!(text.contains("cerul open 1"), "{text}");
        // No colour, hyperlink, or image control sequences without support.
        assert!(!text.contains('\u{1b}'), "{text}");
    }

    #[test]
    fn moments_in_one_video_share_a_card_and_keep_their_global_numbers() {
        let mut second = hit();
        second.start_us = 90_000_000;
        second.end_us = 96_000_000;
        second.score = Some(0.61);
        second.excerpt = String::new();
        let report = Report {
            hits: vec![hit(), second],
            counts: None,
            dry_run: false,
        };
        let context = SearchContext {
            query: Some("a cup".into()),
            image: None,
            text: false,
            saved_to: None,
            terminal: Terminal::none(),
            player: Player::System,
        };
        let mut out = Vec::new();
        search(&mut out, &Palette::new(false), &report, &context).unwrap();
        let text = String::from_utf8(out).unwrap();
        // Two moments, one heading: the file is named once.
        assert_eq!(text.matches("a demo.mp4").count(), 1, "{text}");
        assert!(text.contains("2 moments"), "{text}");
        assert!(text.contains("in 1 video"), "{text}");
        assert!(text.contains("[1]") && text.contains("[2]"), "{text}");
        assert!(text.contains("01:30 → 01:36"), "{text}");
        assert!(text.contains("cerul open 1"), "{text}");
    }

    #[test]
    fn hyperlinks_percent_encode_paths_and_disappear_without_support() {
        let terminal = Terminal {
            hyperlinks: true,
            images: None,
        };
        let link = terminal.file_link("1.mp4", Path::new("/videos/a demo.mp4"));
        assert_eq!(
            link,
            "\u{1b}]8;;file:///videos/a%20demo.mp4\u{7}1.mp4\u{1b}]8;;\u{7}"
        );
        assert_eq!(
            Terminal::none().file_link("1.mp4", Path::new("/videos/a demo.mp4")),
            "1.mp4"
        );
        let (label, target) = Player::Iina.open(Path::new("/videos/a demo.mp4"), 12_500_000);
        assert_eq!(label, "Open at 00:12");
        assert_eq!(
            target,
            "iina://open?url=file%3A%2F%2F%2Fvideos%2Fa%2520demo.mp4&mpv_start=12.500"
        );
        assert_eq!(
            Player::System.open(Path::new("/videos/a demo.mp4"), 12_500_000),
            ("Open video".into(), "file:///videos/a%20demo.mp4".into())
        );
    }

    fn module(item: &str, records: usize, error: Option<&str>) -> annotate::pipeline::ModuleResult {
        annotate::pipeline::ModuleResult {
            episode: "demo/0".into(),
            stream: "primary".into(),
            annotation: format!("semantic.{item}"),
            records,
            complete: error.is_none(),
            error: error.map(str::to_owned),
            source: PathBuf::from("/videos/demo.mp4"),
            dataset: false,
            path: error
                .is_none()
                .then(|| PathBuf::from(format!("/videos/demo.mp4.cerul/semantic.{item}.jsonl"))),
        }
    }

    #[test]
    fn the_annotation_receipt_names_every_published_file_and_what_to_run_next() {
        let report = annotate::pipeline::Report {
            modules: vec![module("subtask", 12, None), module("event", 18, None)],
            writebacks: Vec::new(),
            partial: false,
            dry_run: false,
            retry: None,
        };
        let names = BTreeMap::from([("demo/0".to_owned(), PathBuf::from("/videos/demo.mp4"))]);
        let mut out = Vec::new();
        annotate(&mut out, &Palette::new(false), &report, &names, None).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("Annotated demo.mp4"), "{text}");
        // The episode hash is an internal name; a person needs the file.
        assert!(!text.contains("demo/0"), "{text}");
        assert!(text.contains("subtask"), "{text}");
        assert!(text.contains("12 records"), "{text}");
        assert!(
            text.contains("/videos/demo.mp4.cerul/semantic.event.jsonl"),
            "{text}"
        );
        assert!(
            text.contains("cerul status /videos/demo.mp4 --timeline"),
            "{text}"
        );
        assert!(text.contains("cerul search --filter"), "{text}");
        assert!(text.contains("review them before training"), "{text}");
    }

    #[test]
    fn a_partial_run_separates_published_work_from_the_command_that_continues_it() {
        let report = annotate::pipeline::Report {
            modules: vec![
                module("subtask", 12, None),
                module("event", 0, Some("gemini rate limit (429)")),
                annotate::pipeline::ModuleResult {
                    records: 0,
                    complete: false,
                    ..module("state", 0, None)
                },
            ],
            writebacks: Vec::new(),
            partial: true,
            dry_run: false,
            retry: None,
        };
        let names = BTreeMap::from([("demo/0".to_owned(), PathBuf::from("/videos/demo.mp4"))]);
        let mut out = Vec::new();
        let retry = "cerul annotate /videos/demo.mp4 --semantic subtask,event,state --rpm 6";
        annotate(&mut out, &Palette::new(false), &report, &names, Some(retry)).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("partially"), "{text}");
        assert!(text.contains("12 records"), "{text}");
        assert!(text.contains("stopped · gemini rate limit (429)"), "{text}");
        assert!(text.contains("not started"), "{text}");
        assert!(text.contains(retry), "{text}");
        // Recomputing would throw away the windows that did finish.
        assert!(!text.contains("--recompute"), "{text}");
    }

    fn record(
        start_us: i64,
        end_us: i64,
        fields: &[(&str, serde_json::Value)],
    ) -> cerul::annotations::Record {
        cerul::annotations::Record {
            id: format!("r-{start_us}"),
            start_us,
            end_us,
            confidence: None,
            fields: fields
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect(),
        }
    }

    #[test]
    fn the_timeline_orders_records_and_writes_each_kind_in_its_own_words() {
        let entries = [
            (
                "semantic.subtask",
                record(0, 14_000_000, &[("text", "reach for the cup".into())]),
            ),
            (
                "semantic.event",
                record(
                    3_000_000,
                    3_000_000,
                    &[
                        ("verb", "grasp".into()),
                        ("objects", serde_json::json!(["cup"])),
                    ],
                ),
            ),
            (
                "semantic.state",
                record(
                    20_000_000,
                    20_000_000,
                    &[
                        ("object", "cup".into()),
                        ("attribute", "location".into()),
                        ("before", "on table".into()),
                        ("after", "in hand".into()),
                    ],
                ),
            ),
        ];
        let timeline = cerul::status::Timeline {
            episodes: vec![cerul::status::TimelineEpisode {
                episode_id: "demo/0".into(),
                media: PathBuf::from("/videos/demo.mp4"),
                sidecar: PathBuf::from("/videos/demo.mp4.cerul"),
                duration_us: Some(150_000_000),
                annotations: vec!["semantic.subtask".into(), "semantic.event".into()],
                total: 53,
                entries: entries
                    .iter()
                    .map(|(annotation, record)| cerul::status::TimelineEntry {
                        revision: "fixture-revision".into(),
                        episode: "demo/0".into(),
                        stream: "primary".into(),
                        annotation: (*annotation).to_owned(),
                        start_us: record.start_us,
                        end_us: record.end_us,
                        summary: cerul::status::summarize_record(annotation, record),
                        record: record.clone(),
                    })
                    .collect(),
            }],
        };
        let mut out = Vec::new();
        super::timeline(&mut out, &Palette::new(false), &timeline, None).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("demo.mp4  02:30"), "{text}");
        assert!(text.contains("00:00 – 00:14   subtask"), "{text}");
        assert!(text.contains("reach for the cup"), "{text}");
        // A moment has one time, not a range that starts and ends together.
        assert!(
            text.contains("00:03") && !text.contains("00:03 – 00:03"),
            "{text}"
        );
        assert!(text.contains("grasp cup"), "{text}");
        assert!(
            text.contains("cup · location: on table → in hand"),
            "{text}"
        );
        assert!(text.contains("showing 3 of 53 records"), "{text}");
    }

    #[test]
    fn a_timeline_of_several_cameras_says_which_camera_each_record_came_from() {
        let entry = |stream: &str, start_us: i64| cerul::status::TimelineEntry {
            revision: "fixture-revision".into(),
            episode: "d7f0/000000".into(),
            stream: stream.into(),
            annotation: "semantic.event".into(),
            start_us,
            end_us: start_us,
            summary: "grasp cup".into(),
            record: record(start_us, start_us, &[("verb", "grasp".into())]),
        };
        let mut episode = cerul::status::TimelineEpisode {
            episode_id: "d7f0/000000".into(),
            media: PathBuf::from("/data/egodemo/videos/front/file-000.mp4"),
            sidecar: PathBuf::from("/data/egodemo/.cerul/episodes/0"),
            duration_us: Some(8_000_000),
            annotations: vec!["semantic.event".into()],
            total: 2,
            entries: vec![
                entry("observation.images.front", 3_000_000),
                entry("observation.images.wrist", 3_000_000),
            ],
        };
        let render = |episodes| {
            let mut out = Vec::new();
            super::timeline(
                &mut out,
                &Palette::new(false),
                &cerul::status::Timeline { episodes },
                None,
            )
            .unwrap();
            String::from_utf8(out).unwrap()
        };
        // Two cameras record the same kind of thing at the same time, so the
        // lines are indistinguishable without the camera.
        let text = render(vec![episode.clone()]);
        assert!(text.contains("observation.images.front · event"), "{text}");
        assert!(text.contains("observation.images.wrist · event"), "{text}");
        // One camera needs no column repeating its name on every line.
        episode.entries.truncate(1);
        episode.total = 1;
        let text = render(vec![episode]);
        assert!(!text.contains("observation.images.front ·"), "{text}");
        assert!(text.contains("event"), "{text}");
    }

    #[test]
    fn a_dataset_receipt_points_at_the_dataset_rather_than_a_shared_shard() {
        let module = annotate::pipeline::ModuleResult {
            episode: "d7f0/000001".into(),
            stream: "observation.images.front".into(),
            annotation: "semantic.subtask".into(),
            records: 6,
            complete: true,
            error: None,
            source: PathBuf::from("/data/egodemo"),
            dataset: true,
            path: Some(PathBuf::from(
                "/data/egodemo/.cerul/episodes/1/semantic.subtask.jsonl",
            )),
        };
        let report = annotate::pipeline::Report {
            modules: vec![module],
            writebacks: Vec::new(),
            partial: false,
            dry_run: false,
            retry: None,
        };
        // Every episode of a dataset shares this shard, so the registry's media
        // path would read back one episode out of the run.
        let names = BTreeMap::from([(
            "d7f0/000001".to_owned(),
            PathBuf::from("/data/egodemo/videos/observation.images.front/chunk-000/file-000.mp4"),
        )]);
        let mut out = Vec::new();
        annotate(&mut out, &Palette::new(false), &report, &names, None).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("egodemo · episode 000001"), "{text}");
        assert!(
            text.contains("cerul status /data/egodemo --timeline"),
            "{text}"
        );
        assert!(!text.contains("file-000.mp4"), "{text}");
    }

    #[test]
    fn upgrading_says_what_it_would_replace_and_what_it_would_keep() {
        let available = crate::upgrade::Available {
            current: "0.0.6".into(),
            latest: "0.0.7".into(),
            newer: true,
            installer: "https://example.test/v0.0.7/cerul-installer.sh".into(),
        };
        let render = |available: &crate::upgrade::Available, upgraded| {
            let mut out = Vec::new();
            super::upgrade(&mut out, &Palette::new(false), available, upgraded).unwrap();
            String::from_utf8(out).unwrap()
        };
        // Behind, and not installed: the command to install is the whole point.
        let text = render(&available, false);
        assert!(text.contains("0.0.6 is behind 0.0.7"), "{text}");
        assert!(text.contains("cerul upgrade --yes"), "{text}");
        assert!(text.contains("cerul-installer.sh"), "{text}");

        let text = render(&available, true);
        assert!(text.contains("Upgraded to cerul 0.0.7"), "{text}");
        assert!(text.contains("cerul --version"), "{text}");

        // Nothing to do is a result, not a warning.
        let current = crate::upgrade::Available {
            current: "0.0.7".into(),
            newer: false,
            ..available.clone()
        };
        let text = render(&current, false);
        assert!(text.contains("0.0.7 is the newest release"), "{text}");
        assert!(!text.contains("upgrade --yes"), "{text}");

        // What replacing the program touches, said before anyone agrees to it.
        let mut out = Vec::new();
        super::upgrade_plan(&mut out, &Palette::new(false), &available).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("cerul 0.0.6 → 0.0.7"), "{text}");
        assert!(text.contains("cerul-ffmpeg"), "{text}");
        assert!(text.contains("saved key"), "{text}");
    }

    #[test]
    fn a_command_survives_being_copied_out_of_the_receipt() {
        assert_eq!(shell_quote("./demo.mp4"), "./demo.mp4");
        assert_eq!(
            shell_quote("my videos/a demo.mp4"),
            "'my videos/a demo.mp4'"
        );
        assert_eq!(shell_quote("it's here.mp4"), r"'it'\''s here.mp4'");
        assert_eq!(
            shell_command(&["cerul".into(), "annotate".into(), "a b.mp4".into()]),
            "cerul annotate 'a b.mp4'"
        );
    }

    #[test]
    fn wrapping_breaks_on_words_and_keeps_every_one() {
        let lines = wrap("the quick brown fox jumps", 11);
        assert_eq!(lines, ["the quick", "brown fox", "jumps"]);
        assert!(wrap("", 10).is_empty());
    }
}
