import io
import json
import os
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
import unittest

from lider import ipc, native_host
from lider.core import settings as settings_mod

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


class FakeLid:
    """Collects the lines a running LID would receive on its socket."""

    def __init__(self, name):
        self.lines = []
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.bind(name)
        self.sock.listen(4)
        threading.Thread(target=self._serve, daemon=True).start()

    def _serve(self):
        while True:
            try:
                conn, _ = self.sock.accept()
            except OSError:
                return
            with conn:
                data = b""
                while chunk := conn.recv(65536):
                    data += chunk
            self.lines += data.decode().splitlines()

    def requests(self, timeout=3):
        end = time.monotonic() + timeout
        while not self.lines and time.monotonic() < end:
            time.sleep(0.01)
        return [json.loads(line) for line in self.lines]

    def close(self):
        self.sock.close()


def frame(obj):
    data = json.dumps(obj).encode()
    return struct.pack("=I", len(data)) + data


class FramingTests(unittest.TestCase):
    def test_round_trip(self):
        out = io.BytesIO()
        native_host.write_message(out, {"type": "hello", "ü": 1})
        self.assertEqual(native_host.read_message(io.BytesIO(out.getvalue())), {"type": "hello", "ü": 1})

    def test_end_of_input(self):
        self.assertIsNone(native_host.read_message(io.BytesIO(b"")))

    def test_oversized_message_is_refused(self):
        with self.assertRaises(ValueError):
            native_host.read_message(io.BytesIO(struct.pack("=I", 64 * 1024 * 1024)))


class HandleTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.old = os.environ.get("XDG_CONFIG_HOME")
        os.environ["XDG_CONFIG_HOME"] = self.tmp.name
        self.name = f"\0lider-test-host-{os.getpid()}-{id(self)}"

    def tearDown(self):
        if self.old is None:
            os.environ.pop("XDG_CONFIG_HOME", None)
        else:
            os.environ["XDG_CONFIG_HOME"] = self.old
        self.tmp.cleanup()

    def test_hello_reports_the_capture_settings(self):
        settings_mod.save(settings_mod.Settings(browser_capture=False, auto_capture_types="ZIP ISO",
                                                no_auto_sites="example.com"))
        reply = native_host.handle({"type": "hello"}, self.name)
        self.assertEqual((reply["ok"], reply["capture"], reply["types"], reply["skip_sites"], reply["running"]),
                         (True, False, ["ZIP", "ISO"], ["example.com"], False))

    def test_download_is_forwarded_with_only_known_fields(self):
        lid = FakeLid(self.name)
        try:
            reply = native_host.handle({"type": "download", "url": "https://x.org/a.zip", "referer": "https://x.org/",
                                        "cookies": "s=1", "user_agent": "UA", "size": 10, "evil": "x",
                                        "page_url": "javascript:alert(1)"}, self.name)
            self.assertEqual(reply, {"ok": True})
            self.assertEqual(lid.requests(), [{"action": "download", "url": "https://x.org/a.zip",
                                               "referer": "https://x.org/", "cookies": "s=1",
                                               "user_agent": "UA", "size": 10}])
        finally:
            lid.close()

    def test_bad_urls_are_rejected(self):
        for url in ("javascript:alert(1)", "file:///etc/passwd", "", None):
            self.assertFalse(native_host.handle({"type": "download", "url": url}, self.name)["ok"])
        reply = native_host.handle({"type": "links", "items": [{"url": "data:text/html,x"}]}, self.name)
        self.assertEqual(reply["error"], "invalid request")

    def test_links_are_cleaned(self):
        lid = FakeLid(self.name)
        try:
            reply = native_host.handle({"type": "links", "page_url": "https://x.org/p", "items": [
                {"url": "https://x.org/a.zip", "text": "A", "kind": "a"},
                {"url": "ftp://x.org/b.iso", "kind": "weird"},
                {"url": "chrome://settings"}, "not a dict"]}, self.name)
            self.assertTrue(reply["ok"])
            req = lid.requests()[0]
            self.assertEqual(req["items"], [{"url": "https://x.org/a.zip", "text": "A", "kind": "a"},
                                            {"url": "ftp://x.org/b.iso", "text": "", "kind": "a"}])
            self.assertEqual(req["page_url"], "https://x.org/p")
        finally:
            lid.close()

    def test_starts_lid_when_it_is_not_running(self):
        started = []

        def fake_start():
            started.append(FakeLid(self.name))

        # The "launch command" starts our fake LID in a thread via a tiny Python process stand-in.
        original = subprocess.Popen
        subprocess.Popen = lambda *a, **k: fake_start()
        try:
            ok = native_host.deliver({"action": "show"}, self.name, launch=["lider"], wait=5)
        finally:
            subprocess.Popen = original
        self.assertTrue(ok)
        self.assertEqual(started[0].requests(), [{"action": "show"}])
        started[0].close()

    def test_unknown_type(self):
        self.assertFalse(native_host.handle({"type": "format-disk"}, self.name)["ok"])


class ProcessTests(unittest.TestCase):
    def test_host_process_speaks_native_messaging(self):
        with tempfile.TemporaryDirectory() as tmp:
            env = dict(os.environ, XDG_CONFIG_HOME=tmp, PYTHONPATH=ROOT)
            proc = subprocess.run([sys.executable, "-m", "lider.native_host"],
                                  input=frame({"type": "hello"}) + frame({"type": "nope"}),
                                  capture_output=True, env=env, timeout=30)
        out = io.BytesIO(proc.stdout)
        first, second = native_host.read_message(out), native_host.read_message(out)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertTrue(first["ok"])
        self.assertIn("ZIP", first["types"])
        self.assertFalse(second["ok"])
        self.assertNotIn(b"PySide6", proc.stderr)

    def test_native_host_does_not_load_qt(self):
        code = "import sys, lider.native_host; print(any(m.startswith('PySide6') for m in sys.modules))"
        out = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True,
                             env=dict(os.environ, PYTHONPATH=ROOT), timeout=30).stdout.strip()
        self.assertEqual(out, "False")


class IpcTests(unittest.TestCase):
    def test_send_request_and_is_running(self):
        name = f"\0lider-test-ipc-{os.getpid()}"
        self.assertFalse(ipc.is_running(name))
        lid = FakeLid(name)
        try:
            self.assertTrue(ipc.is_running(name))
            self.assertTrue(ipc.send_request({"action": "show"}, name))
        finally:
            lid.close()


if __name__ == "__main__":
    unittest.main()
