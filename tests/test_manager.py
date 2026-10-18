import os
import tempfile
import time
import unittest
from datetime import datetime

from lider.core import categories, fmt
from lider.core import settings as settings_mod
from lider.core.manager import DownloadManager
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
