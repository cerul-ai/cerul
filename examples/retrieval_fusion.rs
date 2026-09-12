//! Replay explicit fusion recipes over saved candidates, with zero model calls.
use anyhow::{Context, Result, ensure};
use cerul::{
    search::fusion::{self, Candidate, Recipe, Track},
    storage,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    time::Instant,
};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Suite {
    schema: String,
    space_id: String,
    /// "diagnostic" permits exploratory replay only, never held-out scoring.
    parameter_split: String,
    parameter_episodes: Vec<String>,
    /// Clips from the same original video must share a source group.
    source_groups: BTreeMap<String, String>,
    recipes: BTreeMap<String, Recipe>,
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 3,
        "usage: retrieval_fusion CANDIDATES.jsonl RECIPES.json OUTPUT.jsonl"
    );
    let suite: Suite = serde_json::from_slice(&fs::read(&args[1])?)?;
    ensure!(
        suite.schema == "retrieval-fusion-recipes/1",
        "unsupported recipe schema"
    );
    ensure!(
        matches!(suite.parameter_split.as_str(), "diagnostic" | "tune"),
        "parameters must come from diagnostic or tuning data"
    );
    ensure!(
        !suite.parameter_episodes.is_empty(),
        "record the parameter source episodes"
    );
    let parameter_groups: BTreeSet<_> = suite
        .parameter_episodes
        .iter()
        .map(|episode| {
            suite
                .source_groups
                .get(episode)
                .context("parameter episode is missing its source group")
        })
        .collect::<Result<_>>()?;
    ensure!(
        !suite.recipes.is_empty() && suite.recipes.keys().all(|key| !key.trim().is_empty()),
        "recipes need unique nonempty names"
    );
    ensure!(
        suite.source_groups.values().all(|s| !s.is_empty()),
        "source groups cannot be empty"
    );
    for recipe in suite.recipes.values() {
        recipe.validate()?;
    }
    let input = fs::read_to_string(&args[0])?;
    let suite_hash = storage::cache_key(&(fusion::ALGORITHM, &suite))?;
    let mut seen = BTreeSet::new();
    let mut splits: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    splits.insert(
        suite.parameter_split.clone(),
        parameter_groups.iter().map(|s| (*s).clone()).collect(),
    );
    let mut output = String::new();
    for line in input.lines().filter(|line| !line.trim().is_empty()) {
        let run: Value = serde_json::from_str(line)?;
        ensure!(
            run["schema"] == "retrieval-diagnostics/1",
            "unsupported diagnostic schema"
        );
        ensure!(
            run["space_id"] == suite.space_id,
            "recipe and candidate embedding spaces differ"
        );
        let query_id = run["query_id"].as_str().context("missing query ID")?;
        ensure!(
            !query_id.is_empty() && seen.insert(query_id.to_owned()),
            "invalid or duplicate query ID"
        );
        let split = run["split"].as_str().context("missing split")?;
        ensure!(
            matches!(split, "diagnostic" | "tune" | "holdout"),
            "invalid split"
        );
        ensure!(
            split != "holdout" || suite.parameter_split == "tune",
            "diagnostic parameters cannot score held-out data"
        );
        let episodes: Vec<String> = serde_json::from_value(run["episodes"].clone())?;
        ensure!(!episodes.is_empty(), "missing corpus scope");
        for episode in &episodes {
            let source = suite
                .source_groups
                .get(episode)
                .context("corpus episode is missing its source group")?;
            ensure!(
                split != "holdout" || !parameter_groups.contains(source),
                "parameter sources overlap held-out data"
            );
            ensure!(
                suite.parameter_split != "diagnostic" || split == "diagnostic",
                "diagnostic parameters are exploratory only"
            );
            splits
                .entry(split.into())
                .or_default()
                .insert(source.clone());
        }
        let tracks = run["tracks"]
            .as_object()
            .context("missing candidate tracks")?;
        let mut candidates = Vec::new();
        for (track, rows) in tracks {
            let track: Track = serde_json::from_value(json!(track))?;
            for row in rows.as_array().context("invalid candidate list")? {
                let mut row = row.as_object().context("invalid candidate row")?.clone();
                ensure!(
                    !row.contains_key("track"),
                    "candidate track must come from its list"
                );
                row.insert("track".into(), serde_json::to_value(track)?);
                let candidate: Candidate = serde_json::from_value(Value::Object(row))?;
                ensure!(
                    episodes.contains(&candidate.episode),
                    "candidate escaped query scope"
                );
                candidates.push(candidate);
            }
        }
        let mut recipes = BTreeMap::new();
        for (name, recipe) in &suite.recipes {
            let started = Instant::now();
            let moments = fusion::fuse(&candidates, recipe)?;
            recipes.insert(
                name,
                json!({"moments":moments,"fusion_ms":started.elapsed().as_secs_f64()*1000.}),
            );
        }
        output.push_str(&serde_json::to_string(&json!({
            "schema":"retrieval-fusion/1", "algorithm_version":fusion::ALGORITHM, "query_id":query_id, "split":split,
            "space_id":suite.space_id, "episodes":episodes, "input_hash":storage::sha256_hex(line.as_bytes()),
            "recipe_hash":suite_hash, "suite":suite, "model_calls":0, "recipes":recipes
        }))?);
        output.push('\n');
    }
    ensure!(!seen.is_empty(), "no diagnostic queries");
    for (left, right) in [
        ("diagnostic", "tune"),
        ("diagnostic", "holdout"),
        ("tune", "holdout"),
    ] {
        if let (Some(left), Some(right)) = (splits.get(left), splits.get(right)) {
            ensure!(
                left.is_disjoint(right),
                "source videos cannot cross evaluation splits"
            );
        }
    }
    let destination = PathBuf::from(&args[2]);
    // Validate the entire run before publishing; never overwrite earlier evidence.
    let temporary = tempfile::NamedTempFile::new_in(
        destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new(".")),
    )?;
    fs::write(temporary.path(), output)?;
    temporary.persist_noclobber(destination)?;
    Ok(())
}
