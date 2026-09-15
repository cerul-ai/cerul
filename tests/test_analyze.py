"""Local-only analysis integration; set CERUL_TEST_BINARY to a built CLI."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import unittest
from test_index_concurrency import ModelServer


@unittest.skipUnless(os.environ.get("CERUL_TEST_BINARY"), "requires a built CLI")
class AnalyzeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="cerul-analyze-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / "demo.mp4"
        subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i",
            "color=size=64x64:rate=1:duration=61", "-c:v", "libx264", str(self.source)], check=True)
        self.server = ModelServer()
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        self.addCleanup(self.server.server_close)
        self.addCleanup(self.server.shutdown)

    def run_analysis(self, *extra):
        base = f"http://127.0.0.1:{self.server.server_port}"
        return subprocess.run([os.environ["CERUL_TEST_BINARY"], "--workspace", str(self.root / "workspace"),
            "--set", f'vision.base_url="{base}"', "--json", "analyze", str(self.source), *extra],
            env={"HOME": str(self.root), "PATH": os.environ["PATH"], "GEMINI_API_KEY": "local-fixture",
                "NO_PROXY": "127.0.0.1"}, capture_output=True, text=True, timeout=120)

    def test_standalone_analysis_and_cached_replay_without_embedding(self):
        result = self.run_analysis("--dry-run")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse(Path(str(self.source) + ".cerul").exists())
        self.assertFalse(self.server.calls)
        result = self.run_analysis()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        report = json.loads(result.stdout)
        self.assertTrue(report["streams"][0]["scenes"])
        self.assertIsNotNone(report["streams"][0]["summary"])
        self.assertFalse(any(c["kind"] in ("embedding", "speech") for c in self.server.calls))
        sidecar = Path(str(self.source) + ".cerul")
        self.assertFalse((sidecar / "embeddings").exists())
        for name in ["semantic.scene.jsonl", "semantic.section.jsonl", "semantic.summary.jsonl"]:
            self.assertTrue((sidecar / name).exists(), name)
        count = len(self.server.calls)
        result = self.run_analysis()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(len(self.server.calls), count)

    def test_partial_analysis_resumes_only_failed_scene(self):
        self.server.fail_scene = True
        result = self.run_analysis()
        self.assertEqual(result.returncode, 6, result.stdout + result.stderr)
        self.assertTrue(json.loads(result.stdout)["partial"])
        before = sum(c["kind"] == "scene" for c in self.server.calls)
        result = self.run_analysis()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(sum(c["kind"] == "scene" for c in self.server.calls) - before, 1)
