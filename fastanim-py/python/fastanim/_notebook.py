"""Showing scenes inline in Jupyter: `Timeline._repr_html_` and `SceneFn._repr_html_` call `html`."""

from __future__ import annotations

import base64
import os
import sys
import tempfile
from typing import Any


def html(tl: Any, quality: str = "720p30") -> str:
    """The scene as an HTML `<video>` with the MP4 inline, sounds included."""
    # shortcut: the browser's build (Pyodide, JupyterLite) can't render video, so it shows the
    # first frame; a scrubber over `tl.frame(t)` like fastanim-web's player would play it.
    if sys.platform == "emscripten":
        return tl.svg(0.0)
    with tempfile.TemporaryDirectory() as d:
        path = os.path.join(d, "scene.mp4")
        tl.render(path, quality=quality)
        with open(path, "rb") as f:
            data = base64.b64encode(f.read()).decode()
    return f'<video controls loop playsinline src="data:video/mp4;base64,{data}"></video>'
