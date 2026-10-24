"""Offscreen smoke tests for the main window."""

import os
import tempfile
import time
import unittest

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from PySide6.QtWidgets import QApplication, QMenu, QMessageBox  # noqa: E402

from lider.core.manager import DownloadManager  # noqa: E402
from lider.core.models import MAIN_QUEUE, Status  # noqa: E402
from lider.core.settings import Settings  # noqa: E402
from lider.core.store import Store  # noqa: E402
from tests.httpfixture import FileServer  # noqa: E402

app = QApplication.instance() or QApplication([])

from lider.ui.main_window import MainWindow  # noqa: E402


class MainWindowTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        os.environ["XDG_CONFIG_HOME"] = os.path.join(self.tmp.name, "config")
        s = Settings(temp_dir=os.path.join(self.tmp.name, "parts"), show_complete_dialog=False,
                     show_progress_dialog=False, monitor_clipboard=False)
        for c in s.categories:
            c.save_dir = os.path.join(self.tmp.name, "dl", c.name)
        self.store = Store(os.path.join(self.tmp.name, "db.sqlite"))
        self.m = DownloadManager(s, self.store)
        self.w = MainWindow(self.m)
        self.w.tray = None  # closing must not hide to tray in tests

    def tearDown(self):
        self.w.really_quit = True
        self.w.timer.stop()
        self.m.shutdown()
        self.w.deleteLater()
        self.store.close()
        self.tmp.cleanup()

    def pump(self, until, timeout=30):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            app.processEvents()
            self.w.tick()
            if until():
                return
            time.sleep(0.02)
        self.fail("timed out")

    def test_menu_bar_layout(self):
        titles = [a.text() for a in self.w.menuBar().actions()]
        self.assertEqual(titles, ["&Tasks", "&File", "&Downloads", "&View", "&Help"])
        downloads = next(m for m in self.w.findChildren(QMenu) if m.title() == "&Downloads")
        texts = [a.text() for a in downloads.actions() if not a.isSeparator()]
        for expected in ("&Pause All", "&Stop All", "&Delete All Completed", "Find (Ctrl-F)", "Find Next (F3)",
                         "S&cheduler", "Start &queue", "S&top queue", "Speed &Limiter", "&Options"):
            self.assertIn(expected, texts)

    def test_toolbar_captions(self):
        captions = [b.text() for b in self.w._toolbar_buttons]
        self.assertEqual(captions, ["Add URL", "Start", "Stop", "Delete", "Options", "Scheduler", "Grabber"])

    def test_category_tree(self):
        top = [self.w.tree.topLevelItem(i).text(0) for i in range(self.w.tree.topLevelItemCount())]
        self.assertEqual(top, ["All Downloads", "Unfinished", "Finished", "Grabber projects", "Queues"])
        all_dl = self.w.tree.topLevelItem(0)
        self.assertEqual([all_dl.child(i).text(0) for i in range(all_dl.childCount())],
                         ["Compressed", "Documents", "Music", "Programs", "Video"])

    def test_searchbar_filter_and_find_next(self):
        for name in ("lecture-01.mp4", "setup.exe", "lecture-02.mp4"):
            self.m.add(f"https://example.com/{name}", name)
        self.w.refresh_rows()
        self.assertEqual(len(self.w.model.rows), 3)
        self.w.show_searchbar()
        self.w.searchbar.edit.setText("LECTURE")
        self.w.apply_search()
        self.assertEqual([d.filename for d in self.w.model.rows], ["lecture-01.mp4", "lecture-02.mp4"])
        self.w.close_searchbar()
        self.assertEqual(len(self.w.model.rows), 3)
        self.w.find_next()
        self.assertEqual(self.w.selected_ids(), [self.w.model.rows[0].id])
        self.w.find_next()
        self.assertEqual(self.w.selected_ids(), [self.w.model.rows[2].id])

    def test_tree_filters(self):
        a = self.m.add("https://example.com/a.zip", "a.zip")
        b = self.m.add("https://example.com/b.mp3", "b.mp3")
        b.status = Status.COMPLETE
        self.m.set_queue([a.id], MAIN_QUEUE)
        self.w.tree_filter = ("finished", None)
        self.w.refresh_rows()
        self.assertEqual([d.id for d in self.w.model.rows], [b.id])
        self.w.tree_filter = ("all", "Compressed")
        self.w.refresh_rows()
        self.assertEqual([d.id for d in self.w.model.rows], [a.id])
        self.w.tree_filter = ("queue", MAIN_QUEUE)
        self.w.refresh_rows()
        self.assertEqual([d.id for d in self.w.model.rows], [a.id])

    def test_ef2_round_trip_format(self):
        text = "<\r\nhttps://x.com/a.zip\r\nreferer: https://x.com/\r\n>\r\n<\r\nhttps://x.com/b.pdf\r\n>\r\n"
        self.assertEqual(MainWindow.parse_ef2(text), [("https://x.com/a.zip", "https://x.com/"),
                                                     ("https://x.com/b.pdf", "")])

    def test_download_through_window(self):
        srv = FileServer(size=2 * 1024 * 1024)
        try:
            d = self.m.add(srv.url, "file.zip", start=True)
            self.pump(lambda: d.status == Status.COMPLETE)
            self.assertTrue(os.path.exists(d.path))
            row = next(r for r in self.w.model.rows if r.id == d.id)
            self.assertEqual(self.w.model.text(row, "status"), "Complete")
        finally:
            srv.close()

    def test_browser_download_request_reaches_add_url(self):
        calls = []
        self.w.add_url = lambda url, **kw: calls.append((url, kw))
        self.w.handle_browser_request({"action": "download", "url": "https://x.org/a.zip", "cookies": "s=1",
                                       "user_agent": "UA", "page_url": "https://x.org/p", "filename": "a.zip"})
        self.w.handle_browser_request({"action": "download", "url": "javascript:alert(1)"})
        self.assertEqual(calls, [("https://x.org/a.zip", {
            "referer": "https://x.org/p", "parent_page": "https://x.org/p", "ask_address": False,
            "headers": {"Cookie": "s=1", "User-Agent": "UA"}, "filename": "a.zip"})])

    def test_browser_links_request_adds_to_queue_with_cookies(self):
        from lider.ui.dialogs import add as add_dialogs

        orig = add_dialogs.LinksDialog.exec
        add_dialogs.LinksDialog.exec = lambda self_: 1
        import lider.ui.main_window as mw
        mw.LinksDialog.exec = lambda self_: 1
        started = []
        self.m.start_queue = lambda name: started.append(name) or []
        try:
            self.w.handle_browser_request({"action": "links", "page_url": "https://x.org/p", "cookies": "s=1",
                                           "items": [{"url": "https://x.org/a.zip", "text": "A", "kind": "a"}]})
        finally:
            add_dialogs.LinksDialog.exec = orig
            mw.LinksDialog.exec = orig
        d = next(iter(self.m.downloads.values()))
        self.assertEqual((d.url, d.headers, d.referer, d.queue), ("https://x.org/a.zip", {"Cookie": "s=1"},
                                                                  "https://x.org/p", MAIN_QUEUE))
        self.assertEqual(started, [MAIN_QUEUE])

    def test_remove_selected_asks_and_removes(self):
        d = self.m.add("https://example.com/a.zip", "a.zip")
        self.w.refresh_rows()
        self.w.view.setCurrentIndex(self.w.model.index(0, 0))
        orig = QMessageBox.exec
        QMessageBox.exec = lambda self_: QMessageBox.Yes
        try:
            self.w.remove_selected()
        finally:
            QMessageBox.exec = orig
        self.assertNotIn(d.id, self.m.downloads)


class BrowserDialogTests(unittest.TestCase):
    def test_get_the_extension_links_point_at_release_files(self):
        import pathlib

        from PySide6.QtWidgets import QPushButton

        from lider import browser
        from lider.ui.dialogs.browsers import BrowserIntegrationDialog

        with tempfile.TemporaryDirectory() as tmp:
            browser.build_extensions(os.path.join(tmp, "dist"), os.path.join(tmp, "work"))
            built = {p.name for p in pathlib.Path(tmp, "dist").iterdir()}
        if browser.FIREFOX_SIGNED_XPI:
            built.add("lid-extension-firefox.xpi")  # made by web-ext sign in the CI
        dialog = BrowserIntegrationDialog()
        try:
            urls = [b.toolTip() for b in dialog.findChildren(QPushButton) if b.toolTip().startswith("https://")]
        finally:
            dialog.deleteLater()
        self.assertIn(browser.RELEASES_URL, urls)
        for url in urls:
            if "/download/" in url:
                self.assertIn(url.rsplit("/", 1)[1], built, url)


if __name__ == "__main__":
    unittest.main()
