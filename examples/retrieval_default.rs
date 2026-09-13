//! Lightweight replay of the production recipe over cached diagnostic candidates.
//! Reads JSONL from stdin, writes JSONL to stdout, and makes no model calls.
use anyhow::{Context, Result, ensure};
use cerul::{index::lexical, search::fusion};
use serde_json::{Value, json};
use std::io::{self, BufRead};

fn main() -> Result<()> {
    for line in io::stdin().lock().lines() {
        let line = line?;
        let run: Value = serde_json::from_str(&line)?;
        ensure!(
            run["schema"] == "retrieval-diagnostics/1",
            "unsupported diagnostic input"
        );
        let query = run["query"].as_str().context("missing query")?;
        let mut candidates = Vec::new();
        for (track, rows) in run["tracks"].as_object().context("missing tracks")? {
            for row in rows.as_array().context("invalid candidate list")? {
                let mut row = row.as_object().context("invalid candidate")?.clone();
                ensure!(!row.contains_key("track"), "unexpected track override");
                row.insert("track".into(), json!(track));
                let candidate: fusion::Candidate = serde_json::from_value(Value::Object(row))?;
                if candidate.track != fusion::Track::Lexical
                    || lexical::covers_query(query, candidate.text.as_deref().unwrap_or(""))
                {
                    candidates.push(candidate);
                }
            }
        }
        let baseline = fusion::fuse(&candidates, &fusion::Recipe::RawMax { threshold: None })?;
        let default = fusion::fuse(&candidates, &fusion::default_recipe(None))?;
        let thresholded = fusion::fuse(&candidates, &fusion::default_recipe(Some(0.6)))?;
        println!(
            "{}",
            serde_json::to_string(&json!({
                "query_id":run["query_id"], "space_id":run["space_id"], "model_calls":0,
                "input_hash":cerul::storage::sha256_hex(line.as_bytes()),
                "recipe":fusion::DEFAULT_RECIPE, "parameters":fusion::default_recipe(None),
                "baseline":baseline.first(), "default":default.first(),
                "threshold_0_6":thresholded.first(),
                "scope":"diagnostic fusion replay; excludes index lookup and final duplicate suppression"
            }))?
        );
    }
    Ok(())
}
