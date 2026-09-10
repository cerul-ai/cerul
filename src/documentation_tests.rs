//! Parse documentation examples without executing commands or calling providers.

use super::{Cli, Command};
use clap::Parser;
use serde::Deserialize;

#[derive(Deserialize)]
struct Example {
    path: String,
    line: usize,
    args: Vec<String>,
}

fn stopped(error: &str) -> cerul::annotate::pipeline::Retry {
    let report = cerul::annotate::pipeline::Report {
        modules: vec![cerul::annotate::pipeline::ModuleResult {
            episode: "demo/0".into(),
            stream: "primary".into(),
            annotation: "semantic.event".into(),
            records: 0,
            complete: false,
            error: Some(error.to_owned()),
            source: std::path::PathBuf::from("/my videos/demo.mp4"),
            dataset: false,
            path: None,
        }],
        writebacks: Vec::new(),
        partial: true,
        dry_run: false,
        retry: None,
    };
    let original: Vec<String> = [
        "/usr/local/bin/cerul",
        "annotate",
        "/my videos/demo.mp4",
        "--semantic",
        "subtask,event",
    ]
    .iter()
    .map(|argument| (*argument).to_owned())
    .collect();
    super::retry_after(&report, &original).expect("a partial run offers a retry")
}

/// The retry has to be the same run again, or it is not a retry: same paths, same
/// labels, with only the limit the provider asked for.
#[test]
fn the_retry_repeats_the_invocation_and_changes_only_the_rate() {
    let limited = stopped("gemini rate limit (429)");
    assert_eq!(
        limited.reason,
        cerul::annotate::pipeline::RetryReason::RateLimit
    );
    assert_eq!(
        limited.argv,
        [
            "cerul",
            "annotate",
            "/my videos/demo.mp4",
            "--semantic",
            "subtask,event",
            "--rpm",
            "6"
        ]
    );
    // Quoting is what makes the printed command survive a copy into a shell.
    assert_eq!(
        crate::render::shell_command(&limited.argv),
        "cerul annotate '/my videos/demo.mp4' --semantic subtask,event --rpm 6"
    );
    // Nothing else earns a rate cap, and no failure earns --recompute.
    let other = stopped("connection reset");
    assert_eq!(
        other.reason,
        cerul::annotate::pipeline::RetryReason::Incomplete
    );
    assert!(!other.argv.iter().any(|argument| argument == "--rpm"));
    assert!(!other.argv.iter().any(|argument| argument == "--recompute"));
}

/// The committed skill and the one this build installs have to be the same file:
/// somebody reading it on GitHub is being told how this version behaves.
#[test]
fn the_published_skill_matches_what_this_build_installs() {
    let committed = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/cerul/SKILL.md"),
    )
    .expect("skills/cerul/SKILL.md is part of the repository");
    assert_eq!(
        committed,
        super::skill_text(),
        "run `cerul skill --print > skills/cerul/SKILL.md` after changing commands or prompts/skill.md"
    );
    assert!(
        committed.starts_with("---\nname: cerul\n"),
        "skill needs front matter"
    );
    assert!(committed.contains(super::SKILL_MARKER));
}

#[test]
fn public_command_examples_match_cli() {
    let output = std::process::Command::new("python3")
        .args(["scripts/check-docs.py", "--commands"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("documentation checks require Python 3");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let examples: Vec<Example> = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        !examples.is_empty(),
        "no documented commands were discovered"
    );
    for example in examples {
        let location = format!("{}:{}", example.path, example.line);
        match Cli::try_parse_from(&example.args) {
            Ok(cli) => {
                if let Some(Command::Search(search)) = cli.command {
                    assert!(
                        search.extra_words.is_empty(),
                        "{location}: unquoted search terms"
                    );
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
                ) => {}
            Err(error) => panic!("{location}: {error}"),
        }
    }
}
