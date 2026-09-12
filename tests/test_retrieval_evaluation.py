import importlib.util
import copy
import hashlib
import json
import pathlib
import unittest

spec = importlib.util.spec_from_file_location("evaluation", pathlib.Path(__file__).parents[1] / "scripts/evaluate-retrieval.py")
evaluation = importlib.util.module_from_spec(spec)
spec.loader.exec_module(evaluation)


class EvaluationTest(unittest.TestCase):
    def fixture(self):
        interval = dict(episode="clip", stream="primary", start_us=0, end_us=10)
        export = dict(query_id="q", split="holdout", space_id="space", vector_ms=2, lexical_ms=1, episodes=["clip"],
                      tracks={"video": [dict(interval, raw_score=.7)], "speech": [dict(interval, raw_score=.9)]})
        label = dict(query_id="q", split="holdout", reviewed=True, exhaustive=True, scope=["clip"],
                     answers=[dict(interval, kinds=["video"])])
        return export, label

    def test_stronger_wrong_modality_does_not_count_as_correct_evidence(self):
        export, label = self.fixture()
        result = evaluation.evaluate([export], [label])
        tracks = {r["track"]: r for r in result["metrics"]}
        self.assertEqual(tracks["video"]["recall"], 1)
        self.assertEqual(tracks["raw_max"]["recall"], 0)
        self.assertEqual(tracks["raw_max"]["wrong_evidence_count"], 1)

    def test_no_answer_gate_and_unreviewed_labels(self):
        export, label = self.fixture()
        label["answers"] = []
        result = evaluation.evaluate([export], [label], threshold=.95)
        self.assertTrue(all(r["no_answer_false_positive"] == 0 for r in result["metrics"]))
        label["reviewed"] = False
        with self.assertRaises(ValueError):
            evaluation.evaluate([export], [label])

    def test_unknown_or_misspelled_evidence_kinds_are_rejected(self):
        for kinds in (["visual"], ["screens"], ["Video"], ["video", "unknown"], "video"):
            with self.subTest(kinds=kinds):
                export, label = self.fixture()
                label["answers"][0]["kinds"] = kinds
                with self.assertRaisesRegex(ValueError, "unknown answer evidence kind"):
                    evaluation.evaluate([export], [label])

    def test_tuning_and_holdout_cannot_share_answer_videos(self):
        export, label = self.fixture()
        other_export = dict(export, query_id="other", split="tune")
        other_label = dict(label, query_id="other", split="tune")
        with self.assertRaises(ValueError):
            evaluation.evaluate([export, other_export], [label, other_label])

    def test_no_answer_videos_cannot_leak_between_splits(self):
        export, label = self.fixture()
        label["answers"] = []
        for split in ("diagnostic", "tune"):
            with self.subTest(split=split):
                other_export = dict(export, query_id="other", split=split)
                other_label = dict(label, query_id="other", split=split)
                with self.assertRaisesRegex(ValueError, "must not share videos"):
                    evaluation.evaluate([export, other_export], [label, other_label])

    def test_retrieval_scope_must_match_the_exhaustive_label_scope(self):
        export, label = self.fixture()
        export["episodes"].append("unjudged-video")
        with self.assertRaises(ValueError):
            evaluation.evaluate([export], [label])

    def test_disjoint_no_answer_scopes_are_valid_but_outside_results_are_not(self):
        export, label = self.fixture()
        other_export = dict(export, query_id="other", split="tune", episodes=["other-video"], tracks={"video": []})
        other_label = dict(label, query_id="other", split="tune", scope=["other-video"], answers=[])
        result = evaluation.evaluate([export, other_export], [label, other_label])
        self.assertEqual(result["queries"], 2)
        other_export["tracks"]["video"] = export["tracks"]["video"]
        with self.assertRaisesRegex(ValueError, "escaped the labeled corpus"):
            evaluation.evaluate([export, other_export], [label, other_label])

    def fusion_fixture(self):
        export, label = self.fixture()
        export["_input_hash"] = "exact-export-hash"
        for track, rows in export["tracks"].items():
            for row in rows:
                row.update(id=track, rank=1, text=None, annotation=None)
        evidence = [dict(row, track=track, value=row["raw_score"], contributes=True)
                    for track, rows in export["tracks"].items() for row in rows]
        moment = dict(episode="clip", stream="primary", start_us=0, end_us=10, score=.03, evidence=evidence)
        suite = dict(schema="retrieval-fusion-recipes/1", space_id="space", parameter_split="tune",
                     parameter_episodes=["tuning"], source_groups={"clip": "source", "tuning": "tuning-source"},
                     recipes={"rrf": {"method": "grouped_rrf"}})
        replay = dict(schema="retrieval-fusion/2", algorithm_version="fusion/two-groups-half-coverage/1", query_id="q", split="holdout", space_id="space", episodes=["clip"],
                      model_calls=0, input_hash="exact-export-hash", suite=suite,
                      recipes={"rrf": dict(fusion_ms=.1, moments=[moment])})
        self.freeze_recipe(replay)
        return export, label, replay

    def freeze_recipe(self, replay):
        replay["recipe_manifest"] = json.dumps([replay["algorithm_version"], replay["suite"]], ensure_ascii=False, separators=(",", ":"))
        replay["recipe_hash"] = hashlib.sha256(replay["recipe_manifest"].encode("utf-8")).hexdigest()

    def test_replays_verify_manifest_bytes_and_embedded_parameters(self):
        for mutation in ("weight", "source", "hash", "manifest", "missing", "legacy"):
            with self.subTest(mutation=mutation):
                export, label, replay = self.fusion_fixture()
                if mutation == "weight":
                    replay["suite"]["recipes"]["rrf"]["rank_constant"] = 99
                elif mutation == "source":
                    replay["suite"]["source_groups"]["clip"] = "different-source"
                elif mutation == "hash":
                    replay["recipe_hash"] = "unchanged-hash-from-another-run"
                elif mutation == "manifest":
                    replay["recipe_manifest"] += " "
                elif mutation == "missing":
                    replay.pop("recipe_manifest")
                else:
                    replay["schema"] = "retrieval-fusion/1"
                with self.assertRaises(ValueError):
                    evaluation.evaluate([export], [label], fusions=[replay])

    def test_manifest_hash_preserves_original_json_spelling(self):
        export, label, replay = self.fusion_fixture()
        replay["suite"]["recipes"]["rrf"]["weight"] = 1e-7
        replay["suite"]["source_groups"]["clip"] = "视频"
        self.freeze_recipe(replay)
        # Rust and Python may spell this same float differently. Hash the
        # producer's bytes, then independently compare the complete parsed suite.
        replay["recipe_manifest"] = replay["recipe_manifest"].replace("1e-07", "1e-7")
        replay["recipe_hash"] = hashlib.sha256(replay["recipe_manifest"].encode("utf-8")).hexdigest()
        self.assertEqual(evaluation.evaluate([export], [label], fusions=[replay])["queries"], 1)

    def test_fused_support_retains_wrong_evidence_and_has_its_own_score_units(self):
        export, label, replay = self.fusion_fixture()
        # Cosine's .95 gate must not discard a separately gated RRF score of .03.
        result = evaluation.evaluate([export], [label], threshold=.95, fusions=[replay])
        tracks = {r["track"]: r for r in result["metrics"]}
        self.assertEqual(tracks["fusion:rrf"]["recall"], 1)
        self.assertEqual(tracks["fusion:rrf"]["wrong_evidence_count"], 1)
        self.assertEqual(tracks["raw_max"]["recall"], 0)
        self.assertEqual(result["fusion_latency"]["rrf"]["p95"], .1)
        # Merely carrying a matching record must not count it as a ranking vote.
        replay["recipes"]["rrf"]["moments"][0]["evidence"][0]["contributes"] = False
        result = evaluation.evaluate([export], [label], fusions=[replay])
        fused = next(r for r in result["metrics"] if r["track"] == "fusion:rrf")
        self.assertEqual(fused["recall"], 0)

    def test_replays_reject_stale_exports_and_changed_evidence(self):
        for mutation in ("hash", "interval", "rank", "duplicate", "foreign"):
            with self.subTest(mutation=mutation):
                export, label, replay = self.fusion_fixture()
                moment = replay["recipes"]["rrf"]["moments"][0]
                if mutation == "hash":
                    replay["input_hash"] = "old-export"
                elif mutation == "interval":
                    moment["evidence"][0]["end_us"] = 11
                elif mutation == "rank":
                    moment["evidence"][0]["rank"] = 2
                elif mutation == "duplicate":
                    replay["recipes"]["rrf"]["moments"].append(copy.deepcopy(moment))
                else:
                    moment["evidence"][0]["id"] = "invented"
                with self.assertRaises(ValueError):
                    evaluation.evaluate([export], [label], fusions=[replay])

    def test_original_source_and_diagnostic_parameters_cannot_leak_into_holdout(self):
        for mutation in ("source", "diagnostic"):
            export, label, replay = self.fusion_fixture()
            if mutation == "source":
                replay["suite"]["source_groups"]["tuning"] = "source"
            else:
                replay["suite"]["parameter_split"] = "diagnostic"
            self.freeze_recipe(replay)
            with self.assertRaises(ValueError):
                evaluation.evaluate([export], [label], fusions=[replay])

    def test_fusion_no_answer_uses_gated_results_and_still_requires_review(self):
        export, label, replay = self.fusion_fixture()
        label["answers"] = []
        result = evaluation.evaluate([export], [label], fusions=[replay])
        fused = next(r for r in result["metrics"] if r["track"] == "fusion:rrf")
        self.assertEqual(fused["no_answer_false_positive"], 1)
        replay["recipes"]["rrf"]["moments"] = []
        result = evaluation.evaluate([export], [label], fusions=[replay])
        fused = next(r for r in result["metrics"] if r["track"] == "fusion:rrf")
        self.assertEqual(fused["no_answer_false_positive"], 0)
        label["reviewed"] = False
        with self.assertRaises(ValueError):
            evaluation.evaluate([export], [label], fusions=[replay])

    def test_aligned_moment_does_not_invent_finer_evidence_boundaries(self):
        export, label, replay = self.fusion_fixture()
        # A long description overlaps the answer but cannot establish a short
        # action interval just because the returned anchor has those boundaries.
        original = dict(episode="clip", stream="primary", start_us=0, end_us=90,
                        id="description", rank=1, text=None, raw_score=.9, annotation=None)
        export["tracks"]["description"] = [original]
        moment = replay["recipes"]["rrf"]["moments"][0]
        moment["evidence"] = [dict(original, track="description", value=.9, contributes=True)]
        label["answers"][0]["kinds"] = ["description"]
        with self.assertRaisesRegex(ValueError, "temporal anchor coverage"):
            evaluation.evaluate([export], [label], fusions=[replay])
