#!/usr/bin/env python3
"""Isolated protocol test ONLY. Never install as a production renderer."""
import json
import os
import sys
import time
from pathlib import Path

request = json.load(sys.stdin)
title = request["title"]
fixture = Path(os.environ["GFC02_FIXTURE"])
if title == "slow":
    time.sleep(12)
if title == "cancel":
    result = {"outcome": "cancelled"}
elif title == "relative":
    result = {"outcome": "selected", "paths": ["relative-path.txt"]}
elif title == "save_one":
    result = {"outcome": "selected", "paths": [str(fixture / "new-cabinet-file.txt")]}
elif title in ("save_many", "save_collision"):
    result = {"outcome": "selected", "paths": [str(fixture)]}
else:
    target = fixture / ("alpha.txt" if title == "alpha" else "beta.txt")
    result = {"outcome": "selected", "paths": [str(target)]}
print(json.dumps(result), flush=True)
