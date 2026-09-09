#!/usr/bin/env python3
"""Catch color conversion corruption before compiling the CLI release binary."""
from pathlib import Path
import subprocess
import sys
import tempfile

bundle = Path(sys.argv[1]).resolve()
fixture = Path(__file__).resolve().parent.parent / "tests/fixtures/ocr-text.png"
ffmpeg = str(bundle / "cerul-ffmpeg")
with tempfile.TemporaryDirectory(prefix="cerul-media-test-") as directory:
    video = Path(directory) / "gray.mp4"
    subprocess.run([ffmpeg, "-v", "error", "-loop", "1", "-i", str(fixture),
                    "-t", "1", "-r", "2", "-pix_fmt", "yuv420p", "-c:v", "libx264", str(video)], check=True)
    pixels = subprocess.check_output([ffmpeg, "-v", "error", "-i", str(video),
                                      "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
    assert pixels and len(pixels) % 3 == 0, "missing RGB frame"
    difference = sum(abs(r - g) + abs(g - b) for r, g, b in zip(pixels[0::3], pixels[1::3], pixels[2::3])) / (len(pixels) / 3)
    assert difference < 4, f"grayscale frame has color corruption: mean channel difference {difference:.2f}"
print("Media passed: H.264 RGB/YUV conversion preserves grayscale without color stripes.")
