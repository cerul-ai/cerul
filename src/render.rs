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
    collections::{BTreeMap, HashMap},
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    time::Duration,
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
            finished: HashMap::new(),
            active: None,
            spinner: None,
            names: HashMap::new(),
            resolved: std::collections::HashSet::new(),
        }
    }
    fn name(&mut self, episode: &str) -> String {
        if let Some(name) = self.names.get(episode) {
            return name.clone();
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
            .unwrap_or_else(|| episode.to_owned())
    }
    /// Replaces one episode's station bars with a single line, so indexing many
    /// videos keeps the live area the size of the video being worked on.
    fn collapse(&mut self, episode: &str) {
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
            Event::Progress {
                episode,
                station,
                done,
                total,
            } => {
                if self.quiet {
                    return;
                }
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
                        // Only the first station of an episode repeats its name.
                        let heading = !self.bars.keys().any(|(id, _)| id == &episode);
                        let key = (episode.clone(), station.clone());
                        let bar = self.bars.entry(key).or_insert_with(|| {
                            let bar = multi.add(ProgressBar::new(total.max(1)));
                            bar.set_style(
                                ProgressStyle::with_template(
                                    "  {spinner:.cyan} {prefix:<20!} {msg:<13} {bar:20.cyan/black} {pos}/{len} {elapsed}",
                                )
                                .expect("static template")
                                .progress_chars("━╸─"),
                            );
                            bar.set_prefix(if heading {
                                name.clone()
                            } else {
                                String::new()
                            });
                            bar.set_message(label.clone());
                            bar.enable_steady_tick(Duration::from_millis(100));
                            bar
                        });
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
                            bar.set_style(
                                ProgressStyle::with_template("    {prefix:<20!} {msg}")
                                    .expect("static template"),
                            );
                            bar.set_message(format!(
                                "{} {:<13} {}",
                                self.palette.ok("✓"),
                                label,
                                self.palette.dim(&unit)
                            ));
                            bar.finish();
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
        writeln!(out, "  {}{pad}{}", palette.cmd(cmd), palette.dim(note))?;
    }
    Ok(())
}

/// The screen shown for a bare `cerul`. Until the first video is searchable it
/// walks through setup step by step; afterwards it is a short launch pad.
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
        palette.dim("·  Search your videos with words")
    )?;
    writeln!(out)?;
    let key_ready = models.key.available();
    let indexed = status.episodes.len();
    let searchable = status
        .episodes
        .iter()
        .filter(|e| e.embeddings.iter().any(|x| x.complete) || !e.annotations.is_empty())
        .count();
    if !key_ready || searchable == 0 {
        writeln!(out, "{}", palette.bold("Get started"))?;
        let step = |done: bool, current: bool| {
            if done {
                palette.ok("✓")
            } else if current {
                palette.cmd("→")
            } else {
                palette.dim("·")
            }
        };
        let rows = [
            (
                key_ready,
                "Save your Gemini key",
                "cerul auth set".to_owned(),
                if key_ready {
                    models.key.summary(palette)
                } else {
                    palette.dim("get one at https://aistudio.google.com/apikey")
                },
            ),
            (
                searchable > 0,
                "Index a video",
                "cerul index ./video.mp4".to_owned(),
                if indexed > 0 && searchable == 0 {
                    palette.warn("indexing incomplete, run it again")
                } else {
                    palette.dim("screen text, speech, and visual search")
                },
            ),
            (
                false,
                "Search it",
                "cerul search \"a person holding a cup\"".to_owned(),
                palette.dim("or --text \"exact words\""),
            ),
            (
                false,
                "Export clips",
                "cerul search \"...\" --save ./clips".to_owned(),
                String::new(),
            ),
        ];
        let mut current_marked = false;
        for (number, (done, title, cmd, note)) in rows.iter().enumerate() {
            let current = !done && !current_marked;
            if current {
                current_marked = true;
            }
            writeln!(
                out,
                "  {} {}. {:<22} {:<40} {}",
                step(*done, current),
                number + 1,
                title,
                palette.cmd(cmd),
                note
            )?;
        }
        writeln!(out)?;
        if !key_ready {
            writeln!(
                out,
                "{}",
                palette.dim(
                    "Skipping step 1 is fine: cerul index asks for the key when it first needs it."
                )
            )?;
        }
    } else {
        let videos = match indexed {
            1 => "1 video indexed".to_owned(),
            n => format!("{n} videos indexed"),
        };
        writeln!(
            out,
            "Workspace  {}  ·  {videos}  ·  {}",
            tilde(&status.workspace),
            models.key.summary(palette)
        )?;
        writeln!(out)?;
        let steps = [
            ("cerul index ./video.mp4", "add a video"),
            ("cerul search \"a person holding a cup\"", "find moments"),
            (
                "cerul search \"...\" --save ./clips",
                "export matching clips",
            ),
        ];
        next_steps(out, palette, &steps)?;
        writeln!(out)?;
    }
    writeln!(
        out,
        "{}",
        palette.dim("cerul status for details · cerul --help for all commands")
    )
}

pub fn tilde(path: &Path) -> String {
    if let Some(rest) = std::env::var_os("HOME").and_then(|home| path.strip_prefix(home).ok()) {
        return format!("~/{}", rest.display());
    }
    path.display().to_string()
}

fn checks(episode: &cerul::status::EpisodeStatus, palette: &Palette) -> String {
    let mark = |ready: bool| {
        if ready {
            palette.ok("✓")
        } else {
            palette.dim("–")
        }
    };
    let has = |name: &str| episode.annotations.iter().any(|a| a == name);
    let search = episode.embeddings.iter().any(|e| e.complete);
    let semantic = episode
        .annotations
        .iter()
        .any(|a| a.starts_with("semantic"));
    let mut parts = vec![
        format!("screen text {}", mark(has("screen_text"))),
        format!("speech {}", mark(has("transcript"))),
        format!("search {}", mark(search)),
    ];
    if semantic {
        parts.push(format!("annotations {}", mark(true)));
    }
    if let Some(failed) = episode.embeddings.iter().find(|e| !e.complete) {
        parts.push(palette.warn(&format!(
            "search incomplete{}",
            failed
                .error
                .as_deref()
                .map(|e| format!(": {e}"))
                .unwrap_or_default()
        )));
    }
    if !episode.media_present {
        parts.push(palette.warn("media missing"));
    }
    parts.join("   ")
}

pub fn status(
    out: &mut dyn Write,
    palette: &Palette,
    status: &Status,
    models: &ModelSummary,
    probed: bool,
) -> io::Result<()> {
    writeln!(
        out,
        "Workspace  {}   ·   cerul {}",
        tilde(&status.workspace),
        status.version
    )?;
    writeln!(out)?;
    writeln!(
        out,
        "{}",
        palette.bold(&format!("Videos ({})", status.episodes.len()))
    )?;
    if status.episodes.is_empty() {
        writeln!(
            out,
            "  none yet   {}",
            palette.dim("add one with: cerul index ./video.mp4")
        )?;
    }
    let width = status
        .episodes
        .iter()
        .map(|e| measure_text_width(&file_name(&e.media)))
        .max()
        .unwrap_or(0)
        .min(40);
    for episode in &status.episodes {
        let name = truncate_str(&file_name(&episode.media), 40, "…").to_string();
        let pad = " ".repeat(width.saturating_sub(measure_text_width(&name)));
        writeln!(out, "  {name}{pad}   {}", checks(episode, palette))?;
    }
    writeln!(out)?;
    writeln!(out, "{}", palette.bold("Models"))?;
    let dims = models
        .embedding_dims
        .map(|d| format!(" ({d}d)"))
        .unwrap_or_default();
    writeln!(
        out,
        "  Gemini   search {}{}   ·   speech & vision {}",
        models.embedding, dims, models.transcription
    )?;
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
        palette.dim("not checked · run cerul status --providers to verify")
    };
    writeln!(
        out,
        "           {}   ·   {check}",
        models.key.summary(palette)
    )?;
    for (name, provider) in &status.providers {
        if let Some(error) = &provider.error {
            writeln!(out, "           {} {name}: {error}", palette.warn("!"))?;
        }
    }
    writeln!(out)?;
    let ocr = match status.capabilities.get("ocr") {
        Some(Some(true)) => "screen text OCR runs locally",
        _ => "screen text OCR unavailable",
    };
    writeln!(
        out,
        "{}",
        palette.dim(&format!(
            "Storage  indexes live beside your videos (*.cerul) · {} vector space{} cached · {ocr}",
            status.spaces.len(),
            if status.spaces.len() == 1 { "" } else { "s" }
        ))
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

pub fn index_plan(
    out: &mut dyn Write,
    palette: &Palette,
    paths: &[PathBuf],
    no_ocr: bool,
    no_audio: bool,
) -> io::Result<()> {
    let names: Vec<String> = paths.iter().map(|path| file_name(path)).collect();
    writeln!(
        out,
        "{} {}",
        palette.bold("Indexing"),
        truncate_str(&names.join(", "), 60, "…")
    )?;
    let mut stations = Vec::new();
    if !no_ocr {
        stations.push(format!("screen text {}", palette.dim("(local)")));
    }
    if !no_audio {
        stations.push(format!("speech {}", palette.dim("(Gemini)")));
    }
    stations.push(format!("search index {}", palette.dim("(Gemini)")));
    writeln!(out, "  {}", stations.join(&palette.dim("  ·  ")))?;
    writeln!(out)
}

pub fn index(
    out: &mut dyn Write,
    palette: &Palette,
    report: &index::pipeline::Report,
    names: &BTreeMap<String, PathBuf>,
) -> io::Result<()> {
    let name_of = |episode: &index::pipeline::EpisodeResult| {
        names
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
        let name = name_of(episode);
        let errors: Vec<&String> = episode.streams.iter().flat_map(|s| &s.errors).collect();
        let indexed = episode.streams.iter().any(|s| s.indexed);
        if errors.is_empty() && indexed {
            ready += 1;
            if compact {
                continue;
            }
            writeln!(
                out,
                "{} {} ready to search   {}",
                palette.ok("✓"),
                palette.bold(&name),
                palette.dim(&format!("index: {}", tilde(&episode.sidecar)))
            )?;
        } else if indexed {
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
                palette
                    .dim("what finished is searchable now; re-run cerul index to retry the rest")
            )?;
        } else {
            writeln!(out, "{} {} failed", palette.err("✗"), palette.bold(&name))?;
            for error in errors {
                writeln!(out, "    {error}")?;
            }
        }
    }
    if ready > 1 && ready == report.episodes.len() {
        writeln!(
            out,
            "{} {} ready to search   {}",
            palette.ok("✓"),
            palette.bold(&format!("{ready} videos")),
            palette.dim("indexes saved beside your videos")
        )?;
    }
    if ready > 0 {
        writeln!(out)?;
        writeln!(out, "{}", palette.bold("Try"))?;
        next_steps(
            out,
            palette,
            &[
                ("cerul search \"what you're looking for\"", "find moments"),
                ("cerul search \"...\" --save ./clips", "export clips"),
            ],
        )?;
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
    let separator = palette.dim("  ·  ");
    writeln!(
        out,
        "🔍 {} for {subject}",
        palette.bold(&format!(
            "{} result{}",
            report.hits.len(),
            if report.hits.len() == 1 { "" } else { "s" }
        ))
    )?;
    writeln!(out)?;
    // One card per video: the same recording matched at several moments is one
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
    let rule = |mark: &str| palette.dim(mark);
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
        let count = if moments.len() > 1 {
            format!(
                "{}{}",
                separator,
                palette.dim(&format!("{} moments", moments.len()))
            )
        } else {
            String::new()
        };
        writeln!(out, "  {} {title}{count}", rule("┌"))?;
        for (index, hit) in moments {
            writeln!(out, "  {}", rule("│"))?;
            let mut meta = Vec::new();
            if let Some(score) = hit.score {
                meta.push(format!(
                    "📊 {}",
                    palette.ok(&format!("{:.0}% match", (score * 100.).clamp(0., 100.)))
                ));
            }
            meta.push(format!(
                "🕐 {}",
                palette.dim(&format!("{} → {}", clock(hit.start_us), clock(hit.end_us)))
            ));
            meta.push(format!("🎬 {}", palette.dim(kind_label(hit.matched))));
            writeln!(
                out,
                "  {}  {}  {}",
                rule("│"),
                palette.cmd(&format!("[{}]", index + 1)),
                meta.join(&separator).trim_end()
            )?;
            if let Some(preview) = hit
                .preview
                .as_deref()
                .filter(|_| context.terminal.images.is_some())
            {
                write!(out, "  {}       ", rule("│"))?;
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
                    writeln!(
                        out,
                        "  {}      {}",
                        rule("│"),
                        palette.dim(&format!("\"{line}\""))
                    )?;
                }
            }
            if let Some(path) = hit.media.as_deref() {
                let (label, target) = context.player.open(path, hit.start_us);
                writeln!(
                    out,
                    "  {}       🔗 {}",
                    rule("│"),
                    context.terminal.link(&palette.cmd(&label), &target)
                )?;
            }
            if let Some(clip) = hit.clip.as_deref() {
                writeln!(
                    out,
                    "  {}       🎞 {}   {}",
                    rule("│"),
                    context.terminal.file_link(&palette.cmd("Saved clip"), clip),
                    palette.dim(&tilde(clip))
                )?;
            }
        }
        match first.media.as_deref() {
            Some(path) => writeln!(out, "  {} {}", rule("└"), palette.dim(&tilde(path)))?,
            None => writeln!(out, "  {}", rule("└"))?,
        }
        writeln!(out)?;
    }
    if let Some((number, _)) = report
        .hits
        .iter()
        .enumerate()
        .find(|(_, hit)| hit.media.is_some())
    {
        writeln!(
            out,
            "{}",
            palette.dim(&format!(
                "Open a moment in your player: cerul open {}",
                number + 1
            ))
        )?;
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
        writeln!(out)?;
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
        writeln!(out)?;
    }
    if let Some(directory) = &context.saved_to {
        writeln!(
            out,
            "{} clip{} saved to {}",
            report.hits.iter().filter(|h| h.clip.is_some()).count(),
            if report.hits.len() == 1 { "" } else { "s" },
            directory.display()
        )?;
    } else if let Some(query) = &context.query {
        let flag = if context.text { " --text" } else { "" };
        next_steps(
            out,
            palette,
            &[(
                &format!("cerul search{flag} \"{query}\" --save ./clips"),
                "save these moments as clips",
            )],
        )?;
    }
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

pub fn annotate(
    out: &mut dyn Write,
    palette: &Palette,
    report: &annotate::pipeline::Report,
    names: &BTreeMap<String, PathBuf>,
) -> io::Result<()> {
    if report.dry_run {
        writeln!(out, "{} nothing was written.", palette.bold("Dry run:"))?;
    }
    if report.modules.is_empty() && report.writebacks.is_empty() {
        writeln!(out, "Nothing to annotate.")?;
        return Ok(());
    }
    for module in &report.modules {
        let name = names
            .get(&module.episode)
            .map(|p| file_name(p))
            .unwrap_or_else(|| module.episode.clone());
        let label = station_label(&module.annotation);
        match (&module.error, module.complete) {
            (None, true) => writeln!(
                out,
                "{} {}  {label}  {}",
                palette.ok("✓"),
                palette.bold(&name),
                palette.dim(&format!("{} records", module.records))
            )?,
            (None, false) => writeln!(
                out,
                "{} {}  {label}  {}",
                palette.warn("!"),
                palette.bold(&name),
                palette.dim(&format!("incomplete, {} records so far", module.records))
            )?,
            (Some(error), _) => {
                writeln!(out, "{} {}  {label}", palette.err("✗"), palette.bold(&name))?;
                writeln!(out, "    {error}")?;
            }
        }
    }
    for writeback in &report.writebacks {
        match &writeback.error {
            None => writeln!(
                out,
                "{} wrote LeRobot dataset {}",
                palette.ok("✓"),
                tilde(&writeback.destination)
            )?,
            Some(error) => {
                writeln!(
                    out,
                    "{} LeRobot writeback failed for {}",
                    palette.err("✗"),
                    tilde(&writeback.source)
                )?;
                writeln!(out, "    {error}")?;
            }
        }
    }
    if report.partial {
        writeln!(out)?;
        writeln!(
            out,
            "{}",
            palette.dim("Finished windows are saved; re-run the same command to resume the rest.")
        )?;
    }
    Ok(())
}

/// Human error line plus a hint on how to recover.
pub fn error(palette: &Palette, code: u8, message: &str, hint: Option<&str>) -> String {
    let mut text = format!("{} {message}", palette.err("error:"));
    if let Some(hint) = hint {
        text.push_str(&format!("\n  {} {hint}", palette.dim("hint:")));
    }
    if code == 5 {
        text = format!("{} {message}", palette.warn("cancelled:"));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use cerul::search::{Hit, Report};

    fn hit() -> Hit {
        Hit {
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
        assert!(text.contains("74% match"), "{text}");
        assert!(text.contains("00:12 → 00:19"), "{text}");
        assert!(text.contains("speech"), "{text}");
        assert!(text.contains("perfecting this harness"), "{text}");
        assert!(text.contains("Open video"), "{text}");
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
        assert_eq!(text.matches("a demo.mp4").count(), 2, "{text}");
        assert!(text.contains("2 moments"), "{text}");
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

    #[test]
    fn wrapping_breaks_on_words_and_keeps_every_one() {
        let lines = wrap("the quick brown fox jumps", 11);
        assert_eq!(lines, ["the quick", "brown fox", "jumps"]);
        assert!(wrap("", 10).is_empty());
    }
}
