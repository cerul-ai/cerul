"""Opt-in PTY integration checks: CERUL_TEST_BINARY=/path/to/cerul python3 -m unittest discover -s tests -p test_setup.py."""
import http.server
import json
import os
import pathlib
import pty
import select
import signal
import tempfile
import threading
import termios
import time
import unittest


@unittest.skipUnless(os.environ.get("CERUL_TEST_BINARY"), "requires a built CLI")
class SetupTests(unittest.TestCase):
    def interact(self, home, steps, parent_ready=None):
        pid, fd = pty.fork()
        if pid == 0:
            env = {"HOME": str(home), "PATH": os.environ["PATH"], "TERM": "xterm", "GEMINI_API_KEY": "fixture-embedding-key"}
            os.execve(os.environ["CERUL_TEST_BINARY"], ["cerul", "config"], env)
        if parent_ready:
            parent_ready()
        output = b""
        pending = list(steps)
        eof = False
        try:
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                if select.select([fd], [], [], 0.1)[0]:
                    try:
                        chunk = os.read(fd, 65536)
                    except OSError:
                        eof = True
                        break
                    if not chunk:
                        eof = True
                        break
                    output += chunk
                    if pending and pending[0][0] in output:
                        prompt, answer = pending.pop(0)
                        if b"hidden" in prompt:
                            until = time.monotonic() + 2
                            while termios.tcgetattr(fd)[3] & termios.ECHO and time.monotonic() < until:
                                time.sleep(0.01)
                        os.write(fd, answer)
                child, status = os.waitpid(pid, os.WNOHANG)
                if child:
                    self.assertEqual(os.waitstatus_to_exitcode(status), 0, output.decode(errors="replace"))
                    pid = None
                    break
            if pid:
                child, status = os.waitpid(pid, 0 if eof else os.WNOHANG)
                if child:
                    pid = None
                    self.assertEqual(os.waitstatus_to_exitcode(status), 0, output.decode(errors="replace"))
                else:
                    self.fail("setup did not complete: " + output.decode(errors="replace"))
            self.assertFalse(pending, output.decode(errors="replace"))
            return output
        finally:
            os.close(fd)
            if pid:
                os.kill(pid, signal.SIGKILL)
                os.waitpid(pid, 0)

    def test_disabled_choice_is_saved_without_requesting_another_key(self):
        with tempfile.TemporaryDirectory() as directory:
            home = pathlib.Path(directory)
            output = self.interact(home, [(b"Esc leave", b"\x1b[A\r")])
            self.assertIn("enabled = false", (home / ".cerul/config.toml").read_text())
            self.assertNotIn(b"API key (hidden", output)
            self.assertFalse((home / ".cerul/credentials.json").exists())

    def test_custom_endpoint_saves_private_key_and_checks_transcription_protocol(self):
        requests = []

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                body = self.rfile.read(int(self.headers["Content-Length"]))
                requests.append((self.path, body, self.headers.get("Authorization")))
                response = json.dumps({"segments": [], "language": "en"}).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(response)))
                self.end_headers()
                self.wfile.write(response)

            def log_message(self, *_):
                pass

        with http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server:
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            try:
                with tempfile.TemporaryDirectory() as directory:
                    home = pathlib.Path(directory)
                    base = f"http://127.0.0.1:{server.server_port}/v1"
                    output = self.interact(home, [
                        (b"Esc leave", b"\x1b[B\x1b[B\x1b[B\r"),
                        (b"Base URL", (base + "\n").encode()),
                        (b"ASR model", b"fixture-whisper\n"),
                        (b"API key (hidden", b"fixture-asr-key\n"),
                    ], parent_ready=thread.start)
                    config = (home / ".cerul/config.toml").read_text()
                    self.assertIn(base, config)
                    self.assertIn("enabled = true", config)
                    self.assertNotIn("fixture-asr-key", config)
                    self.assertNotIn(b"fixture-asr-key", output)
                    keys = home / ".cerul/credentials.json"
                    self.assertEqual(keys.stat().st_mode & 0o777, 0o600)
                    self.assertIn("fixture-asr-key", json.loads(keys.read_text()).values())
                    self.assertEqual(len(requests), 2)
                    for path, body, auth in requests:
                        self.assertEqual(path, "/v1/audio/transcriptions")
                        self.assertEqual(auth, "Bearer fixture-asr-key")
                        self.assertIn(b"verbose_json", body)
                        self.assertIn(b"timestamp_granularities[]", body)
            finally:
                server.shutdown()
                thread.join()
