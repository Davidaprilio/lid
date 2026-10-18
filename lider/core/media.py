"""YouTube support: detect a link, list the variants, resolve fresh stream URLs.

yt-dlp is used only to read the video page (title, formats, direct stream
URLs). The streams themselves are fetched by LID's multi-connection engine
(see media_task.py) and joined or converted with ffmpeg, which makes video downloads much faster than a
single connection.
"""

from __future__ import annotations

import functools
import re
import shutil
import subprocess
from dataclasses import asdict, dataclass, field

YOUTUBE_RE = re.compile(
    r"^https?://(?:(?:www|m|music)\.)?(?:youtube\.com/(?:watch\?|shorts/|live/|embed/)|youtu\.be/)\S+",
    re.I,
)

AUDIO_CHOICES = ("m4a", "mp3", "opus")


def is_youtube(url: str) -> bool:
    return bool(YOUTUBE_RE.match(url.strip()))


def available() -> bool:
    try:
        import yt_dlp  # noqa: F401
    except ImportError:
        return False
    return True


@functools.lru_cache(maxsize=1)
def ffmpeg() -> str | None:
    return shutil.which("ffmpeg")


@functools.lru_cache(maxsize=1)
def ffmpeg_has_mp3() -> bool:
    exe = ffmpeg()
    if not exe:
        return False
    try:
        out = subprocess.run([exe, "-hide_banner", "-encoders"], capture_output=True, text=True, timeout=10).stdout
    except (OSError, subprocess.SubprocessError):
        return False
    return "libmp3lame" in out


def js_runtimes() -> dict:
    """YouTube needs a JavaScript runtime to unlock its streams; use whichever is installed."""
    for name in ("deno", "node", "bun"):
        path = shutil.which(name)
        if path:
            return {name: {"path": path}}
    return {}


def _ydl_options() -> dict:
    opts = {"quiet": True, "no_warnings": True, "noprogress": True, "skip_download": True, "noplaylist": True}
    runtimes = js_runtimes()
    if runtimes:
        opts["js_runtimes"] = runtimes
    return opts


def extract_info(url: str) -> dict:
    import yt_dlp

    with yt_dlp.YoutubeDL(_ydl_options()) as ydl:
        return ydl.extract_info(url, download=False)


# --------------------------------------------------------------------------- variants

@dataclass
class Variant:
    kind: str  # video | audio
    quality: str  # "1080p", "720p60", "M4A"...
    label: str
    ext: str  # extension of the saved file
    video_format: str = ""
    audio_format: str = ""
    size: int = -1
    height: int = 0
    convert: str = ""  # mp3 | opus | "" (remux only)


@dataclass
class MediaInfo:
    url: str
    id: str
    title: str
    uploader: str
    duration: int
    variants: list[Variant] = field(default_factory=list)

    def default_variant(self, kind: str = "video", quality: str = "") -> Variant | None:
        options = [v for v in self.variants if v.kind == kind] or self.variants
        if not options:
            return None
        for v in options:
            if v.quality == quality:
                return v
        if kind == "video":
            # Prefer the best H.264 MP4 up to 1080p: plays everywhere.
            fitting = [v for v in options if v.height <= 1080 and v.ext == "mp4"]
            return fitting[0] if fitting else options[0]
        return options[0]


def _size(f: dict, duration: float) -> int:
    n = f.get("filesize") or f.get("filesize_approx")
    if n:
        return int(n)
    if f.get("tbr") and duration:
        return int(f["tbr"] * 1000 / 8 * duration)
    return -1


def _usable(f: dict) -> bool:
    return (f.get("protocol") or "").startswith("http") and "drc" not in str(f.get("format_id", ""))


def _quality(f: dict) -> str:
    note = str(f.get("format_note") or "")
    m = re.match(r"(\d{3,4}p)", note)
    base = m.group(1) if m else f"{f.get('height')}p"
    fps = f.get("fps") or 0
    return f"{base}{int(fps)}" if fps and fps > 30 else base


def _codec(f: dict) -> str:
    v = str(f.get("vcodec") or "")
    if v.startswith("avc1"):
        return "H.264"
    if v.startswith(("vp9", "vp09")):
        return "VP9"
    if v.startswith("av01"):
        return "AV1"
    return v.split(".")[0].upper()


def _fmt_size(n: int) -> str:
    if n < 0:
        return "size unknown"
    if n < 1024 * 1024:
        return f"{n / 1024:.0f} KB"
    if n < 1024 ** 3:
        return f"{n / 1024 ** 2:.1f} MB"
    return f"{n / 1024 ** 3:.2f} GB"


def build_variants(formats: list[dict], duration: float, have_ffmpeg: bool, have_mp3: bool) -> list[Variant]:
    usable = [f for f in formats if _usable(f)]
    audio = [f for f in usable if f.get("vcodec") == "none" and f.get("acodec") not in (None, "none")]
    best = lambda fs: max(fs, key=lambda f: (f.get("abr") or f.get("tbr") or 0)) if fs else None  # noqa: E731
    m4a = best([f for f in audio if f.get("ext") == "m4a"])
    opus = best([f for f in audio if f.get("ext") == "webm"])
    out: list[Variant] = []

    if have_ffmpeg:
        video_only = [f for f in usable if f.get("vcodec") not in (None, "none") and f.get("acodec") == "none"
                      and f.get("height")]
        by_quality: dict[str, list[dict]] = {}
        for f in video_only:
            by_quality.setdefault(_quality(f), []).append(f)
        rank = {"H.264": 0, "VP9": 1, "AV1": 2}
        for q, fs in by_quality.items():
            f = min(fs, key=lambda f: (rank.get(_codec(f), 9), -(f.get("tbr") or 0)))
            if _codec(f) in ("H.264", "AV1") and m4a:
                audio_f, ext = m4a, "mp4"
            elif opus:
                audio_f, ext = opus, "webm"
            elif m4a:
                audio_f, ext = m4a, "mkv"
            else:
                continue
            size = _size(f, duration)
            a_size = _size(audio_f, duration)
            total = size + a_size if size >= 0 and a_size >= 0 else -1
            label = f"{q} · {ext.upper()} ({_codec(f)}) · {_fmt_size(total)}"
            out.append(Variant("video", q, label, ext, str(f["format_id"]), str(audio_f["format_id"]), total,
                               int(f.get("height") or 0)))
    else:
        for f in usable:
            if f.get("vcodec") not in (None, "none") and f.get("acodec") not in (None, "none"):
                q = _quality(f)
                size = _size(f, duration)
                out.append(Variant("video", q, f"{q} · {str(f.get('ext')).upper()} · {_fmt_size(size)}",
                                   str(f.get("ext")), str(f["format_id"]), "", size, int(f.get("height") or 0)))
    out.sort(key=lambda v: (-v.height, v.quality))

    if m4a:
        size = _size(m4a, duration)
        kbps = int(m4a.get("abr") or 0)
        out.append(Variant("audio", "M4A", f"M4A (AAC {kbps} kbps) · {_fmt_size(size)}", "m4a", "",
                           str(m4a["format_id"]), size))
        if have_mp3:
            est = int(duration * 192_000 / 8) if duration else -1
            out.append(Variant("audio", "MP3", f"MP3 (192 kbps, converted) · ~{_fmt_size(est)}", "mp3", "",
                               str(m4a["format_id"]), est, convert="mp3"))
    if opus and have_ffmpeg:
        size = _size(opus, duration)
        kbps = int(opus.get("abr") or 0)
        out.append(Variant("audio", "OPUS", f"Opus ({kbps} kbps) · {_fmt_size(size)}", "opus", "",
                           str(opus["format_id"]), size, convert="opus"))
    return out


def probe(url: str) -> MediaInfo:
    info = extract_info(url)
    duration = float(info.get("duration") or 0)
    return MediaInfo(
        url=info.get("webpage_url") or url,
        id=str(info.get("id") or ""),
        title=info.get("title") or "video",
        uploader=info.get("uploader") or info.get("channel") or "",
        duration=int(duration),
        variants=build_variants(info.get("formats") or [], duration, bool(ffmpeg()), ffmpeg_has_mp3()),
    )


def suggested_filename(info: MediaInfo, v: Variant) -> str:
    from .engine import safe_filename

    return f"{safe_filename(info.title).rstrip('. ')}.{v.ext}"


def media_record(info: MediaInfo, v: Variant) -> dict:
    """The ``Download.media`` value saved for a chosen variant."""
    streams = []
    if v.video_format:
        streams.append({"kind": "video", "format_id": v.video_format})
    if v.audio_format:
        streams.append({"kind": "audio", "format_id": v.audio_format})
    return {
        "source": "youtube",
        "page_url": info.url,
        "video_id": info.id,
        "title": info.title,
        "uploader": info.uploader,
        "variant": asdict(v),
        "streams": streams,
    }


def resolve_streams(page_url: str, format_ids: list[str]) -> list[dict]:
    """Fresh direct URLs (they expire after a few hours) for the given format ids."""
    info = extract_info(page_url)
    formats = {str(f.get("format_id")): f for f in info.get("formats") or []}
    duration = float(info.get("duration") or 0)
    out = []
    for fid in format_ids:
        f = formats.get(fid)
        if f is None:
            raise LookupError(f"YouTube no longer offers format {fid} for this video")
        out.append({"url": f["url"], "headers": dict(f.get("http_headers") or {}),
                    "size": _size(f, duration) if f.get("filesize") else -1, "ext": f.get("ext") or "bin"})
    return out


def postprocess_command(inputs: list[str], output: str, variant: dict) -> list[str] | None:
    """ffmpeg command that turns the downloaded streams into the final file, or None to just move it."""
    exe = ffmpeg()
    if exe is None:
        return None
    base = [exe, "-y", "-hide_banner", "-loglevel", "error"]
    if len(inputs) == 2:
        cmd = base + ["-i", inputs[0], "-i", inputs[1], "-map", "0:v:0", "-map", "1:a:0", "-c", "copy"]
        if variant.get("ext") == "mp4":
            cmd += ["-movflags", "+faststart"]
        return cmd + [output]
    convert = variant.get("convert")
    if convert == "mp3":
        return base + ["-i", inputs[0], "-vn", "-c:a", "libmp3lame", "-q:a", "2", output]
    if convert == "opus":
        return base + ["-i", inputs[0], "-vn", "-c:a", "copy", output]
    # YouTube's M4A/MP4 streams are fragmented DASH files; a stream copy makes them regular files.
    return base + ["-i", inputs[0], "-c", "copy", output]
