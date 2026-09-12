import importlib.util
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
