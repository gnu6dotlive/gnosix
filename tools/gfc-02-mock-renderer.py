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
if request.get("choices"):
    selected = [(item["id"], item["selected"]) for item in request["choices"]]
    if title == "choice_switch":
        selected = [(key, "utf8" if key == "encoding" else "true") for key, _ in selected]
    if title == "choice_invalid_value":
        selected[0] = (selected[0][0], "not-offered")
    if title == "choice_wrong_id":
        selected[0] = ("forged-id", selected[0][1])
    result = {
        "outcome": "selected_with_metadata",
        "paths": {
            "paths": [str(fixture / "alpha.txt")],
            "filter_index": request.get("selected_filter"),
            "choices": selected
        }
    }
elif request.get("filters"):
    selection = fixture / ("alpha.txt" if title == "filter_text" else "pixel.png")
    chosen = request.get("selected_filter") or 0
    if title == "filter_bad_index":
        chosen = 100
    result = {
        "outcome": "selected_with_filter",
        "paths": {"paths": [str(selection)], "filter_index": chosen}
    }
elif title == "cancel":
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
