//! Acceptance harness: synthetic annotations, Rust writeback, official Python loader.
use anyhow::{Context, Result, ensure};
use cerul::{
    annotations::{AnnotationFile, Header, Model, Record},
    lerobot::{
        self,
        writer::{Assignment, write_out},
    },
};
use serde_json::json;
use std::{collections::BTreeMap, path::PathBuf, process::Command};
fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let source = PathBuf::from(args.next().context("source dataset required")?);
    let output = PathBuf::from(args.next().context("output dataset required")?);
    let python = args
        .next()
        .context("official-loader Python executable required")?;
    let episodes = lerobot::read(&source)?;
    ensure!(
        episodes.len() == 5,
        "use the five-episode acceptance fixture"
    );
    let selected: Vec<_> = episodes
        .iter()
        .filter(|episode| episode.local_id.parse::<u64>().is_ok_and(|id| id % 2 == 0))
        .collect();
    let files: Vec<_> = selected
        .iter()
        .map(|episode| AnnotationFile {
            header: Header {
                schema: "annotation/1".into(),
                name: "semantic.subtask".into(),
                episode: episode.episode_id.clone(),
                stream: episode.time.reference.clone(),
                model: Model {
                    kind: "fixture".into(),
                    name: "acceptance".into(),
                    base_url: None,
                },
                params: json!({}),
                created: "2026-09-08T00:00:00Z".into(),
                cerul_version: env!("CARGO_PKG_VERSION").into(),
                input_hash: "acceptance".into(),
                record_schema: "semantic.subtask/1".into(),
            },
            records: ["First subtask", "Second subtask"]
                .into_iter()
                .enumerate()
                .map(|(i, text)| Record {
                    id: format!("subtask-{i}"),
                    start_us: i as i64 * 2_000_000,
                    end_us: (i + 1) as i64 * 2_000_000,
                    confidence: None,
                    fields: BTreeMap::from([
                        ("text".into(), json!(text)),
                        ("index".into(), json!(i)),
                    ]),
                })
                .collect(),
        })
        .collect();
    let assignments: Vec<_> = selected
        .iter()
        .zip(&files)
        .map(|(episode, subtasks)| Assignment { episode, subtasks })
        .collect();
    write_out(&source, &output, &assignments, |stage| {
        let status = Command::new(&python)
            .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/lerobot_roundtrip.py"))
            .arg("verify")
            .arg(&source)
            .arg(stage)
            .env("HF_HUB_OFFLINE", "1")
            .env("HF_DATASETS_OFFLINE", "1")
            .status()?;
        ensure!(status.success(), "official loader rejected staged dataset");
        Ok(())
    })?;
    println!("{}", json!({"published":output}));
    Ok(())
}
