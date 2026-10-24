"""Fourier epicycles drawing a star, written the way manim scenes usually are (after
3Blue1Brown's `FourierCirclesScene`): a clock that updaters advance by `dt`, each vector
chained to the end of the one before it, circles following the vectors and a `TracedPath`
on the last tip.

    manim -qh --disable_caching manim_epicycles.py Epicycles
"""

from manim import *

from fourier import COEFFS, DURATION, phase

STROKE = 2


class Epicycles(Scene):
    def construct(self):
        clock = ValueTracker(0)
        clock.add_updater(lambda m, dt: m.increment_value(dt))
        self.add(clock)

        vectors, circles = VGroup(), VGroup()
        last = None
        for f, c in COEFFS:
            v = Line(ORIGIN, RIGHT, stroke_width=STROKE)

            def follow(v, f=f, c=c, last=last):
                start = last.get_end() if last else ORIGIN
                z = c * np.exp(1j * f * TAU * phase(clock.get_value()))
                v.put_start_and_end_on(start, start + np.array([z.real, z.imag, 0]) + 1e-9)

            v.add_updater(follow)
            follow(v)
            vectors.add(v)

            circle = Circle(radius=abs(c), color=BLUE, stroke_width=STROKE, stroke_opacity=0.5)
            circle.add_updater(lambda m, v=v: m.move_to(v.get_start()))
            circles.add(circle)
            last = v

        trace = TracedPath(last.get_end, stroke_color=YELLOW, stroke_width=2 * STROKE)
        # Vectors first, so circles update to where the vectors are this frame.
        self.add(vectors, circles, trace)
        self.wait(DURATION)
