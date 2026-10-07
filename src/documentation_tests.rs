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
