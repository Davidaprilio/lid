import base64
import hashlib
import json
import os
import stat
import tempfile
import unittest
import zipfile
from pathlib import Path

from lider import __version__, browser

EXT = browser.EXTENSION_SOURCE


class ManifestTests(unittest.TestCase):
    def test_chromium_key_gives_the_registered_extension_id(self):
        key = json.loads((EXT / "manifest.chromium.json").read_text())["key"]
        digest = hashlib.sha256(base64.b64decode(key)).hexdigest()[:32]
        ext_id = "".join(chr(ord("a") + int(c, 16)) for c in digest)
        self.assertEqual(ext_id, browser.CHROMIUM_EXTENSION_ID)

    def test_firefox_id_matches(self):
        gecko = json.loads((EXT / "manifest.firefox.json").read_text())["browser_specific_settings"]["gecko"]
        self.assertEqual(gecko["id"], browser.FIREFOX_EXTENSION_ID)

    def test_manifests_reference_existing_files(self):
        for family in ("chromium", "firefox"):
            m = json.loads((EXT / f"manifest.{family}.json").read_text())
            files = list(m["icons"].values()) + [m["action"]["default_popup"]]
            files += m["background"].get("scripts", []) + [m["background"].get("service_worker", "background.js")]
            for cs in m["content_scripts"]:
                files += cs["js"] + cs.get("css", [])
            for f in files:
                self.assertTrue((EXT / f).is_file(), f"{family}: {f}")
            self.assertIn("nativeMessaging", m["permissions"])

    def test_background_uses_the_same_host_name(self):
        self.assertIn(f'"{browser.HOST_NAME}"', (EXT / "background.js").read_text())


class DistributionTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def manifest(self, folder):
        return json.loads((folder / "manifest.json").read_text())

    def test_store_variant_has_no_youtube_and_no_key(self):
        full = self.manifest(browser.export_extension("chromium", self.dir / "full"))
        store_dir = browser.export_extension("chromium", self.dir / "store", "store")
        store = self.manifest(store_dir)
        self.assertIn("key", full)
        self.assertIn("content_scripts", full)
        self.assertNotIn("key", store)
        self.assertNotIn("content_scripts", store)
        self.assertFalse((store_dir / "youtube.js").exists())
        self.assertEqual(store["version"], __version__)
        self.assertNotIn("YouTube", store["description"])

    def test_update_url_only_for_the_self_distributed_firefox_build(self):
        listed = self.manifest(browser.export_extension("firefox", self.dir / "listed"))
        unlisted = self.manifest(browser.export_extension("firefox", self.dir / "unlisted",
                                                          update_url=browser.FIREFOX_UPDATES_URL))
        self.assertNotIn("update_url", listed["browser_specific_settings"]["gecko"])
        self.assertEqual(unlisted["browser_specific_settings"]["gecko"]["update_url"], browser.FIREFOX_UPDATES_URL)

    def test_release_files_are_reproducible(self):
        browser.build_extensions(str(self.dir / "a"), str(self.dir / "wa"))
        browser.build_extensions(str(self.dir / "b"), str(self.dir / "wb"))
        names = sorted(p.name for p in (self.dir / "a").iterdir())
        self.assertEqual(names, ["firefox-updates.json", "lid-extension-chromium-store.zip",
                                 "lid-extension-chromium.zip", "lid-extension-firefox.zip"])
        for name in names:
            self.assertEqual((self.dir / "a" / name).read_bytes(), (self.dir / "b" / name).read_bytes(), name)
        with zipfile.ZipFile(self.dir / "a" / "lid-extension-chromium-store.zip") as z:
            self.assertIn("manifest.json", z.namelist())
            self.assertNotIn("youtube.js", z.namelist())
        updates = json.loads((self.dir / "a" / "firefox-updates.json").read_text())
        entry = updates["addons"][browser.FIREFOX_EXTENSION_ID]["updates"][0]
        self.assertEqual(entry["version"], __version__)
        self.assertTrue(entry["update_link"].endswith(f"/v{__version__}/lid-extension-firefox.xpi"))

    def test_host_accepts_every_known_chromium_id(self):
        old = browser.CHROMIUM_EXTENSION_IDS
        browser.CHROMIUM_EXTENSION_IDS = (old[0], "abcdefghijklmnopabcdefghijklmnop")
        try:
            origins = browser.host_manifest("chromium", "/x")["allowed_origins"]
        finally:
            browser.CHROMIUM_EXTENSION_IDS = old
        self.assertEqual(origins, [f"chrome-extension://{old[0]}/", "chrome-extension://abcdefghijklmnopabcdefghijklmnop/"])


class InstallTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.home = Path(self.tmp.name)
        self.old = os.environ.get("XDG_DATA_HOME")
        os.environ["XDG_DATA_HOME"] = str(self.home / "data")
        (self.home / ".config/BraveSoftware/Brave-Browser").mkdir(parents=True)
        (self.home / ".mozilla").mkdir()

    def tearDown(self):
        if self.old is None:
            os.environ.pop("XDG_DATA_HOME", None)
        else:
            os.environ["XDG_DATA_HOME"] = self.old
        self.tmp.cleanup()

    def test_detect_install_uninstall(self):
        found = browser.detected(self.home)
        self.assertEqual([b.name for b in found], ["Brave", "Firefox"])
        exe = browser.install(found, self.home)
        self.assertTrue(exe.stat().st_mode & stat.S_IXUSR)
        self.assertIn("lider.native_host", exe.read_text())
        brave = json.loads((self.home / ".config/BraveSoftware/Brave-Browser/NativeMessagingHosts"
                            / f"{browser.HOST_NAME}.json").read_text())
        self.assertEqual(brave["allowed_origins"], [f"chrome-extension://{browser.CHROMIUM_EXTENSION_ID}/"])
        self.assertEqual(brave["path"], str(exe))
        fox = json.loads((self.home / ".mozilla/native-messaging-hosts" / f"{browser.HOST_NAME}.json").read_text())
        self.assertEqual(fox["allowed_extensions"], [browser.FIREFOX_EXTENSION_ID])
        self.assertTrue(all(browser.is_installed(b, self.home) for b in found))
        browser.uninstall(found, self.home)
        self.assertFalse(any(browser.is_installed(b, self.home) for b in found))

    def test_firefox_in_the_xdg_config_folder(self):
        """Firefox 147+ without ~/.mozilla keeps profiles and hosts in ~/.config/mozilla."""
        (self.home / ".mozilla").rmdir()
        (self.home / ".config/mozilla/firefox").mkdir(parents=True)
        found = browser.detected(self.home)
        self.assertIn("Firefox", [b.name for b in found])
        fox = next(b for b in found if b.name == "Firefox")
        browser.install([fox], self.home)
        self.assertTrue((self.home / ".config/mozilla/native-messaging-hosts" / f"{browser.HOST_NAME}.json").is_file())
        self.assertFalse((self.home / ".mozilla").exists())
        self.assertTrue(browser.is_installed(fox, self.home))
        browser.uninstall([fox], self.home)
        self.assertFalse(browser.is_installed(fox, self.home))

    def test_appimage_wrapper_runs_the_appimage(self):
        os.environ["APPIMAGE"] = "/opt/apps/Lider 1.0.AppImage"
        try:
            script = browser.wrapper_script()
        finally:
            del os.environ["APPIMAGE"]
        self.assertIn("exec '/opt/apps/Lider 1.0.AppImage' --native-host", script)

    def test_export_extension_per_family(self):
        dest = browser.export_extension("firefox", self.home / "ext")
        self.assertEqual(json.loads((dest / "manifest.json").read_text())["browser_specific_settings"]["gecko"]["id"],
                         browser.FIREFOX_EXTENSION_ID)
        self.assertFalse(list(dest.glob("manifest.*.json")))
        self.assertTrue((dest / "background.js").is_file())

    def test_system_files_for_packages(self):
        browser.write_system_files(str(self.home / "root"), "/usr/lib/lider/lider-native-host", "/usr/bin/python3",
                                   "/usr/lib/lider")
        root = self.home / "root"
        host = (root / "usr/lib/lider/lider-native-host").read_text()
        self.assertIn("PYTHONPATH=/usr/lib/lider exec /usr/bin/python3 -m lider.native_host", host)
        chrome = json.loads((root / "etc/opt/chrome/native-messaging-hosts" / f"{browser.HOST_NAME}.json").read_text())
        self.assertEqual(chrome["path"], "/usr/lib/lider/lider-native-host")
        self.assertTrue((root / "usr/lib/mozilla/native-messaging-hosts" / f"{browser.HOST_NAME}.json").is_file())
        self.assertTrue((root / "usr/share/lider/browser-extension/chromium/manifest.json").is_file())


if __name__ == "__main__":
    unittest.main()
