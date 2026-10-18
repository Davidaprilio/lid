import json
import os
import shutil
import subprocess
import tempfile
import threading
import time
import unittest

from lider.core import media
from lider.core.engine import EngineOptions, HttpClient
from lider.core.media_task import MediaTask
from lider.core.models import Download, Status
from tests.httpfixture import FileServer

HERE = os.path.dirname(__file__)
FFMPEG = shutil.which("ffmpeg")


def load_formats():
    with open(os.path.join(HERE, "data", "youtube_formats.json")) as f:
        return json.load(f)


class DetectTests(unittest.TestCase):
    def test_youtube_links(self):
        for url in ("https://www.youtube.com/watch?v=8h9mr7EZiA4", "https://youtu.be/8h9mr7EZiA4",
                    "https://m.youtube.com/watch?v=8h9mr7EZiA4&t=10", "https://www.youtube.com/shorts/abc123",
                    "https://music.youtube.com/watch?v=x", "http://youtube.com/live/abc"):
            self.assertTrue(media.is_youtube(url), url)
        for url in ("https://example.com/watch?v=1", "https://www.youtube.com/", "https://youtube.com.evil.io/watch?v=1",
                    "https://www.youtube.com/channel/UC123"):
            self.assertFalse(media.is_youtube(url), url)


class VariantTests(unittest.TestCase):
    """Uses the real format list of https://www.youtube.com/watch?v=8h9mr7EZiA4 (saved 2026-10-18)."""

    def setUp(self):
        data = load_formats()
        self.variants = media.build_variants(data["formats"], data["duration"], True, True)
        self.video = [v for v in self.variants if v.kind == "video"]
        self.audio = [v for v in self.variants if v.kind == "audio"]

    def test_one_variant_per_resolution_best_first(self):
        self.assertEqual([v.quality for v in self.video],
                         ["2160p", "1440p", "1080p", "720p", "480p", "360p", "240p", "144p"])

    def test_h264_mp4_preferred_then_vp9_webm(self):
        by_q = {v.quality: v for v in self.video}
        self.assertEqual((by_q["1080p"].ext, by_q["1080p"].video_format, by_q["1080p"].audio_format), ("mp4", "137", "140"))
        self.assertEqual((by_q["2160p"].ext, by_q["2160p"].video_format, by_q["2160p"].audio_format), ("webm", "313", "251"))
        self.assertEqual(by_q["1080p"].size, 92006373 + 8106621)
        self.assertIn("1080p · MP4 (H.264)", by_q["1080p"].label)

    def test_audio_choices_skip_drc(self):
        self.assertEqual([(v.quality, v.audio_format, v.convert) for v in self.audio],
                         [("M4A", "140", ""), ("MP3", "140", "mp3"), ("OPUS", "251", "opus")])

    def test_without_ffmpeg_only_unmerged_formats(self):
        data = load_formats()
        variants = media.build_variants(data["formats"], data["duration"], False, False)
        self.assertTrue(all(not v.convert for v in variants))
        self.assertTrue(all(v.kind == "audio" or not v.audio_format for v in variants))

    def test_default_variant(self):
        info = media.MediaInfo("u", "id", "t", "", 501, self.variants)
        self.assertEqual(info.default_variant("video").quality, "1080p")
        self.assertEqual(info.default_variant("audio").quality, "M4A")
        self.assertEqual(info.default_variant("video", "480p").quality, "480p")


def make_clip(path: str, kind: str) -> bytes:
    if kind == "video":
        args = ["-f", "lavfi", "-i", "testsrc=size=160x120:rate=10:duration=3", "-c:v", "libx264", "-pix_fmt", "yuv420p"]
    else:
        args = ["-f", "lavfi", "-i", "sine=frequency=440:duration=3", "-c:a", "aac"]
    subprocess.run([FFMPEG, "-y", "-loglevel", "error", *args, path], check=True)
    with open(path, "rb") as f:
        return f.read()


def streams_of(path: str) -> list[str]:
    out = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "stream=codec_type", "-of", "csv=p=0", path],
                         capture_output=True, text=True).stdout
    return sorted(out.split())


@unittest.skipUnless(FFMPEG and shutil.which("ffprobe"), "ffmpeg is required")
class MediaTaskTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp()
        self.video = FileServer(data=make_clip(os.path.join(self.tmp, "v.mp4"), "video"), delay_per_chunk=0.0)
        self.audio = FileServer(data=make_clip(os.path.join(self.tmp, "a.m4a"), "audio"))
        self.resolves = 0

    def tearDown(self):
        self.video.close()
        self.audio.close()
        shutil.rmtree(self.tmp, ignore_errors=True)

    def resolver(self, page_url, ids):
        self.resolves += 1
        urls = {"137": self.video.url, "140": self.audio.url}
        return [{"url": urls[i], "headers": {"User-Agent": "test"}, "size": -1, "ext": "mp4" if i == "137" else "m4a"}
                for i in ids]

    def run_task(self, d):
        done = threading.Event()
        opts = EngineOptions(max_connections=4, retries=1, retry_delay=0.05, temp_dir=os.path.join(self.tmp, "parts"))
        t = MediaTask(d, lambda ua: HttpClient(ua, timeout=5), opts, on_finish=lambda _t: done.set(),
                      resolver=self.resolver)
        t.start()
        return t, done

    def download(self, variant):
        info = media.MediaInfo("https://www.youtube.com/watch?v=x", "x", "Clip", "Me", 3, [variant])
        return Download(1, info.url, f"Clip.{variant.ext}", os.path.join(self.tmp, "out"),
                        media=media.media_record(info, variant), size=variant.size)

    def test_video_and_audio_are_merged(self):
        d = self.download(media.Variant("video", "1080p", "", "mp4", "137", "140"))
        t, done = self.run_task(d)
        self.assertTrue(done.wait(30))
        self.assertEqual(d.status, Status.COMPLETE, d.error)
        self.assertEqual(streams_of(d.path), ["audio", "video"])
        self.assertEqual(d.size, os.path.getsize(d.path))
        self.assertEqual(os.listdir(os.path.join(self.tmp, "parts")), [])

    def test_audio_converted_to_mp3(self):
        d = self.download(media.Variant("audio", "MP3", "", "mp3", "", "140", convert="mp3"))
        t, done = self.run_task(d)
        self.assertTrue(done.wait(30))
        self.assertEqual(d.status, Status.COMPLETE, d.error)
        self.assertTrue(d.path.endswith(".mp3"))
        codec = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "stream=codec_name", "-of", "csv=p=0",
                                d.path], capture_output=True, text=True).stdout.strip()
        self.assertEqual(codec, "mp3")

    def test_pause_resume_refreshes_urls_and_keeps_progress(self):
        self.video.delay = 0.05
        d = self.download(media.Variant("video", "1080p", "", "mp4", "137", "140"))
        t, done = self.run_task(d)
        while d.downloaded == 0:
            time.sleep(0.01)
        t.stop()
        self.assertTrue(done.wait(10))
        self.assertEqual(d.status, Status.PAUSED)
        first = d.media["streams"][0]
        self.assertTrue(first.get("segments") or first.get("done"))
        self.video.delay = 0
        self.video.requests.clear()
        t2, done2 = self.run_task(d)
        self.assertTrue(done2.wait(30))
        self.assertEqual(d.status, Status.COMPLETE, d.error)
        self.assertEqual(self.resolves, 2, "stream URLs must be resolved again on resume")
        self.assertEqual(streams_of(d.path), ["audio", "video"])

    def test_forbidden_stream_gets_a_fresh_url(self):
        self.video.fail_first = 1
        d = self.download(media.Variant("video", "1080p", "", "mp4", "137", "140"))
        t, done = self.run_task(d)
        self.assertTrue(done.wait(30))
        self.assertEqual(d.status, Status.COMPLETE, d.error)
        self.assertEqual(self.resolves, 2)

    def test_suggested_filename_drops_trailing_dot(self):
        info = media.MediaInfo("u", "x", "Review: A07 / A08.", "", 1)
        self.assertEqual(media.suggested_filename(info, media.Variant("video", "1080p", "", "mp4")),
                         "Review: A07 _ A08.mp4")

    def test_resolver_error_is_reported(self):
        def broken(page_url, ids):
            raise LookupError("YouTube no longer offers format 137 for this video")

        d = self.download(media.Variant("video", "1080p", "", "mp4", "137", "140"))
        done = threading.Event()
        opts = EngineOptions(temp_dir=os.path.join(self.tmp, "parts"))
        MediaTask(d, lambda ua: HttpClient(ua), opts, on_finish=lambda _t: done.set(), resolver=broken).start()
        self.assertTrue(done.wait(10))
        self.assertEqual(d.status, Status.ERROR)
        self.assertIn("format 137", d.error)


@unittest.skipUnless(os.environ.get("LIDER_NETWORK_TESTS"), "set LIDER_NETWORK_TESTS=1 to query YouTube")
class LiveYouTubeTests(unittest.TestCase):
    def test_probe_real_video(self):
        info = media.probe("https://www.youtube.com/watch?v=8h9mr7EZiA4")
        self.assertTrue(info.title)
        self.assertTrue(any(v.kind == "video" for v in info.variants))


if __name__ == "__main__":
    unittest.main()
