import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from PySide6.QtWidgets import QApplication  # noqa: E402

from lider import browser, desktop  # noqa: E402
from lider.core.settings import Settings  # noqa: E402

app = QApplication.instance() or QApplication([])

APPIMAGE = "/opt/apps/Lider 1.2.0.AppImage"
ENV = ("XDG_CONFIG_HOME", "XDG_DATA_HOME", "APPIMAGE")


class DesktopTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.old = {k: os.environ.get(k) for k in ENV}
        os.environ.update(XDG_CONFIG_HOME=str(self.root / "config"), XDG_DATA_HOME=str(self.root / "data"),
                          APPIMAGE=APPIMAGE)
        patches = [mock.patch.object(desktop, "SYSTEM_ENTRY", self.root / "usr" / "lider.desktop"),
                   mock.patch.object(desktop, "_update_menus", lambda folder: None)]
        for p in patches:
            p.start()
            self.addCleanup(p.stop)

    def tearDown(self):
        for k, v in self.old.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        self.tmp.cleanup()

    def test_exec_quoting(self):
        self.assertEqual(desktop._exec_arg("/usr/bin/lider"), "/usr/bin/lider")
        self.assertEqual(desktop._exec_arg("/a b/LID"), '"/a b/LID"')
        self.assertEqual(desktop._exec_arg("/a/$x"), '"/a/\\\\$x"')
        self.assertEqual(desktop._exec_arg('/a/"b"'), '"/a/\\\\"b\\\\""')
        self.assertEqual(desktop._exec_arg("/a/100%"), "/a/100%%")

    def test_menu_entry_runs_the_appimage(self):
        text = desktop.menu_entry_text()
        self.assertIn(f'Exec="{APPIMAGE}" %U\n', text)
        self.assertIn(f"TryExec={APPIMAGE}\n", text)
        self.assertIn("Icon=lider\n", text)
        self.assertIn(f'Exec="{APPIMAGE}" --minimized\n', desktop.autostart_text())

    def test_entry_passes_desktop_file_validate(self):
        tool = shutil.which("desktop-file-validate")
        if not tool:
            self.skipTest("desktop-file-validate is not installed")
        path = self.root / "lider.desktop"
        path.write_text(desktop.menu_entry_text())
        out = subprocess.run([tool, str(path)], capture_output=True, text=True)
        self.assertNotIn("error", out.stdout.lower(), out.stdout)

    def test_add_and_remove(self):
        self.assertFalse(desktop.in_menu())
        desktop.add_to_menu()
        self.assertTrue(desktop.in_menu())
        self.assertEqual(desktop.icon_path().read_bytes(), desktop.ICON_SOURCE.read_bytes())
        self.assertEqual(desktop.menu_entry_path(), self.root / "data/applications/lider.desktop")
        desktop.remove_from_menu()
        self.assertFalse(desktop.in_menu())
        self.assertFalse(desktop.icon_path().exists())

    def test_autostart(self):
        desktop.set_autostart(True)
        self.assertTrue(desktop.autostart_enabled())
        self.assertEqual(desktop.autostart_path(), self.root / "config/autostart/lider.desktop")
        desktop.set_autostart(False)
        self.assertFalse(desktop.autostart_enabled())

    def test_refresh_follows_a_moved_appimage(self):
        desktop.add_to_menu()
        desktop.set_autostart(True)
        browser.install([])
        self.assertEqual(desktop.refresh(), [])
        os.environ["APPIMAGE"] = "/home/me/Apps/Lider-1.3.0-x86_64.AppImage"
        self.assertEqual(desktop.refresh(), ["menu", "autostart", "browser helper"])
        self.assertIn("Exec=/home/me/Apps/Lider-1.3.0-x86_64.AppImage %U", desktop.menu_entry_path().read_text())
        self.assertIn("exec /home/me/Apps/Lider-1.3.0-x86_64.AppImage --native-host",
                      browser.host_executable().read_text())
        self.assertFalse(desktop.listed_elsewhere())

    def test_refresh_leaves_missing_files_alone(self):
        self.assertEqual(desktop.refresh(), [])
        self.assertFalse(desktop.in_menu())
        self.assertFalse(desktop.autostart_enabled())

    def test_a_checkout_does_not_take_over_the_entries(self):
        desktop.add_to_menu()
        before = desktop.menu_entry_path().read_text()
        del os.environ["APPIMAGE"]
        with mock.patch.object(desktop.shutil, "which", lambda name: None):
            self.assertEqual(desktop.launch_argv()[0], "env")
            self.assertEqual(desktop.refresh(), [])
        self.assertEqual(desktop.menu_entry_path().read_text(), before)

    def test_an_entry_from_a_menu_editor_is_left_alone(self):
        folder = desktop.menu_entry_path().parent
        folder.mkdir(parents=True)
        mine = "[Desktop Entry]\nType=Application\nName=My downloader\nExec=lider %U\n"
        desktop.menu_entry_path().write_text(mine)
        self.assertEqual(desktop.refresh(), [])
        self.assertEqual(desktop.menu_entry_path().read_text(), mine)

    def test_an_old_autostart_entry_is_fixed(self):
        desktop.autostart_path().parent.mkdir(parents=True)
        desktop.autostart_path().write_text("[Desktop Entry]\nType=Application\nName=LID\n"
                                            "Exec=python3 -m lider --minimized\nX-GNOME-Autostart-enabled=true\n")
        self.assertEqual(desktop.refresh(), ["autostart"])
        self.assertIn(f'Exec="{APPIMAGE}" --minimized', desktop.autostart_path().read_text())

    def test_package_entry_takes_over(self):
        desktop.add_to_menu()
        desktop.SYSTEM_ENTRY.parent.mkdir(parents=True)
        desktop.SYSTEM_ENTRY.write_text("[Desktop Entry]\n")
        self.assertIn("menu", desktop.refresh())
        self.assertFalse(desktop.in_menu())

    def test_offer_once_and_only_to_an_appimage(self):
        from lider.ui.dialogs.menu import should_offer

        s = Settings()
        self.assertTrue(should_offer(s))
        s.menu_offered = True
        self.assertFalse(should_offer(s))
        s.menu_offered = False
        folder = desktop.menu_entry_path().parent
        folder.mkdir(parents=True)
        (folder / "appimagekit_lider.desktop").write_text(f"[Desktop Entry]\nExec={APPIMAGE}\n")
        self.assertTrue(desktop.listed_elsewhere())
        self.assertFalse(should_offer(s))
        del os.environ["APPIMAGE"]
        self.assertFalse(should_offer(Settings()))

    def test_options_general_page(self):
        from lider.ui.dialogs.options import OptionsDialog

        s = Settings()
        dlg = OptionsDialog(s)
        self.assertTrue(dlg.menu.isEnabled())
        self.assertFalse(dlg.menu.isChecked())
        dlg.menu.setChecked(True)
        dlg.launch.setChecked(True)
        dlg.accept()
        self.assertTrue(desktop.in_menu())
        self.assertIn(f'Exec="{APPIMAGE}" --minimized', desktop.autostart_path().read_text())
        self.assertTrue(s.launch_on_startup)
        desktop.SYSTEM_ENTRY.parent.mkdir(parents=True)
        desktop.SYSTEM_ENTRY.write_text("[Desktop Entry]\n")
        dlg = OptionsDialog(s)
        self.assertFalse(dlg.menu.isEnabled())
        self.assertTrue(dlg.menu.isChecked())
        self.assertTrue(dlg.launch.isChecked())


if __name__ == "__main__":
    unittest.main()
