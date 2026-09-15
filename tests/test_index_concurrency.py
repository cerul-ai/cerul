"""Opt-in local-API checks: CERUL_TEST_BINARY=/path/to/cerul python3 -m unittest discover -s tests -p test_index_concurrency.py."""
import collections
import http.server
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import threading
import time
import unittest


class ModelServer(http.server.ThreadingHTTPServer):
    def __init__(self):
        self.lock = threading.Lock()
        self.active = 0
        self.peak = 0
        self.calls = []
        self.fail_scene = False
        self.failed = False
        super().__init__(("127.0.0.1", 0), ModelHandler)


class ModelHandler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        data = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        parts = data.get("contents", [{}])[0].get("parts", [])
        prompt = "\n".join(p.get("text", "") for p in parts)
        if "embedContent" in self.path or "batchEmbedContents" in self.path:
            kind = "embedding"
            value = ({"embeddings":[{"values":[1.,0.]} for _ in data["requests"]]}
                if "requests" in data else {"embedding": {"values": [1., 0.]}})
        elif "audioTranscriptionConfig" in data.get("generationConfig", {}):
            kind = "speech"
            value = {"candidates": [{"finishReason": "STOP", "content": {"parts": [{
                "audioTranscription": {"words": [{"word": "hello", "startOffset": "0.1s", "endOffset": "0.9s"}]}
            }]}}]}
        else:
            if "Clip duration:" in prompt:
                kind = "scene"
                duration = int(re.search(r"Clip duration: (\d+)", prompt)[1])
                value = {"scenes": [{"start_us": i * duration // 12,
                    "end_us": (i + 1) * duration // 12,
                    "description": "A worker guides a panel into the machine. " + "The panel is visible. " * 15,
                    "objects": ["panel"], "actions": ["push"], "kind": "static"} for i in range(12)]}
                with self.server.lock:
                    if self.server.fail_scene and not self.server.failed:
                        self.server.failed = True
                        value["scenes"][0]["end_us"] = -1
            elif "\nRecords: " in prompt:
                kind = "overview"
                records = json.loads(prompt.split("\nRecords: ", 1)[1])
                ref = records[0].get("source") or records[0]["source_refs"][0]
                value = {"title": "Panel handling", "summary": "A worker guides a panel into the machine.",
                    "content_type": "static", "environment": None, "language": None,
                    "source_refs": [ref], "sections": [], "suggestions": []}
            else:
                kind = "probe"
                value = {"ok": True}
            value = {"candidates": [{"content": {"parts": [{"text": json.dumps(value)}]}}]}
        with self.server.lock:
            self.server.active += 1
            self.server.peak = max(self.server.peak, self.server.active)
            call = {"kind": kind, "start": time.monotonic()}
            self.server.calls.append(call)
        try:
            time.sleep({"scene": .45, "speech": .35, "overview": .6, "embedding": .10}.get(kind, .01))
            body = json.dumps(value).encode()
            self.send_response(200)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        finally:
            with self.server.lock:
                call["end"] = time.monotonic()
                self.server.active -= 1


@unittest.skipUnless(os.environ.get("CERUL_TEST_BINARY"), "requires a built CLI")
class ConcurrencyTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="cerul-concurrency-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.server = ModelServer()
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        self.addCleanup(self.server.server_close)
        self.addCleanup(self.server.shutdown)

    def run_index(self, name, jobs, binary=None):
        source = self.root / f"{name}.mp4"
        if not source.exists():
            subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i",
                "color=size=64x64:rate=1:duration=301", "-f", "lavfi", "-i",
                "sine=frequency=440:sample_rate=16000:duration=301", "-c:v", "libx264",
                "-c:a", "aac", str(source)], check=True)
        base = f"http://127.0.0.1:{self.server.server_port}"
        args = [binary or os.environ["CERUL_TEST_BINARY"], "--json", "--workspace", str(self.root / name)]
        for key, value in [("embedding.base_url", base), ("embedding.dims", 2),
            ("vision.base_url", base), ("transcription.base_url", base),
            ("transcription.model", "gemini-3.5-transcribe"), ("transcription.enabled", True)]:
            args += ["--set", f"{key}={json.dumps(value)}"]
        args += ["index", str(source), "--no-ocr", "--jobs", str(jobs)]
        start = time.monotonic()
        result = subprocess.run(args, env={"HOME": str(self.root), "PATH": os.environ["PATH"],
            "GEMINI_API_KEY": "local-fixture", "NO_PROXY": "127.0.0.1"}, capture_output=True, text=True, timeout=120)
        elapsed = time.monotonic() - start
        self.assertIn(result.returncode, (0, 6), result.stderr + result.stdout)
        return source, result, elapsed

    def test_concurrency_budget_overlap_and_offline_reuse(self):
        source, result, elapsed = self.run_index("parallel", 4)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertGreaterEqual(self.server.peak, 2)
        self.assertLessEqual(self.server.peak, 4)
        calls = self.server.calls
        def overlaps(a, b):
            return any(x is not y and x["start"] < y["end"] and y["start"] < x["end"]
                for x in calls if x["kind"] == a for y in calls if y["kind"] == b)
        self.assertFalse(any(c["kind"] in ("scene", "overview", "probe") for c in calls))
        self.assertTrue(overlaps("speech", "speech"))
        self.assertTrue(overlaps("embedding", "embedding"))
        self.assertTrue(overlaps("speech", "embedding"), "video embeddings must begin before all speech finishes")
        diagnostics=json.loads((self.root / "parallel/runtime/diagnostics/index-latest.json").read_text())
        self.assertEqual(sum(stage["requests"] for stage in diagnostics["stages"]),len(calls))
        self.assertTrue(any(stage["name"]=="video_embedding" and stage["requests"]>0 for stage in diagnostics["stages"]))
        self.assertTrue(all(stage["elapsed_ms"]<=diagnostics["elapsed_ms"] for stage in diagnostics["stages"]))
        self.assertGreater(sum(stage["request_elapsed_ms"] for stage in diagnostics["stages"]),0)
        sidecar = Path(str(source) + ".cerul")
        for name in ("transcript",):
            records = [json.loads(x) for x in (sidecar / (name + ".jsonl")).read_text().splitlines()][1:]
            self.assertEqual([r["start_us"] for r in records], sorted(r["start_us"] for r in records))
        for name in ("semantic.scene.jsonl", "semantic.summary.jsonl", "semantic.section.jsonl", "understanding.status.json"):
            self.assertFalse((sidecar / name).exists(), name)
        events = [json.loads(line) for line in result.stderr.splitlines() if line.startswith("{")]
        self.assertFalse(any(e.get("station") in ("understanding", "overview", "description") for e in events))
        count = len(calls)
        self.run_index("parallel", 4)
        self.assertEqual(len(calls), count)
        cached=json.loads((self.root / "parallel/runtime/diagnostics/index-latest.json").read_text())
        self.assertEqual(sum(stage["requests"] for stage in cached["stages"]),0)
        self.assertGreater(sum(cache["hits"] for stage in cached["stages"] for cache in stage["caches"].values()),0)
        shown=subprocess.run([os.environ["CERUL_TEST_BINARY"],"--workspace",str(self.root/"parallel"),"diagnostics","--json"],env={"PATH":os.environ["PATH"]},capture_output=True,text=True,timeout=30)
        self.assertEqual(shown.returncode,0,shown.stderr)
        self.assertEqual(json.loads(shown.stdout)["index"],cached)
        print(f"Parallel fixture: {elapsed:.2f}s; peak {self.server.peak}; calls {dict(collections.Counter(x['kind'] for x in calls))}; cached replay 0 calls")

    def test_cached_index_preserves_failed_legacy_analysis_without_retrying_it(self):
        source, result, _ = self.run_index("legacy", 4)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        sidecar = Path(str(source) + ".cerul")
        status = sidecar / "understanding.status.json"
        previous = json.dumps({"status": "incomplete", "successful": [], "failed": [],
            "errors": ["understanding overview: too many overview references"]}) + "\n"
        status.write_text(previous)
        count = len(self.server.calls)
        _, result, _ = self.run_index("legacy", 4)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(status.read_text(), previous)
        self.assertEqual(len(self.server.calls), count)
        self.assertNotIn("too many overview references", result.stdout)
