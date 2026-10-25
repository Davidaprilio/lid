"""Optional checksum of a download: read what the user pastes, hash the finished file."""

from __future__ import annotations

import hashlib
import os
import re
import threading
import time
from typing import Callable

ALGORITHMS = {"md5": 32, "sha1": 40, "sha256": 64, "sha512": 128}  # name -> hex length
TITLES = {"md5": "MD5", "sha1": "SHA-1", "sha256": "SHA-256", "sha512": "SHA-512"}
BY_LENGTH = {n: a for a, n in ALGORITHMS.items()}
_PREFIX = re.compile(r"(md5|sha-?1|sha-?256|sha-?512)\s*[:=]\s*", re.I)
_HEX = re.compile(r"[0-9a-f]+")


class ChecksumError(ValueError):
    """The text is not a checksum; the message says why, for the field's error line."""


def parse(text: str, algorithm: str = "auto") -> str:
    """``"<algorithm>:<lower-case hex>"`` for ``text``, ``""`` for an empty field.

    Accepts plain hex, a ``sha256:<hex>`` style prefix, and a line of a SHA256SUMS file
    (``<hex>  name`` or ``<hex> *name``, the name is dropped). ``algorithm`` "auto" picks it from
    the prefix or the length; any other value forces it and the length has to match.
    """
    s = text.strip()
    if not s:
        return ""
    named = None
    m = _PREFIX.match(s)
    if m:
        named = m.group(1).lower().replace("-", "")
        s = s[m.end():].strip()
    value = s.split()[0].lower() if s else ""
    if not value:
        raise ChecksumError("Not a valid checksum — there is no value after the prefix.")
    if not _HEX.fullmatch(value):
        bad = next(ch for ch in value if ch not in "0123456789abcdef")
        raise ChecksumError(f"Not a valid checksum — “{bad}” is not a hex character.")
    if named and algorithm not in ("auto", named):
        raise ChecksumError(f"The value is marked {TITLES[named]} but {TITLES[algorithm]} is picked.")
    algo = named or (algorithm if algorithm != "auto" else BY_LENGTH.get(len(value)))
    if algo is None:
        raise ChecksumError(f"Not a valid checksum — expected 32, 40, 64 or 128 hex characters, got {len(value)}.")
    if len(value) != ALGORITHMS[algo]:
        raise ChecksumError(f"Not a valid {TITLES[algo]} value — expected {ALGORITHMS[algo]} hex characters, "
                            f"got {len(value)}.")
    return f"{algo}:{value}"


def split(checksum: str) -> tuple[str, str]:
    """``("sha256", "<hex>")`` from a stored ``"sha256:<hex>"``."""
    algo, _, value = checksum.partition(":")
    return algo, value


def title(checksum: str) -> str:
    """"SHA-256" for a stored checksum, "" for none."""
    return TITLES.get(split(checksum)[0], "") if checksum else ""


class Verifier(threading.Thread):
    """Hashes a finished file in 1 MiB reads. The results stay on the verifier; the manager copies
    them to the download on the UI thread."""

    CHUNK = 1024 * 1024

    def __init__(self, path: str, checksum: str, manual: bool = False,
                 on_finish: Callable[["Verifier"], None] | None = None, download_id: int = 0):
        super().__init__(daemon=True, name=f"verify-{download_id}")
        self.path = path
        self.algorithm, self.expected = split(checksum)
        self.manual = manual  # "Verify now" in Properties: no Complete dialog afterwards
        self.download_id = download_id
        self.on_finish = on_finish
        self.read = 0
        self.total = 0
        self.result = ""  # verified | mismatch | computed (nothing to compare with) | skipped | error
        self.actual = ""
        self.error = ""
        self.seconds = 0.0
        self.running = True
        self._skip = threading.Event()

    @property
    def progress(self) -> float:
        return min(1.0, self.read / self.total) if self.total > 0 else 0.0

    def skip(self) -> None:
        self._skip.set()

    def run(self) -> None:
        started = time.monotonic()
        try:
            h = hashlib.new(self.algorithm)
            self.total = os.path.getsize(self.path)
            with open(self.path, "rb") as f:
                while not self._skip.is_set():
                    block = f.read(self.CHUNK)
                    if not block:
                        break
                    h.update(block)
                    self.read += len(block)
            if self._skip.is_set():
                self.result = "skipped"
            else:
                self.actual = h.hexdigest()
                if not self.expected:
                    self.result = "computed"
                else:
                    self.result = "verified" if self.actual == self.expected else "mismatch"
        except (OSError, ValueError) as e:
            self.result, self.error = "error", str(e)
        self.seconds = time.monotonic() - started
        self.running = False
        if self.on_finish:
            self.on_finish(self)
