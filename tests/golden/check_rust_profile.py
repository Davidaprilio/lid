"""Read a profile written by the Rust port with the 1.2 code and compare it with expected.json.

    LIDER_GOLDEN_OUT=/tmp/x cargo test -p lider-core --test golden
    python3 tests/golden/check_rust_profile.py /tmp/x

This is the downgrade promise: a user can go back to 1.2 after 2.x has saved their data.
"""

from __future__ import annotations

import json
import os
import sys
from dataclasses import asdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(sys.argv[1]).resolve()
os.environ["HOME"] = "/home/lid-user"
os.environ["XDG_CONFIG_HOME"] = str(OUT / "config")
os.environ["XDG_DATA_HOME"] = str(OUT / "data")
os.environ["XDG_CACHE_HOME"] = "/home/lid-user/.cache"
sys.path.insert(0, str(ROOT))

from lider.core import settings as settings_mod  # noqa: E402
from lider.core.store import Store  # noqa: E402

expected = json.loads((Path(__file__).parent / "v1.2" / "expected.json").read_text(encoding="utf-8"))
store = Store()
got = {
    "settings": asdict(settings_mod.load()),
    "settings_legacy": asdict(settings_mod.load(OUT / "config" / "settings-legacy.json")),
    "downloads": [d.to_dict() for d in store.load_downloads()],
    "queues": store.load_queues(),
    "next_id": store.next_id(),
}
failed = [k for k, v in got.items() if v != expected[k]]
for k in failed:
    print(f"differs: {k}")
    if isinstance(got[k], list):
        for a, b in zip(got[k], expected[k]):
            if a != b:
                print("  got     ", a)
                print("  expected", b)
                break
sys.exit(1 if failed else 0)
