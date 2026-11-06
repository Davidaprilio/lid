"""Read two profiles with the 1.2 code and check that they hold the same data.

    python3 tests/golden/compare_profiles.py IN OUT

IN is a copy of a 1.x profile and OUT what 2.x wrote from it (see `real_profile_round_trip` in
crates/lider-core/tests/golden.rs); each holds config/lider/settings.json and data/lider/downloads.db.
Prints only counts and the names of fields that differ, never the data.
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

READ = r"""
import json, sys
from dataclasses import asdict
from lider.core import settings as settings_mod
from lider.core.store import Store
store = Store()
print(json.dumps({
    "settings": asdict(settings_mod.load()),
    "downloads": [d.to_dict() for d in store.load_downloads()],
    "queues": store.load_queues(),
    "next_id": store.next_id(),
}, sort_keys=True, default=str))
"""


def read(profile: Path) -> dict:
    env = dict(os.environ, XDG_CONFIG_HOME=str(profile / "config"), XDG_DATA_HOME=str(profile / "data"),
               PYTHONPATH=str(ROOT))
    out = subprocess.run([sys.executable, "-c", READ], env=env, check=True, capture_output=True, text=True)
    import json
    return json.loads(out.stdout)


a, b = (read(Path(p).resolve()) for p in sys.argv[1:3])
print(f"downloads {len(a['downloads'])} / {len(b['downloads'])}, queues {len(a['queues'])} / {len(b['queues'])}")
diff = [k for k in a if a[k] != b[k]]
for k in diff:
    if k == "settings":
        print("settings differ in:", sorted(f for f in a[k] if a[k][f] != b[k].get(f)))
    elif k == "downloads":
        for x, y in zip(a[k], b[k]):
            if x != y:
                print(f"download {x.get('id')} differs in:", sorted(f for f in x if x[f] != y.get(f)))
    else:
        print(f"{k} differs")
print("SAME" if not diff else "DIFFERENT")
sys.exit(1 if diff else 0)
