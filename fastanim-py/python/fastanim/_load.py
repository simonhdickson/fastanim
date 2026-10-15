"""Finding a scene in a file, for `python -m fastanim` and preview's reload on save."""

from __future__ import annotations

import os
import runpy
import sys
from typing import Any, Callable

# The `__name__` scene files run under, so `if __name__ == "__main__":` blocks don't run.
RUN_NAME = "__fastanim_scene__"


def scenes(ns: dict[str, Any]) -> dict[str, Callable[..., Any]]:
    """The `@scene` functions defined in a file's namespace, by name; imported ones don't count."""
    from . import SceneFn

    return {
        name: v
        for name, v in ns.items()
        if isinstance(v, SceneFn) and getattr(v.__wrapped__, "__module__", None) == RUN_NAME
    }


def load(path: str | os.PathLike[str], name: str | None = None) -> Callable[..., Any]:
    """Runs the file at `path` and returns its scene called `name`. Without a name, the file's
    one `@scene` function, or else its `construct(scene)` function."""
    path = os.fspath(path)
    # As `python file.py` would: the file's own directory comes first, so it can import its
    # neighbours.
    folder = os.path.dirname(os.path.abspath(path))
    if folder not in sys.path:
        sys.path.insert(0, folder)
    ns = runpy.run_path(path, run_name=RUN_NAME)
    found = scenes(ns)
    if name is not None:
        from . import SceneFn

        f = ns.get(name)
        if not (isinstance(f, SceneFn) or callable(f)):
            raise LookupError(f"{path}: no scene `{name}`{_listing(found)}")
        return f
    if len(found) == 1:
        return next(iter(found.values()))
    if not found and callable(ns.get("construct")):
        return ns["construct"]
    if not found:
        raise LookupError(f"{path}: no `@scene` function or `construct(scene)`")
    raise LookupError(f"{path}: pick a scene with {path}:NAME{_listing(found)}")


def _listing(found: dict[str, Any]) -> str:
    return f" (scenes: {', '.join(found)})" if found else ""
