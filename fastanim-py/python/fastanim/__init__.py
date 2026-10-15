"""Animated scenes in Python: record a scene once, then preview, render or export it.

See the fastanim book for the API; every function is named as in Rust and Rhai.

`pip install fastanim` records, bakes and exports SVG. `pip install fastanim[preview]` adds
`fastanim-preview`, the same native module built with rendering included, which is used in its
place when installed: video, PNG and the preview window.
"""

try:
    from fastanim_preview._native import *  # noqa: F401,F403
    from fastanim_preview._native import SceneError, SceneFn, TypesetError  # noqa: F401
except ImportError:
    from ._native import *  # noqa: F401,F403
    from ._native import SceneError, SceneFn, TypesetError  # noqa: F401
