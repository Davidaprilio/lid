import os
import tempfile
import time
import unittest
from datetime import datetime

from lider.core import categories, checksum, fmt
from lider.core import settings as settings_mod
from lider.core.manager import DownloadManager, QueueSchedule
from lider.core.models import MAIN_QUEUE, SYNC_QUEUE, Download, Status
from lider.core.settings import Settings
from lider.core.store import Store
from tests.httpfixture import FileServer


def wait_for(manager, predicate, timeout=30):
    end = time.monotonic() + timeout
    events = []
    while time.monotonic() < end:
        events += manager.poll()
        if predicate():
            return events
        time.sleep(0.02)
    raise AssertionError("timed out")


class ManagerTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        root = self.tmp.name
        self.settings = Settings(temp_dir=os.path.join(root, "parts"), retry_delay=0.05)
        for c in self.settings.categories:
            c.save_dir = os.path.join(root, "dl", c.name)
        self.store = Store(os.path.join(root, "db.sqlite"))
        self.m = DownloadManager(self.settings, self.store)
        self.srv = FileServer(size=1024 * 1024)

    def tearDown(self):
        self.m.shutdown()
        self.store.close()
        self.srv.close()
        self.tmp.cleanup()

    def test_default_queues_exist(self):
        self.assertIn(MAIN_QUEUE, self.m.queues)
        self.assertTrue(self.m.queues[SYNC_QUEUE].schedule.periodic)

    def test_add_classifies_and_completes(self):
        d = self.m.add(self.srv.url, "movie.mkv", start=True)
        self.assertEqual(d.category, "Video")
        events = wait_for(self.m, lambda: d.status == Status.COMPLETE)
        self.assertTrue(any(e.kind == "complete" and e.download_id == d.id for e in events))
        self.assertTrue(os.path.exists(d.path))
        self.assertTrue(d.path.startswith(self.settings.category("Video").save_dir))

    def test_checksum_is_verified_before_complete(self):
        d = self.m.add(self.srv.url, "a.iso", checksum="sha256:" + self.srv.sha256, start=True)
        events = wait_for(self.m, lambda: d.status == Status.COMPLETE and d.checksum_result)
        self.assertEqual(d.checksum_result, "verified")
        self.assertEqual(d.checksum_actual, self.srv.sha256)
        self.assertEqual([e.kind for e in events if e.download_id == d.id and e.kind != "changed"], ["complete"])

    def test_checksum_mismatch_asks_keeps_or_downloads_again_once(self):
        wrong = "sha256:" + "0" * 64
        d = self.m.add(self.srv.url, "a.iso", checksum=wrong, start=True)
        events = wait_for(self.m, lambda: d.checksum_result == "mismatch")
        self.assertIn(("mismatch", ""), [(e.kind, e.message) for e in events if e.download_id == d.id])
        self.assertEqual(d.status, Status.COMPLETE)
        self.assertNotIn("complete", [e.kind for e in events if e.download_id == d.id])

        self.settings.checksum_mismatch = "redownload"
        requests = len(self.srv.requests)
        self.m.redownload(d.id, overwrite=True)
        events = wait_for(self.m, lambda: d.checksum_retried and d.checksum_result == "mismatch")
        kinds = [e.kind for e in events if e.download_id == d.id and e.kind != "changed"]
        self.assertEqual(kinds, ["mismatch"])  # downloaded again once by itself, then asked
        self.assertGreater(len(self.srv.requests), requests + 1)

    def test_skip_check_and_verify_now(self):
        self.settings.verify_checksum = False
        d = self.m.add(self.srv.url, "a.iso", checksum="sha256:" + self.srv.sha256, start=True)
        wait_for(self.m, lambda: d.status == Status.COMPLETE)
        self.assertEqual(d.checksum_result, "")  # verification turned off in Options
        self.assertTrue(self.m.verify(d.id, manual=True))
        events = wait_for(self.m, lambda: d.status == Status.COMPLETE and d.checksum_result)
        self.assertIn(("checked", "verified"), [(e.kind, e.message) for e in events])
        self.assertNotIn("complete", [e.kind for e in events])
        self.assertFalse(self.m.start(d.id) and self.m.is_running(d.id))

    def test_check_when_finished_runs_even_with_verification_off(self):
        self.settings.verify_checksum = False
        d = self.m.add(self.srv.url, "a.iso")
        self.m.check_when_finished(d.id, "sha256:" + self.srv.sha256)
        self.m.start(d.id)
        events = wait_for(self.m, lambda: d.status == Status.COMPLETE and d.checksum_result)
        self.assertEqual(d.checksum_result, "verified")
        self.assertIn("complete", [e.kind for e in events if e.download_id == d.id])

    def test_queue_runs_items_one_at_a_time_in_order(self):
        ids = [self.m.add(self.srv.url, f"f{i}.zip").id for i in range(3)]
        self.m.set_queue(ids, MAIN_QUEUE)
        self.m.move_in_queue(ids[2], -2)
        self.assertEqual([d.id for d in self.m.queue_items(MAIN_QUEUE)], [ids[2], ids[0], ids[1]])
        self.m.start_queue(MAIN_QUEUE)
        running_seen = 0
        end = time.monotonic() + 30
        events = []
        while time.monotonic() < end:
            events += self.m.poll()
            running_seen = max(running_seen, sum(self.m.is_running(i) for i in ids))
            if any(e.kind == "queue_done" for e in events):
                break
            time.sleep(0.01)
        self.assertTrue(all(self.m.downloads[i].status == Status.COMPLETE for i in ids))
        self.assertLessEqual(running_seen, 1)
        self.assertFalse(self.m.queues[MAIN_QUEUE].running)

    def test_scheduler_starts_and_stops_queue(self):
        d = self.m.add(self.srv.url, "a.zip")
        self.m.set_queue([d.id], MAIN_QUEUE)
        s = self.m.queues[MAIN_QUEUE].schedule
        s.start_enabled, s.start_time, s.daily, s.days = True, "03:15", True, [0]
        monday = datetime(2026, 10, 19, 3, 15)
        self.assertEqual(monday.weekday(), 0)
        self.m.poll(datetime(2026, 10, 20, 3, 15))  # Tuesday: not selected
        self.assertFalse(self.m.queues[MAIN_QUEUE].running)
        events = self.m.poll(monday)
        self.assertTrue(any(e.kind == "queue_started" for e in events))
        self.assertTrue(self.m.queues[MAIN_QUEUE].running)

    def test_remove_and_persist(self):
        d = self.m.add(self.srv.url, "doc.pdf", description="hello")
        again = Store(self.store._db.execute("PRAGMA database_list").fetchone()[2])
        self.assertEqual(again.load_downloads()[0].description, "hello")
        again.close()
        self.m.remove([d.id])
        self.assertEqual(self.store.load_downloads(), [])

    def test_active_status_becomes_paused_after_restart(self):
        d = Download(id=7, url="http://x/y", filename="y", save_dir="/tmp", status=Status.DOWNLOADING)
        self.store.save_download(d)
        m2 = DownloadManager(self.settings, self.store)
        self.assertEqual(m2.downloads[7].status, Status.PAUSED)


class SettingsMigrationTests(unittest.TestCase):
    def test_old_dark_mode_and_toolbar_size_map_to_themes(self):
        s = settings_mod._from_dict({"dark_mode": True, "toolbar_large": False})
        self.assertEqual((s.theme, s.follow_system, s.toolbar_labels), ("ember-dark", False, False))
        s = settings_mod._from_dict({})
        self.assertEqual((s.theme, s.follow_system, s.dark_theme), ("ember", True, "ember-dark"))


class ChecksumParseTests(unittest.TestCase):
    H = "3f9c1e7a5b2d48c6e0a1f7b39d4c2e85a6b0f13d7e9c4a2b58f6d01e3c7a9b42"

    def test_accepted_input(self):
        self.assertEqual(checksum.parse(""), "")
        self.assertEqual(checksum.parse(f"  {self.H.upper()}\n"), "sha256:" + self.H)
        self.assertEqual(checksum.parse(f"SHA-256: {self.H}"), "sha256:" + self.H)
        self.assertEqual(checksum.parse(f"{self.H} *ubuntu.iso"), "sha256:" + self.H)
        self.assertEqual(checksum.parse("d41d8cd98f00b204e9800998ecf8427e"), "md5:d41d8cd98f00b204e9800998ecf8427e")
        self.assertEqual(checksum.parse(self.H, "sha256"), "sha256:" + self.H)

    def test_verifier_computes_without_an_expected_value(self):
        import hashlib

        with tempfile.NamedTemporaryFile(delete=False) as f:
            f.write(b"lider" * 1000)
        try:
            v = checksum.Verifier(f.name, "md5:")
            v.run()
            self.assertEqual((v.result, v.actual), ("computed", hashlib.md5(b"lider" * 1000).hexdigest()))
        finally:
            os.remove(f.name)

    def test_invalid_input_says_why(self):
        with self.assertRaisesRegex(checksum.ChecksumError, "expected 64 hex characters, got 62"):
            checksum.parse(self.H[:-2], "sha256")
        with self.assertRaisesRegex(checksum.ChecksumError, "got 62"):
            checksum.parse(self.H[:-2])
        with self.assertRaisesRegex(checksum.ChecksumError, "“g” is not a hex character"):
            checksum.parse("g" + self.H[1:])
        with self.assertRaisesRegex(checksum.ChecksumError, "marked SHA-256 but MD5"):
            checksum.parse(f"sha256:{self.H}", "md5")


class NextStartTests(unittest.TestCase):
    def test_once_daily_and_off(self):
        now = datetime(2026, 10, 25, 9, 41, 30)  # a Sunday
        s = QueueSchedule(start_enabled=True, start_time="01:00")
        self.assertEqual(s.next_start(now), datetime(2026, 10, 26, 1, 0))  # no date: the next 01:00
        s.start_time = "09:41"
        self.assertEqual(s.next_start(now), datetime(2026, 10, 25, 9, 41))  # this minute still counts
        s.once_date = "2026-10-24"
        self.assertIsNone(s.next_start(now))  # the day has passed
        s = QueueSchedule(start_enabled=True, start_time="08:00", daily=True, days=[0, 2])
        self.assertEqual(s.next_start(now), datetime(2026, 10, 26, 8, 0))  # Monday
        s.days = []
        self.assertIsNone(s.next_start(now))
        self.assertIsNone(QueueSchedule(start_enabled=False).next_start(now))


class CategoryAndFormatTests(unittest.TestCase):
    def test_default_extension_lists(self):
        s = Settings()
        self.assertEqual(categories.classify(s, "a.R01").name, "Compressed")
        self.assertEqual(categories.classify(s, "a.pptx").name, "Documents")
        self.assertEqual(categories.classify(s, "a.flac").name, "General")
        self.assertTrue(categories.is_auto_capture_type(s, "setup.MSI"))
        self.assertFalse(categories.is_auto_capture_type(s, "page.html"))

    def test_site_rule_wins(self):
        s = Settings()
        s.category("Music").sites = ["*.soundhost.com"]
        self.assertEqual(categories.classify(s, "x.zip", "https://cdn.soundhost.com/x.zip").name, "Music")

    def test_settings_round_trip(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = os.path.join(tmp, "s.json")
            s = Settings(max_connections=16)
            s.category("Video").extensions.append("vob")
            settings_mod.save(s, path=__import__("pathlib").Path(path))
            back = settings_mod.load(__import__("pathlib").Path(path))
            self.assertEqual(back.max_connections, 16)
            self.assertIn("vob", back.category("Video").extensions)

    def test_legacy_lindm_folders_are_migrated(self):
        with tempfile.TemporaryDirectory() as tmp:
            env = {k: os.environ.get(k) for k in ("XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME")}
            os.environ.update(XDG_CONFIG_HOME=f"{tmp}/c", XDG_DATA_HOME=f"{tmp}/d", XDG_CACHE_HOME=f"{tmp}/k")
            try:
                for base in ("c", "d", "k"):
                    os.makedirs(f"{tmp}/{base}/lindm")
                with open(f"{tmp}/c/lindm/settings.json", "w") as f:
                    f.write('{"temp_dir": "%s/k/lindm/parts", "max_connections": 16}' % tmp)
                open(f"{tmp}/d/lindm/downloads.db", "w").close()
                os.makedirs(f"{tmp}/d/lider/native-host")  # set up before the first start: must merge
                settings_mod.migrate_legacy_dirs()
                self.assertFalse(os.path.exists(f"{tmp}/c/lindm"))
                self.assertTrue(os.path.exists(f"{tmp}/d/lider/downloads.db"))
                s = settings_mod.load()
                self.assertEqual((s.max_connections, s.temp_dir), (16, f"{tmp}/k/lider/parts"))
            finally:
                for k, v in env.items():
                    if v is None:
                        os.environ.pop(k, None)
                    else:
                        os.environ[k] = v

    def test_formats(self):
        self.assertEqual(fmt.size(512), "512 Bytes")
        self.assertEqual(fmt.size(1536), "1.50 KB")
        self.assertEqual(fmt.size(5 * 1024 ** 3), "5.00 GB")
        self.assertEqual(fmt.duration(125), "2 min 5 sec")
        self.assertEqual(fmt.duration(3720), "1 hour(s) 2 min")
        d = Download(1, "u", "f", "/tmp", size=200, downloaded=50, status=Status.PAUSED)
        self.assertEqual(fmt.status(d), "25.00%")


if __name__ == "__main__":
    unittest.main()
