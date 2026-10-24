"""The maths both epicycle scenes share, in plain Python so neither gets a head start: a star
outline, its Fourier coefficients and where each vector's tip is at time `t`.

`N` (the number of vectors) and `DURATION` (seconds per revolution) come from the environment,
so `bench.sh` can sweep them without editing either scene."""

import cmath
import os
from math import cos, pi, sin

N = int(os.environ.get("N", "5000"))
DURATION = float(os.environ.get("DURATION", "10"))


def star(points: int = 5, outer: float = 3.0, inner: float = 1.2) -> list[complex]:
    """The corners of a star, alternating outer and inner; its corners need many vectors."""
    return [
        (outer if i % 2 == 0 else inner) * cmath.exp(1j * (pi / 2 + pi * i / points))
        for i in range(2 * points)
    ]


def coefficients(corners: list[complex], n: int) -> list[tuple[int, complex]]:
    """(frequency, coefficient) for frequencies 0, 1, -1, 2, -2, ..., `n` of them, of the closed
    polygon through `corners` drawn at constant speed over one period.

    Exact rather than a DFT of samples: integrating by parts twice leaves one term per corner,
    so it costs O(corners) per coefficient and never aliases however large `n` is."""
    m = len(corners)
    edges = [abs(corners[(j + 1) % m] - corners[j]) for j in range(m)]
    # When each corner is reached, as a fraction of the period, and the velocity after it.
    at = [sum(edges[:j]) / sum(edges) for j in range(m + 1)]
    v = [(corners[(j + 1) % m] - corners[j]) / (at[j + 1] - at[j]) for j in range(m)]

    def c(f: int) -> complex:
        if f == 0:
            return sum((corners[j] + corners[(j + 1) % m]) / 2 * (at[j + 1] - at[j]) for j in range(m))
        w = 2 * pi * f
        return -sum((v[j] - v[j - 1]) * cmath.exp(-1j * w * at[j]) for j in range(m)) / w**2

    freqs = [0] + [f for k in range(1, n) for f in (k, -k)][: n - 1]
    return [(f, c(f)) for f in freqs]


COEFFS = coefficients(star(), N)


def phase(t: float) -> float:
    """How far round the drawing is, from 0 to 1."""
    return min(t / DURATION, 1.0)


def tips(t: float) -> list[complex]:
    """Where each vector ends at time `t`; vector `k` starts where vector `k - 1` ends."""
    a, z, out = 2 * pi * phase(t), 0j, []
    for f, c in COEFFS:
        z += c * complex(cos(f * a), sin(f * a))
        out.append(z)
    return out
