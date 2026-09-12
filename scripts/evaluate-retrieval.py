#!/usr/bin/env python3
"""Score offline candidate exports against exhaustive, human-reviewed intervals."""
import argparse
import hashlib
import json
import math
import statistics
from collections import defaultdict


def iou(a, b):
    if (a["episode"], a["stream"]) != (b["episode"], b["stream"]):
        return 0.0
    overlap = max(0, min(a["end_us"], b["end_us"]) - max(a["start_us"], b["start_us"]))
    return overlap / (max(a["end_us"], b["end_us"]) - min(a["start_us"], b["start_us"]))


def percentile(values, p):
    if not values:
        return None
    return sorted(values)[max(0, math.ceil(len(values) * p) - 1)]


def evidence_kind(row, track):
    kind = row.get("kind", row.get("track", track))
    if kind == "lexical":
        return {"transcript": "speech", "screen_text": "screen"}[row["annotation"]]
    return kind


def supporting_answers(row, track, answers):
    temporal = [i for i, answer in enumerate(answers) if iou(row, answer) >= 0.5]
    if track.startswith("fusion:"):
        voters = [e for e in row["evidence"] if e["contributes"]]
        support = [[i for i in temporal if evidence_kind(e, e["track"]) in answers[i]["kinds"]
                    and iou(e, answers[i]) >= 0.5] for e in voters]
        return sorted({i for matches in support for i in matches}), sum(not matches for matches in support) if temporal else 0
    supported = [i for i in temporal if evidence_kind(row, track) in answers[i]["kinds"]]
    return supported, int(bool(temporal and not supported))


def replay_tracks(exports, replays):
    """Verify replay identity and original evidence before consuming fused scores."""
    if replays is None:
        return {}, None
    by_id = {run["query_id"]: run for run in exports}
    if len(replays) != len(by_id) or {r["query_id"] for r in replays} != set(by_id):
        raise ValueError("fusion replays and exports must contain the same query IDs")
    suites = [r["suite"] for r in replays]
    if any(suite != suites[0] for suite in suites) or len({r["recipe_hash"] for r in replays}) != 1:
        raise ValueError("one frozen recipe suite is required across all queries")
    suite = suites[0]
    if suite["schema"] != "retrieval-fusion-recipes/1" or suite["parameter_split"] not in ("diagnostic", "tune"):
        raise ValueError("invalid fusion recipe provenance")
    sources = suite["source_groups"]
    if not suite["parameter_episodes"] or any(not value for value in sources.values()):
        raise ValueError("fusion parameters need source provenance")
    parameter_sources = {sources[e] for e in suite["parameter_episodes"]}
    split_sources = defaultdict(set)
    split_sources[suite["parameter_split"]].update(parameter_sources)
    result = {}
    for replay in replays:
        run = by_id[replay["query_id"]]
        if replay["schema"] != "retrieval-fusion/2" or replay["algorithm_version"] != "fusion/two-groups-half-coverage/1" or replay["model_calls"] != 0:
            raise ValueError("invalid offline fusion replay")
        manifest = replay.get("recipe_manifest")
        if not isinstance(manifest, str) or hashlib.sha256(manifest.encode("utf-8")).hexdigest() != replay["recipe_hash"]:
            raise ValueError("fusion recipe hash does not match its manifest")
        if json.loads(manifest) != [replay["algorithm_version"], replay["suite"]]:
            raise ValueError("fusion recipe manifest does not match the embedded suite")
        if not run.get("_input_hash") or replay["input_hash"] != run["_input_hash"]:
            raise ValueError("fusion replay does not match the exact candidate export")
        if any(replay[key] != run[key] for key in ("space_id", "split", "episodes")) or suite["space_id"] != run["space_id"]:
            raise ValueError("fusion space, split, or scope mismatch")
        if set(replay["recipes"]) != set(suite["recipes"]) or not suite["recipes"]:
            raise ValueError("fusion recipe set changed between queries")
        split = run["split"]
        groups = {sources[e] for e in run["episodes"]}
        split_sources[split].update(groups)
        if suite["parameter_split"] == "diagnostic" and split != "diagnostic":
            raise ValueError("diagnostic parameters are exploratory only")
        if split == "holdout" and groups & parameter_sources:
            raise ValueError("parameter source videos overlap held-out data")
        originals = {(track, row["episode"], row["stream"], row.get("annotation"), row["id"]): row
                     for track, rows in run["tracks"].items() for row in rows}
        tracks = {}
        for name, data in replay["recipes"].items():
            moments = data["moments"]
            if not math.isfinite(data["fusion_ms"]) or data["fusion_ms"] < 0:
                raise ValueError("invalid fusion timing")
            used = set()
            previous_score = math.inf
            for moment in moments:
                if not math.isfinite(moment["score"]) or moment["score"] > previous_score:
                    raise ValueError("fusion results must be finite and ranked")
                previous_score = moment["score"]
                if moment["episode"] not in run["episodes"] or moment["start_us"] < 0 or moment["end_us"] <= moment["start_us"]:
                    raise ValueError("invalid fusion moment scope or interval")
                if not any(e["contributes"] is True for e in moment["evidence"]):
                    raise ValueError("fusion moment has no contributing evidence")
                for e in moment["evidence"]:
                    key = (e["track"], e["episode"], e["stream"], e.get("annotation"), e["id"])
                    original = originals.get(key)
                    if original is None or key in used:
                        raise ValueError("fusion invented or reused source evidence")
                    used.add(key)
                    if any(e.get(field) != original.get(field) for field in ("start_us", "end_us", "rank", "raw_score", "text")):
                        raise ValueError("fusion changed original evidence")
                    if (e["episode"], e["stream"]) != (moment["episode"], moment["stream"]) or iou(e, moment) <= 0:
                        raise ValueError("fusion evidence does not overlap its moment")
                    if (e["start_us"], e["end_us"]) != (moment["start_us"], moment["end_us"]):
                        overlap = min(e["end_us"], moment["end_us"]) - max(e["start_us"], moment["start_us"])
                        length, anchor_length = e["end_us"] - e["start_us"], moment["end_us"] - moment["start_us"]
                        if e["track"] not in ("description", "lexical") or length > anchor_length or 2 * overlap < max(length, anchor_length):
                            raise ValueError("fusion violated temporal anchor coverage")
                    if not math.isfinite(e["value"]) or not isinstance(e["contributes"], bool):
                        raise ValueError("invalid fusion contribution")
            tracks["fusion:" + name] = [dict(moment, raw_score=moment["score"]) for moment in moments]
        result[run["query_id"]] = tracks
    for left, right in (("diagnostic", "tune"), ("diagnostic", "holdout"), ("tune", "holdout")):
        if split_sources[left] & split_sources[right]:
            raise ValueError("original source videos cannot cross evaluation splits")
    return result, suite


def evaluate(exports, labels, k=5, threshold=None, fusions=None):
    if k <= 0 or (threshold is not None and not math.isfinite(threshold)):
        raise ValueError("invalid limit or threshold")
    by_id = {label["query_id"]: label for label in labels}
    if len(by_id) != len(labels) or len({r["query_id"] for r in exports}) != len(exports):
        raise ValueError("duplicate query ID")
    if {r["query_id"] for r in exports} != set(by_id):
        raise ValueError("exports and labels must contain the same query IDs")
    if len({r["space_id"] for r in exports}) != 1:
        raise ValueError("evaluate one embedding space at a time")
    split_episodes = defaultdict(set)
    for label in labels:
        if label.get("reviewed") is not True or label.get("exhaustive") is not True:
            raise ValueError("labels must be reviewed and exhaustive")
        if label["split"] not in ("diagnostic", "tune", "holdout"):
            raise ValueError("invalid split")
        scope = label.get("scope")
        if not isinstance(scope, list) or not scope or not all(isinstance(x, str) and x for x in scope):
            raise ValueError("every label needs an explicit corpus scope, including no-answer queries")
        split_episodes[label["split"]].update(scope)
        for answer in label["answers"]:
            if answer["episode"] not in scope:
                raise ValueError("answer outside labeled scope")
            if answer["start_us"] < 0 or answer["end_us"] <= answer["start_us"] or not answer["kinds"]:
                raise ValueError("invalid answer interval or evidence kinds")
            if not isinstance(answer["kinds"], list) or any(kind not in ("video", "speech", "screen", "description") for kind in answer["kinds"]):
                raise ValueError("unknown answer evidence kind")
    for left, right in (("diagnostic", "tune"), ("diagnostic", "holdout"), ("tune", "holdout")):
        if split_episodes[left] & split_episodes[right]:
            raise ValueError("diagnostic, tuning, and holdout scopes must not share videos")
    fused_tracks, suite = replay_tracks(exports, fusions)
    metrics = defaultdict(lambda: defaultdict(list))
    distributions = defaultdict(lambda: defaultdict(list))
    for run in exports:
        label = by_id[run["query_id"]]
        if set(run.get("episodes", [])) != set(label["scope"]):
            raise ValueError("retrieved and labeled corpus scopes differ")
        if label["split"] != run["split"]:
            raise ValueError("split mismatch")
        tracks = dict(run["tracks"])
        if any(row["episode"] not in label["scope"] for rows in tracks.values() for row in rows):
            raise ValueError("retrieval escaped the labeled corpus")
        # Uncalibrated max is a diagnostic baseline over unique intervals.
        # No description or BM25 score is mixed into this cosine baseline.
        unique = {}
        for kind in ("video", "speech", "screen"):
            for row in tracks.get(kind, []):
                key = (row["episode"], row["stream"], row["start_us"], row["end_us"])
                if key not in unique or row["raw_score"] > unique[key]["raw_score"]:
                    unique[key] = dict(row, kind=kind)
        tracks["raw_max"] = sorted(unique.values(), key=lambda r: (-r["raw_score"], r["episode"], r["stream"], r["start_us"]))
        tracks.update(fused_tracks.get(run["query_id"], {}))
        for track, candidates in tracks.items():
            group = (run["split"], track)
            stats = metrics[group]
            relevant_scores, irrelevant_scores = [], []
            for row in candidates:
                matches, _ = supporting_answers(row, track, label["answers"])
                (relevant_scores if matches else irrelevant_scores).append(row["raw_score"])
            distributions[group]["relevant"].extend(relevant_scores)
            distributions[group]["irrelevant"].extend(irrelevant_scores)
            selected = [r for r in candidates if track == "lexical" or track.startswith("fusion:") or threshold is None or r["raw_score"] >= threshold][:k]
            matched, gains, wrong = set(), [], 0
            for row in selected:
                supported, incorrect = supporting_answers(row, track, label["answers"])
                new = next((i for i in supported if i not in matched), None)
                gains.append(int(new is not None))
                if new is not None:
                    matched.add(new)
                wrong += incorrect
            answers = label["answers"]
            if answers:
                stats["recall"].append(len(matched) / len(answers))
                ideal = sum(1 / math.log2(i + 2) for i in range(min(k, len(answers))))
                stats["ndcg"].append(sum(g / math.log2(i + 2) for i, g in enumerate(gains)) / ideal)
                stats["best_iou"].append(max((iou(r, a) for r in selected for a in answers), default=0))
            else:
                stats["no_answer_false_positive"].append(int(bool(selected)))
            stats["wrong_evidence_count"].append(wrong)
            duplicates = sum(any(iou(row, prev) >= 0.5 for prev in selected[:i]) for i, row in enumerate(selected))
            stats["duplicate_rate"].append(duplicates / max(1, len(selected)))
    result = []
    for (split, track), stats in sorted(metrics.items()):
        row = {"split": split, "track": track, "k": k}
        if track.startswith("fusion:"):
            row["score_units"] = "recipe score, not cosine or probability"
        row.update({name: statistics.mean(values) for name, values in stats.items()})
        row["score_distributions"] = {name: {"count": len(values), "p05": percentile(values, .05), "p50": percentile(values, .5), "p95": percentile(values, .95)} for name, values in distributions[(split, track)].items()}
        result.append(row)
    return {"schema": "retrieval-evaluation/1", "cosine_threshold": threshold, "fusion_suite": suite,
            "scope": "bounded candidate diagnostic, not production temporal-merge or paid-model benchmark",
            "queries": len(exports), "metrics": result,
            "latency": {name: {"p50": percentile([r[name] for r in exports], .5), "p95": percentile([r[name] for r in exports], .95)} for name in ("vector_ms", "lexical_ms")},
            "fusion_latency": {name: {"p50": percentile([r["recipes"][name]["fusion_ms"] for r in fusions], .5),
                                      "p95": percentile([r["recipes"][name]["fusion_ms"] for r in fusions], .95)}
                               for name in (suite["recipes"] if suite else [])}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("exports")
    parser.add_argument("labels")
    parser.add_argument("--k", type=int, default=5)
    parser.add_argument("--threshold", type=float)
    parser.add_argument("--fusions", help="frozen recipe replay from the retrieval_fusion example")
    args = parser.parse_args()
    def read(path, hashes=False):
        with open(path) as source:
            rows = []
            for line in source:
                if line.strip():
                    row = json.loads(line)
                    if hashes:
                        row["_input_hash"] = hashlib.sha256(line.rstrip("\r\n").encode()).hexdigest()
                    rows.append(row)
            return rows
    print(json.dumps(evaluate(read(args.exports, hashes=True), read(args.labels), args.k, args.threshold,
                              read(args.fusions) if args.fusions else None), indent=2))


if __name__ == "__main__":
    main()
