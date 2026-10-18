import hashlib
import os
import tempfile
import threading
import time
import unittest

from lider.core.engine import (
    DownloadTask,
    EngineOptions,
    HttpClient,
    SpeedLimiter,
    parse_content_disposition,
    unique_path,
)
from lider.core.models import Download, Status
from tests.httpfixture import FileServer


def sha(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


class EngineTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = self.tmp.name
        self.client = HttpClient("lider-test", timeout=5)
        self.servers = []

    def tearDown(self):
        for s in self.servers:
            s.close()
        self.tmp.cleanup()

    def server(self, **kw):
        s = FileServer(**kw)
        self.servers.append(s)
        return s

    def task(self, url, connections=8, **kw):
        d = Download(id=1, url=url, filename="out.bin", save_dir=os.path.join(self.dir, "save"))
        opts = EngineOptions(max_connections=connections, retries=3, retry_delay=0.05,
                             temp_dir=os.path.join(self.dir, "tmp"))
        done = threading.Event()
        t = DownloadTask(d, self.client, opts, on_finish=lambda _t: done.set(), **kw)
        return d, t, done

    def test_segmented_download_is_byte_exact(self):
        srv = self.server(size=5 * 1024 * 1024 + 123, delay_per_chunk=0.002)
        d, t, done = self.task(srv.url, connections=8)
        t.start()
        self.assertTrue(done.wait(30))
        self.assertEqual(d.status, Status.COMPLETE, d.error)
        self.assertEqual(sha(d.path), srv.sha256)
        self.assertEqual(d.size, len(srv.data))
        self.assertGreater(len(t.connections), 1, "should open several connections")
        self.assertGreater(sum(1 for r in srv.requests if r and r != "bytes=0-"), 1)

    def test_snapshot_reports_speed_per_connection(self):
        srv = self.server(size=8 * 1024 * 1024, delay_per_chunk=0.01)
        d, t, done = self.task(srv.url, connections=4)
        t.start()
        while len(t.connections) < 4 or d.downloaded < 1024 * 1024:
            time.sleep(0.02)
        conns = t.snapshot()["connections"]
        self.assertEqual(len(conns), 4)
        rates = [c[4] for c in conns]
        self.assertTrue(all(r > 0 for r in rates), rates)
        self.assertAlmostEqual(sum(rates), t.meter.rate(), delta=t.meter.rate() * 0.5)
        t.stop()
        self.assertTrue(done.wait(10))

    def test_pause_and_resume(self):
        srv = self.server(size=4 * 1024 * 1024, delay_per_chunk=0.01)
        d, t, done = self.task(srv.url, connections=4)
        t.start()
        while d.downloaded < 512 * 1024:
            time.sleep(0.01)
        t.stop()
        self.assertTrue(done.wait(10))
        self.assertEqual(d.status, Status.PAUSED)
        paused_at = d.downloaded
        self.assertTrue(0 < paused_at < len(srv.data))
        self.assertTrue(d.segments)

        srv.delay = 0
        srv.requests.clear()
        d2_done = threading.Event()
        t2 = DownloadTask(d, self.client, t.options, on_finish=lambda _t: d2_done.set())
        t2.start()
        self.assertTrue(d2_done.wait(30))
        self.assertEqual(d.status, Status.COMPLETE, d.error)
        self.assertEqual(sha(d.path), srv.sha256)
        # Resumed connections must not start again from byte 0.
        self.assertNotIn("bytes=0-", srv.requests[1:])

    def test_server_without_ranges_uses_one_connection(self):
        srv = self.server(size=1024 * 1024, ranges=False)
        d, t, done = self.task(srv.url, connections=8)
        t.start()
        self.assertTrue(done.wait(20))
        self.assertEqual(d.status, Status.COMPLETE, d.error)
        self.assertFalse(d.resumable)
        self.assertEqual(len(t.connections), 1)
        self.assertEqual(sha(d.path), srv.sha256)

    def test_unknown_size_streams_to_end(self):
        srv = self.server(size=700 * 1024, ranges=False, length=False)
        d, t, done = self.task(srv.url)
        t.start()
        self.assertTrue(done.wait(20))
        self.assertEqual(d.status, Status.COMPLETE, d.error)
        self.assertEqual(d.size, len(srv.data))
        self.assertEqual(sha(d.path), srv.sha256)

    def test_dropped_connection_is_retried(self):
        srv = self.server(size=2 * 1024 * 1024, drop_after=300 * 1024)
        d, t, done = self.task(srv.url, connections=2)
        t.start()
        self.assertTrue(done.wait(30))
        self.assertEqual(d.status, Status.COMPLETE, d.error)
        self.assertEqual(sha(d.path), srv.sha256)

    def test_http_404_is_an_error(self):
        srv = self.server(size=1024)
        d, t, done = self.task(srv.url.replace("/files/", "/missing/"), connections=2)
        # The fixture serves every path, so point at a closed port instead.
        d.url = "http://127.0.0.1:9/nothing"
        t.start()
        self.assertTrue(done.wait(20))
        self.assertEqual(d.status, Status.ERROR)
        self.assertTrue(d.error)

    def test_speed_limit(self):
        srv = self.server(size=600 * 1024)
        d, t, done = self.task(srv.url, connections=4, global_limiter=SpeedLimiter(400))
        start = time.monotonic()
        t.start()
        self.assertTrue(done.wait(30))
        elapsed = time.monotonic() - start
        self.assertEqual(d.status, Status.COMPLETE, d.error)
        self.assertGreater(elapsed, 1.0, "600 KB at 400 KB/s should take over a second")


class HelperTests(unittest.TestCase):
    def test_content_disposition(self):
        self.assertEqual(parse_content_disposition('attachment; filename="a b.zip"'), "a b.zip")
        self.assertEqual(parse_content_disposition("attachment; filename*=UTF-8''%E2%82%AC.txt"), "€.txt")
        self.assertIsNone(parse_content_disposition(None))

    def test_unique_path(self):
        with tempfile.TemporaryDirectory() as tmp:
            p = os.path.join(tmp, "f.zip")
            self.assertEqual(unique_path(p), p)
            open(p, "w").close()
            self.assertEqual(unique_path(p), os.path.join(tmp, "f (2).zip"))


if __name__ == "__main__":
    unittest.main()
