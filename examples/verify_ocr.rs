//! Run embedded OCR on acceptance images, reporting text and measured CPU wall time.
use anyhow::{Context, Result, ensure};
use std::{
    path::{Path, PathBuf},
    time::Instant,
};

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1).peekable();
    let jobs = if args.peek().is_some_and(|arg| arg == "--jobs") {
        args.next();
        args.next()
            .context("--jobs requires a count")?
            .to_str()
            .context("invalid worker count")?
            .parse::<usize>()?
    } else {
        1
    };
    if args.peek().is_some_and(|arg| arg == "--video") {
        args.next();
        let path = PathBuf::from(args.next().context("--video requires a path")?);
        ensure!(
            jobs > 0 && args.next().is_none(),
            "usage: verify_ocr [--jobs N] --video VIDEO"
        );
        let episode = cerul::index::discover::ordinary_episode(&path)?;
        let temporary = tempfile::tempdir()?;
        let start = Instant::now();
        let annotation = cerul::index::stations::screen_text(
            &episode,
            "primary",
            temporary.path(),
            true,
            jobs,
            &mut |_| {},
            &tokio_util::sync::CancellationToken::new(),
        )?;
        println!(
            "{}",
            serde_json::json!({
                "path": path, "station_elapsed_ms": start.elapsed().as_millis(),
                "jobs": jobs, "records": annotation.records,
            })
        );
        return Ok(());
    }
    let paths: Vec<PathBuf> = args.map(PathBuf::from).collect();
    ensure!(
        jobs > 0 && !paths.is_empty(),
        "usage: verify_ocr [--jobs N] IMAGE..."
    );
    if jobs == 1 {
        let mut engine = cerul::ocr::Ocr::default();
        for path in &paths {
            println!("{}", read(&mut engine, path)?);
        }
        return Ok(());
    }
    std::thread::scope(|scope| {
        let mut workers = Vec::new();
        for worker in 0..jobs.min(paths.len()) {
            let selected: Vec<_> = paths
                .iter()
                .enumerate()
                .skip(worker)
                .step_by(jobs)
                .collect();
            workers.push(scope.spawn(move || {
                let mut engine = cerul::ocr::Ocr::default();
                selected
                    .into_iter()
                    .map(|(index, path)| read(&mut engine, path).map(|row| (index, row)))
                    .collect::<Result<Vec<_>>>()
            }));
        }
        let mut rows = Vec::new();
        for worker in workers {
            rows.extend(
                worker
                    .join()
                    .map_err(|_| anyhow::anyhow!("OCR worker panicked"))??,
            );
        }
        rows.sort_by_key(|(index, _)| *index);
        for (_, row) in rows {
            println!("{row}");
        }
        Ok(())
    })
}

fn read(engine: &mut cerul::ocr::Ocr, path: &Path) -> Result<serde_json::Value> {
    let image = image::open(path)
        .with_context(|| format!("read {}", path.display()))?
        .to_rgb8();
    let start = Instant::now();
    let boxes = engine.read(&image)?;
    Ok(serde_json::json!({
        "path": path,
        "elapsed_ms": start.elapsed().as_millis(),
        "text": boxes.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join(" "),
        "boxes": boxes,
    }))
}
