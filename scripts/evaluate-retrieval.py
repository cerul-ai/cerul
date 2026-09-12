#!/usr/bin/env python3
"""Score offline candidate exports against exhaustive, human-reviewed intervals."""
import argparse
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


def evaluate(exports, labels, k=5, threshold=None):
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
    for left, right in (("diagnostic", "tune"), ("diagnostic", "holdout"), ("tune", "holdout")):
        if split_episodes[left] & split_episodes[right]:
            raise ValueError("diagnostic, tuning, and holdout scopes must not share videos")
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
        for track, candidates in tracks.items():
            group = (run["split"], track)
            stats = metrics[group]
            relevant_scores, irrelevant_scores = [], []
            for row in candidates:
                kind = row.get("kind", track)
                if kind == "lexical":
                    kind = "speech" if row.get("annotation") == "transcript" else "screen"
                matches = [i for i, answer in enumerate(label["answers"])
                           if kind in answer["kinds"] and iou(row, answer) >= 0.5]
                (relevant_scores if matches else irrelevant_scores).append(row["raw_score"])
            distributions[group]["relevant"].extend(relevant_scores)
            distributions[group]["irrelevant"].extend(irrelevant_scores)
            selected = [r for r in candidates if track == "lexical" or threshold is None or r["raw_score"] >= threshold][:k]
            matched, gains, wrong = set(), [], 0
            for row in selected:
                kind = row.get("kind", track)
                if kind == "lexical":
                    kind = "speech" if row.get("annotation") == "transcript" else "screen"
                temporal = [i for i, a in enumerate(label["answers"]) if iou(row, a) >= 0.5]
                supported = [i for i in temporal if kind in label["answers"][i]["kinds"]]
                new = next((i for i in supported if i not in matched), None)
                gains.append(int(new is not None))
                if new is not None:
                    matched.add(new)
                wrong += bool(temporal and not supported)
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
        row.update({name: statistics.mean(values) for name, values in stats.items()})
        row["score_distributions"] = {name: {"count": len(values), "p05": percentile(values, .05), "p50": percentile(values, .5), "p95": percentile(values, .95)} for name, values in distributions[(split, track)].items()}
        result.append(row)
    return {"schema": "retrieval-evaluation/1", "cosine_threshold": threshold,
            "scope": "bounded candidate diagnostic, not production temporal-merge or paid-model benchmark",
            "queries": len(exports), "metrics": result,
            "latency": {name: {"p50": percentile([r[name] for r in exports], .5), "p95": percentile([r[name] for r in exports], .95)} for name in ("vector_ms", "lexical_ms")}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("exports")
    parser.add_argument("labels")
    parser.add_argument("--k", type=int, default=5)
    parser.add_argument("--threshold", type=float)
    args = parser.parse_args()
    def read(path):
        with open(path) as source:
            return [json.loads(line) for line in source if line.strip()]
    print(json.dumps(evaluate(read(args.exports), read(args.labels), args.k, args.threshold), indent=2))


if __name__ == "__main__":
    main()
