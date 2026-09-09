#!/usr/bin/env python3
"""Exercise the relocatable bundle without PATH media tools or model credentials."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

bundle = Path(sys.argv[1]).resolve()
root = Path(__file__).resolve().parent.parent
for name in ("cerul", "cerul-ffmpeg", "cerul-ffprobe"):
    assert (bundle / name).is_file(), f"missing {name}"
with tempfile.TemporaryDirectory(prefix="cerul-bundle-test-") as temp:
    temp = Path(temp)
    home = temp / "home"
    home.mkdir()
    empty = temp / "empty-path"
    empty.mkdir()
    env = {k: v for k, v in os.environ.items() if not k.startswith("CERUL_") and k not in ("GEMINI_API_KEY", "HOME", "PATH")}
    env.update(HOME=str(home), PATH=str(empty))
    subprocess.run([str(bundle / "cerul-ffmpeg"), "-v", "error", "-loop", "1", "-i", str(root / "tests/fixtures/ocr-text.png"), "-t", "1", "-r", "2", "-pix_fmt", "yuv420p", "-c:v", "libx264", str(temp / "demo.mp4")], env=env, check=True)
    result = subprocess.run([str(bundle / "cerul"), "--json", "--workspace", str(temp / "workspace"), "index", str(temp / "demo.mp4"), "--no-audio", "--jobs", "1"], env=env, capture_output=True, text=True, timeout=120)
    assert result.returncode == 6, (result.returncode, result.stdout, result.stderr)
    indexed = json.loads(result.stdout)
    assert all(len(stream["errors"]) == 1 and "GEMINI_API_KEY" in stream["errors"][0] for episode in indexed["episodes"] for stream in episode["streams"]), indexed
    result = subprocess.run([str(bundle / "cerul"), "--json", "--workspace", str(temp / "workspace"), "search", "--text", "CERUL"], env=env, capture_output=True, text=True, timeout=30)
    assert result.returncode == 0, (result.stdout, result.stderr)
    data = json.loads(result.stdout)
    assert data["hits"], {"search": data, "index": indexed, "ocr": [p.read_text() for p in temp.rglob("screen_text.jsonl")]}
    assert "CERUL" in json.dumps(data["hits"]).upper(), data
    assert not (home / ".cerul/credentials.json").exists()
print("Bundle passed: no PATH tools, real H.264 encode/probe, OCR, text search, noninteractive missing-key handling.")
