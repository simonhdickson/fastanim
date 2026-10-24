"""Animated scenes in Python: record a scene once, then preview, render or export it.

See the fastanim book for the API; every function is named as in Rust and Rhai.

The browser's build (Pyodide) has no renderer: it records, bakes and exports SVG only.
"""

from ._native import *  # noqa: F401,F403
from ._native import SceneError, SceneFn, TypesetError  # noqa: F401
