"""`python -m fastanim preview|render|still scene.py[:name]`: the `fastanim` CLI for Python
scenes, with the same options."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from . import SceneFn, scene
from ._load import RUN_NAME, load

QUALITIES = ["480p15", "720p30", "1080p60", "4k60"]
FORMATS = ["mp4", "webm", "gif", "png", "svg", "animated-svg"]


def seconds(v: str) -> float:
    try:
        return float(v.removesuffix("s"))
    except ValueError:
        raise argparse.ArgumentTypeError(f"bad time `{v}`") from None


def parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="python -m fastanim", description="Preview or export a fastanim scene written in Python"
    )
    sub = p.add_subparsers(dest="command", required=True, metavar="COMMAND")
    target = "the scene file, then `:NAME` if it has more than one scene"

    pre = sub.add_parser("preview", help="Open the scene in a window with a scrubber, reloading it on save")
    pre.add_argument("scene", metavar="FILE[:NAME]", help=target)

    ren = sub.add_parser("render", help="Export the scene as video or frames")
    ren.add_argument("scene", metavar="FILE[:NAME]", help=target)
    ren.add_argument("-q", "--quality", choices=QUALITIES, default="1080p60", metavar="PRESET",
                     help="480p15, 720p30, 1080p60 (default) or 4k60")
    ren.add_argument("-f", "--format", choices=FORMATS, metavar="FORMAT",
                     help="mp4, webm, gif, png or svg (folders of frames), or animated-svg; "
                     "default: from --output's extension, else mp4")
    ren.add_argument("-o", "--output", type=Path, metavar="PATH",
                     help="output file or frames folder (default: named after the scene, e.g. NAME.mp4)")
    ren.add_argument("--section", metavar="MARKER", help="only render from MARKER to the next marker")

    still = sub.add_parser("still", help="Export one frame of the scene")
    still.add_argument("scene", metavar="FILE[:NAME]", help=target)
    still.add_argument("-q", "--quality", choices=QUALITIES, default="1080p60", metavar="PRESET",
                       help="480p15, 720p30, 1080p60 (default) or 4k60")
    still.add_argument("-f", "--format", choices=["png", "svg"], metavar="FORMAT",
                       help="png or svg; default: from --output's extension, else png")
    still.add_argument("-o", "--output", type=Path, metavar="PATH",
                       help="output file (default: named after the scene, e.g. NAME.png)")
    when = still.add_mutually_exclusive_group()
    when.add_argument("--at", type=seconds, default=0.0, metavar="SECS", help="time to render, e.g. 3.5 or 3.5s")
    when.add_argument("--frame", type=int, metavar="N", help="frame index to render")
    return p


def split(target: str) -> tuple[str, str | None]:
    """`file.py:name` as the file and the name. A colon that is part of the path, as in
    `C:\\scenes\\a.py`, is left alone."""
    path, sep, name = target.rpartition(":")
    if sep and name.isidentifier() and path:
        return path, name
    return target, None


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    path, name = split(args.scene)
    try:
        f = load(path, name)
        s = f if isinstance(f, SceneFn) else scene(f)
        if args.command == "preview":
            s.preview()
        elif args.command == "render":
            s.render(args.output, format=args.format, quality=args.quality, section=args.section)
        else:
            s.still(args.output, format=args.format, at=args.at, frame=args.frame, quality=args.quality)
    except (OSError, LookupError, RuntimeError, ValueError) as e:
        # Errors raised by the scene itself keep their traceback; the CLI's own are one line.
        if _raised_in_scene(e):
            raise
        print(f"error: {e}", file=sys.stderr)
        return 1
    return 0


def _raised_in_scene(e: BaseException) -> bool:
    tb = e.__traceback__
    while tb is not None:
        if tb.tb_frame.f_globals.get("__name__") == RUN_NAME:
            return True
        tb = tb.tb_next
    return False


if __name__ == "__main__":
    sys.exit(main())
