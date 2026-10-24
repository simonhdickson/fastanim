"""The same epicycles in fastanim's Python API: same coefficients, same number of vectors and
circles, still a Python closure per shape per frame. The difference is that every shape is a
function of `t`, so the trace is a slice of the finished curve rather than a path that grows
each frame, and frames are drawn on the GPU.

    python -m fastanim render fastanim_epicycles.py -o epicycles.mp4
"""

from functools import lru_cache

from fastanim import *

from fourier import COEFFS, DURATION, tips

STROKE = 0.01
# Points in the finished trace.
TRACE = 1024

# Every closure asks for the tips at its frame's `t`; work them out once per frame. Export
# evaluates frames on several threads at once, so keep one entry per frame in flight.
at = lru_cache(maxsize=64)(tips)
# The whole drawing, for the trace to show the first part of.
CURVE = [tips(DURATION * k / TRACE)[-1] for k in range(TRACE + 1)]


def p(z: complex) -> Point:
    return point(z.real, z.imag)


@scene
def epicycles(s: Scene) -> None:
    for k, (_, c) in enumerate(COEFFS):
        start = lambda t, k=k: at(t)[k - 1] if k else 0j

        ring = s.add(circle(abs(c)))
        s.always(ring, lambda st, t, start=start, r=abs(c): circle(r)
                 .move_to(p(start(t))).stroke(BLUE, STROKE).stroke_opacity(0.5))
        v = s.add(line((0, 0), (0, 0)))
        s.always(v, lambda st, t, k=k, start=start: line(p(start(t)), p(at(t)[k]))
                 .stroke(WHITE, STROKE))

    trace = s.add(line((0, 0), (0, 0)))

    @s.always(trace)
    def draw(st: State, t: float) -> Shape:
        done = CURVE[: int(TRACE * min(t / DURATION, 1.0)) + 1]
        return polyline([p(z) for z in done + [at(t)[-1]]]).stroke(YELLOW, 2 * STROKE)

    s.wait(DURATION)
