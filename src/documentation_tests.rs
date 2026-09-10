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
