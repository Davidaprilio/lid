import json
import tempfile
import unittest
from pathlib import Path

from lider.core.models import Download as _Download
from lider.core.search import (
    FindNext,
    Matcher,
    SearchFlags,
    SearchSettings,
    SearchSettingsError,
    filter_downloads,
)

F = SearchFlags


def Download(id, filename, description="", url="", parent_page="", referer=""):
    return _Download(id, url, filename, "/tmp", description=description, parent_page=parent_page, referer=referer)
ROWS = [
    Download(1, "ubuntu.iso", "Ubuntu LTS", "https://releases.ubuntu.com/u.iso", "https://ubuntu.com/download", "https://ubuntu.com/"),
    Download(2, "Straße.pdf", "map", "https://example.de/s.pdf"),
    Download(3, "setup.exe", "converter", "https://cdn.vendor.com/setup.exe", referer="https://www.google.com/"),
    Download(4, "Setup.exe", "", "https://mirror.example.org/Setup.exe"),
    Download(5, "lecture-01.mp4", "lecture 1", "https://video.uni.edu/01.mp4"),
]


def ids(rows):
    return [d.id for d in rows]


class MatcherTests(unittest.TestCase):
    def test_empty_query_matches_everything(self):
        self.assertTrue(all(Matcher(SearchSettings("", F.TARGET_NAME))(d) for d in ROWS))

    def test_case_insensitive_by_default(self):
        s = SearchSettings("SETUP", F.TARGET_NAME, active=True)
        self.assertEqual(ids(filter_downloads(ROWS, s)), [3, 4])

    def test_match_case(self):
        s = SearchSettings("Setup", F.TARGET_NAME | F.MATCH_CASE, active=True)
        self.assertEqual(ids(filter_downloads(ROWS, s)), [4])

    def test_casefold_handles_sharp_s(self):
        s = SearchSettings("STRASSE", F.TARGET_NAME, active=True)
        self.assertEqual(ids(filter_downloads(ROWS, s)), [2])

    def test_whole_string_is_full_field_equality(self):
        s = SearchSettings("setup", F.TARGET_NAME | F.WHOLE_STRING, active=True)
        self.assertEqual(ids(filter_downloads(ROWS, s)), [])
        s.query = "setup.exe"
        self.assertEqual(ids(filter_downloads(ROWS, s)), [3, 4])

    def test_targets_are_independent(self):
        s = SearchSettings("ubuntu.com", F.TARGET_NAME, active=True)
        self.assertEqual(ids(filter_downloads(ROWS, s)), [])
        s.flags = F.TARGET_LINKS
        self.assertEqual(ids(filter_downloads(ROWS, s)), [1])

    def test_links_include_parent_page_and_referer(self):
        s = SearchSettings("google", F.TARGET_LINKS, active=True)
        self.assertEqual(ids(filter_downloads(ROWS, s)), [3])

    def test_description_target(self):
        s = SearchSettings("lecture", F.TARGET_DESCRIPTION, active=True)
        self.assertEqual(ids(filter_downloads(ROWS, s)), [5])

    def test_no_target_is_rejected_and_name_reenabled(self):
        s = SearchSettings("x", F.MATCH_CASE, active=True)
        with self.assertRaises(SearchSettingsError):
            filter_downloads(ROWS, s)
        self.assertIn(F.TARGET_NAME, s.flags)

    def test_inactive_filter_shows_all(self):
        s = SearchSettings("nothing-matches", F.TARGET_NAME, active=False)
        self.assertEqual(ids(filter_downloads(ROWS, s)), ids(ROWS))


class FindNextTests(unittest.TestCase):
    def test_walks_then_wraps_then_reports_none(self):
        f = FindNext(SearchSettings("setup", F.TARGET_NAME))
        self.assertEqual(f.next(ROWS), 2)
        self.assertEqual(f.next(ROWS), 3)
        self.assertEqual(f.next(ROWS), 2)  # wrapped to the top
        f.settings.query = "missing"
        self.assertIsNone(f.next(ROWS))

    def test_new_query_restarts_from_top(self):
        f = FindNext(SearchSettings("lecture", F.TARGET_NAME))
        f.cursor = 4
        f._last_query = "old"
        self.assertEqual(f.next(ROWS), 4)
        f.settings.query = "ubuntu"
        self.assertEqual(f.next(ROWS), 0)


if __name__ == "__main__":
    unittest.main()
