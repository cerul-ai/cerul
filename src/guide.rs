//! Lightweight guidance for an under-specified command.
//!
//! Nothing here decides what a command does. Each screen only completes the
//! arguments a person did not type, and the completed argument list then goes
//! through the ordinary parser, so a guided run and a typed one are the same
//! run. Guidance appears only when a person is at both ends of the process.
//! Everything it draws goes to stderr, leaving results on stdout alone.
use crate::render::{self, Palette};
use console::{Key, Term, measure_text_width, truncate_str};
use std::{
    ffi::OsString,
    fs,
    io::{self, IsTerminal},
    path::{Path, PathBuf},
};

/// Extensions the discovery step accepts. Listing anything else would offer a
/// choice that fails as soon as it is made.
const VIDEO: &[&str] = &[
    "mp4", "mov", "mkv", "webm", "avi", "m4v", "mpeg", "mpg", "mts", "m2ts",
];
/// Enough choices to see the directory, few enough to read without scrolling.
const LISTED: usize = 12;

/// Flags that mean a machine is reading, or that an answer was already given.
/// Their presence rules out every prompt, whatever else was typed.
fn machine_facing(arguments: &[OsString]) -> bool {
    arguments.iter().any(|argument| {
        matches!(
            argument.to_string_lossy().as_ref(),
            "--json"
                | "--yes"
                | "-y"
                | "--quiet"
                | "-q"
                | "--dry-run"
                | "--help"
                | "-h"
                | "--version"
                | "-V"
        )
    })
}

/// A person is here when both the keys they press and the screen they read
/// belong to a terminal.
fn interactive() -> bool {
    io::stdin().is_terminal() && io::stderr().is_terminal()
}

pub struct Choice<T> {
    pub label: String,
    pub note: String,
    pub value: T,
}
impl<T> Choice<T> {
    pub fn new(label: impl Into<String>, note: impl Into<String>, value: T) -> Self {
        Self {
            label: label.into(),
            note: note.into(),
            value,
        }
    }
}

/// Arrow keys over a short list. Returns `None` when the person leaves, which
/// is a normal outcome and never an error.
pub fn select<T: Clone>(
    palette: &Palette,
    title: &str,
    choices: &[Choice<T>],
) -> io::Result<Option<T>> {
    if choices.is_empty() {
        return Ok(None);
    }
    let term = Term::stderr();
    term.hide_cursor()?;
    // Whatever happens inside, the cursor comes back: a terminal left without
    // one outlives this process and there is no undo for it.
    let chosen = run_menu(&term, palette, title, choices);
    term.show_cursor()?;
    match chosen? {
        // What was asked and what was answered stay on screen; the list does not.
        Some(index) => {
            term.write_line(&format!(
                "{} {}",
                palette.dim(title),
                palette.bold(&choices[index].label)
            ))?;
            Ok(Some(choices[index].value.clone()))
        }
        None => Ok(None),
    }
}

fn run_menu<T>(
    term: &Term,
    palette: &Palette,
    title: &str,
    choices: &[Choice<T>],
) -> io::Result<Option<usize>> {
    let width = choices
        .iter()
        .map(|choice| measure_text_width(&choice.label))
        .max()
        .unwrap_or(0);
    let mut cursor = 0usize;
    let mut drawn = 0usize;
    let chosen = loop {
        if drawn > 0 {
            term.clear_last_lines(drawn)?;
        }
        // A line that wraps is two lines to the terminal and one to this loop,
        // which would clear the wrong rows on the next redraw.
        let columns = (term.size().1 as usize).max(20);
        let line = |text: &str| term.write_line(truncate_str(text, columns - 1, "…").as_ref());
        line(&palette.bold(title))?;
        for (index, choice) in choices.iter().enumerate() {
            let here = index == cursor;
            let pad = " ".repeat(width - measure_text_width(&choice.label) + 3);
            let label = match here {
                true => palette.bold(&choice.label),
                false => choice.label.clone(),
            };
            let note = match choice.note.is_empty() {
                true => String::new(),
                false => format!("{pad}{}", palette.dim(&choice.note)),
            };
            line(&format!(
                "{} {label}{note}",
                if here { palette.cmd("❯") } else { " ".into() }
            ))?;
        }
        line(&palette.dim("↑↓ move · Enter select · Esc leave"))?;
        drawn = choices.len() + 2;
        match term.read_key() {
            Ok(Key::ArrowUp) => cursor = cursor.checked_sub(1).unwrap_or(choices.len() - 1),
            Ok(Key::ArrowDown) => cursor = (cursor + 1) % choices.len(),
            Ok(Key::Enter) => break Some(cursor),
            // Leaving is a normal answer, and so is a terminal that cannot
            // report keys at all: both mean this menu asked for nothing.
            Ok(Key::Escape) | Ok(Key::CtrlC) => break None,
            Err(_) => break None,
            _ => {}
        }
    };
    term.clear_last_lines(drawn)?;
    Ok(chosen)
}

/// One typed line. An empty answer means the person changed their mind.
pub fn input(palette: &Palette, title: &str, note: &str) -> io::Result<Option<String>> {
    let term = Term::stderr();
    term.write_line(&palette.bold(title))?;
    if !note.is_empty() {
        term.write_line(&palette.dim(note))?;
    }
    let answer = match term.read_line() {
        Ok(answer) => answer,
        Err(error) if error.kind() == io::ErrorKind::Interrupted => return Ok(None),
        Err(error) => return Err(error),
    };
    let answer = answer.trim().to_owned();
    Ok((!answer.is_empty()).then_some(answer))
}

/// Videos and datasets in one directory, newest first. The current directory
/// only: walking a tree would offer choices from somewhere the person is not.
fn candidates(directory: &Path) -> Vec<(PathBuf, String)> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut found: Vec<(std::time::SystemTime, PathBuf, String)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let modified = metadata.modified().unwrap_or(std::time::UNIX_EPOCH);
        if metadata.is_dir() {
            // A LeRobot dataset is a directory with a meta/ directory in it.
            if path.join("meta").is_dir() {
                found.push((modified, path, "LeRobot dataset".into()));
            }
            continue;
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_lowercase();
        if VIDEO.contains(&extension.as_str()) {
            let bytes = metadata.len() as f64;
            let size = if bytes >= 1_048_576. {
                format!("{:.0} MB", bytes / 1_048_576.)
            } else {
                format!("{:.0} KB", bytes / 1024.)
            };
            found.push((modified, path, size));
        }
    }
    found.sort_by_key(|(modified, _, _)| std::cmp::Reverse(*modified));
    found.truncate(LISTED);
    found
        .into_iter()
        .map(|(_, path, note)| (path, note))
        .collect()
}

/// Which video or dataset to work on: the files that are here, or a typed path.
fn pick_input(palette: &Palette, title: &str) -> io::Result<Option<String>> {
    let directory = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let found = candidates(&directory);
    if found.is_empty() {
        return input(
            palette,
            title,
            "No video or dataset in this directory. Type a path, or press Enter to leave.",
        );
    }
    let mut choices: Vec<Choice<Option<String>>> = found
        .iter()
        .map(|(path, note)| {
            Choice::new(
                format!("./{}", render::file_name(path)),
                note.clone(),
                Some(format!("./{}", render::file_name(path))),
            )
        })
        .collect();
    choices.push(Choice::new("Type a path…", "somewhere else", None));
    match select(palette, title, &choices)? {
        Some(Some(path)) => Ok(Some(path)),
        Some(None) => input(palette, "Path to a video, folder, or dataset", ""),
        None => Ok(None),
    }
}

/// Turns answers into the command that runs them, shown before it runs so the
/// next run can be typed instead of answered. `typed` is everything the person
/// already wrote, so a global flag they chose appears in the command too.
fn show_command(palette: &Palette, typed: &[String], extra: &[String]) -> io::Result<()> {
    let term = Term::stderr();
    let argv: Vec<String> = std::iter::once("cerul".to_owned())
        .chain(typed.iter().cloned())
        .chain(extra.iter().cloned())
        .collect();
    term.write_line("")?;
    term.write_line(&format!("  {}", palette.cmd(&render::shell_command(&argv))))?;
    Ok(())
}

fn confirm(palette: &Palette, typed: &[String], extra: &[String]) -> io::Result<Option<bool>> {
    show_command(palette, typed, extra)?;
    let choices = [
        Choice::new("Start", "", Some(false)),
        Choice::new(
            "Preview only",
            "--dry-run: no writes, no model calls",
            Some(true),
        ),
        Choice::new("Cancel", "", None),
    ];
    Ok(select(palette, "Ready", &choices)?.flatten())
}

/// Where an invocation came from, which decides how much of it may be missing.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    /// Typed at a prompt. Only a bare `cerul <command>` is completed, so a
    /// half-written command still gets the parser's own error.
    Typed,
    /// Composed by the home menu, which appends a command to what was typed and
    /// so may carry the global flags that came with it.
    Menu,
}

/// Whether a person could be asked something here: one at the keyboard, one
/// reading the screen, and no flag that means an answer was already given.
pub fn asks(arguments: &[OsString]) -> bool {
    !machine_facing(arguments) && interactive()
}

/// What guidance did with an invocation.
pub enum Guided {
    /// The arguments a person answered their way to.
    Completed(Vec<OsString>),
    /// Nothing to ask: a complete command, a machine reading, or no terminal.
    Untouched,
    /// The person left. Nothing was asked for, so nothing should happen.
    Left,
}

/// Fills in what a bare command left out, or leaves the arguments alone. The
/// result is parsed by the ordinary parser, so nothing here can invent
/// behaviour the command line cannot express.
pub fn complete(arguments: Vec<OsString>, palette: &Palette, entry: Entry) -> Guided {
    // Only a command with nothing after it is under-specified in a way guidance
    // can fix. A menu adds its command to flags that were already typed, so its
    // invocation is longer without being any less bare.
    let bare = match entry {
        Entry::Typed => arguments.len() == 2,
        Entry::Menu => true,
    };
    if !bare || !asks(&arguments) {
        return Guided::Untouched;
    }
    let Some(command) = arguments.last() else {
        return Guided::Untouched;
    };
    // Everything already typed, so the command a screen shows is the command
    // that runs: a global flag chosen before the subcommand belongs in it.
    let typed: Vec<String> = arguments
        .iter()
        .skip(1)
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect();
    let extra = match command.to_string_lossy().as_ref() {
        "annotate" => annotate(palette, &typed),
        "index" => index(palette, &typed),
        "search" => search(palette),
        _ => return Guided::Untouched,
    };
    match extra {
        Some(extra) => {
            let mut completed = arguments;
            completed.extend(extra.into_iter().map(OsString::from));
            Guided::Completed(completed)
        }
        // Backing out of a question is an answer: it means no command at all,
        // not the usage error a bare command would otherwise print.
        None => Guided::Left,
    }
}

/// Real episode indices and camera keys, or nothing when the dataset cannot be
/// read. A dataset this build does not support should fail in the run, with its
/// own error, rather than be hidden behind a menu that cannot offer anything.
fn dataset(path: &str) -> Option<Vec<cerul::episode::Episode>> {
    let root = Path::new(path);
    if !root.join("meta/info.json").is_file() {
        return None;
    }
    cerul::lerobot::read(root).ok().filter(|e| !e.is_empty())
}

/// Which episode and which cameras, from what the dataset actually contains.
fn lerobot_scope(palette: &Palette, episodes: &[cerul::episode::Episode]) -> Option<Vec<String>> {
    let mut argv = Vec::new();
    // Indexes are not always consecutive and do not always start at zero, so the
    // first one has to come from the dataset rather than from an assumption.
    let first = episodes.first()?.local_id.clone();
    let scope = [
        Choice::new(
            format!("Episode {first} only"),
            "start small: one episode is one set of model calls",
            Some(first.clone()),
        ),
        Choice::new(
            format!("All {} episodes", episodes.len()),
            "every episode in the dataset",
            None,
        ),
    ];
    if let Some(only) = select(palette, "Which episodes?", &scope).ok()?? {
        argv.push("--only".to_owned());
        argv.push(only);
    }
    let cameras: Vec<&str> = episodes
        .first()
        .map(|episode| {
            episode
                .streams
                .iter()
                .filter(|stream| matches!(stream, cerul::episode::Stream::Video { .. }))
                .map(|stream| stream.id())
                .collect()
        })
        .unwrap_or_default();
    if cameras.len() > 1 {
        let primary = &episodes.first()?.time.reference;
        let choices = [
            Choice::new(format!("Primary camera ({primary})"), "", false),
            Choice::new(
                format!("All {} cameras", cameras.len()),
                "one set of results per camera",
                true,
            ),
        ];
        if select(palette, "Which cameras?", &choices).ok()?? {
            argv.push("--streams".to_owned());
            argv.push("all".to_owned());
        }
    }
    Some(argv)
}

/// What a person has in front of them, named the way they would describe it
/// rather than by the label names the flag happens to use.
fn annotate(palette: &Palette, typed: &[String]) -> Option<Vec<String>> {
    let path = pick_input(palette, "Annotate · which video or dataset?").ok()??;
    let episodes = dataset(&path);
    let robot = Choice::new(
        "A robot or first-person demonstration",
        "subtask, event, interaction, state",
        "subtask,event,interaction,state",
    );
    // Naming the labels rather than leaning on the default: a dataset's own
    // default is all seven types, which is not what this choice promises.
    let general = Choice::new(
        "A general video",
        "task, subtask, flag",
        "task,subtask,flag",
    );
    let everything = Choice::new(
        "Everything",
        "all seven semantic types",
        "task,subtask,event,interaction,state,flag,progress",
    );
    // A dataset's own default is every semantic type, so that choice leads here.
    let presets = match episodes.is_some() {
        true => [
            Choice::new(
                "Everything",
                "the LeRobot default: all seven types",
                "default",
            ),
            robot,
            general,
        ],
        false => [robot, general, everything],
    };
    let items = select(palette, "What is in it?", &presets).ok()??;
    let mut extra = vec![path, "--semantic".to_owned()];
    if items != "default" {
        extra.push(items.to_owned());
    }
    if let Some(episodes) = &episodes {
        extra.extend(lerobot_scope(palette, episodes)?);
    }
    if confirm(palette, typed, &extra).ok()?? {
        extra.push("--dry-run".into());
    }
    Some(extra)
}

fn index(palette: &Palette, typed: &[String]) -> Option<Vec<String>> {
    let path = pick_input(palette, "Index · which video or folder?").ok()??;
    let extra = vec![path];
    show_command(palette, typed, &extra).ok()?;
    Some(extra)
}

/// Search asks one question, because a query is the only thing it needs.
fn search(palette: &Palette) -> Option<Vec<String>> {
    let query = input(
        palette,
        "Search · what do you want to find?",
        "Describe a moment, or press Enter to leave.",
    )
    .ok()??;
    Some(vec![query])
}

/// Offered under the home screen, so running `cerul` still shows what it always
/// showed and the menu is an addition rather than a replacement. The choice is
/// appended to what was typed, so `--workspace` and the rest survive it.
pub fn home_menu(palette: &Palette, arguments: &[OsString]) -> Option<Vec<OsString>> {
    if !asks(arguments) {
        return None;
    }
    let choices = [
        Choice::new("Index a video so it can be searched", "", Some("index")),
        Choice::new("Search indexed videos", "", Some("search")),
        Choice::new(
            "Annotate actions in a video or LeRobot dataset",
            "",
            Some("annotate"),
        ),
        Choice::new("Leave", "", None),
    ];
    let command = select(palette, "What do you want to do?", &choices).ok()??;
    let mut next = arguments.to_vec();
    next.push(OsString::from(command?));
    Some(next)
}
