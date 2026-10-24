// The epicycles of fastanim_epicycles.py in Rhai: the same star, coefficients, vectors, circles
// and trace, with a Rhai closure per shape per frame. Rhai has no GIL, so export evaluates
// frames on every core.
//
//   fastanim render bench/epicycles/epicycles.rhai -o epicycles.mp4
//
// Rhai can't read the environment; bench.sh rewrites these two lines.
let n = 100;
let duration = 10.0;

let stroke = 0.01;
// Points in the finished trace.
let trace_points = 1024;

// Complex numbers as points: x real, y imaginary.
fn cmul(a, b) {
    point(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x)
}

fn cis(a) {
    point(a.cos(), a.sin())
}

// The star's corners, alternating outer and inner, as `fourier.star`.
let corners = [];
for i in 0..10 {
    let r = if i % 2 == 0 { 3.0 } else { 1.2 };
    corners.push(cis(PI / 2.0 + PI * i.to_float() / 5.0) * r);
}

// Exact coefficients of the star drawn at constant speed, as `fourier.coefficients`: when each
// corner is reached, as a fraction of the period, and the velocity after it.
let at = [0.0];
for j in 0..10 {
    let e = corners[(j + 1) % 10] - corners[j];
    at.push(at[j] + (e.x * e.x + e.y * e.y).sqrt());
}
let perimeter = at[10];
at = at.map(|a| a / perimeter);
let v = [];
for j in 0..10 {
    v.push((corners[(j + 1) % 10] - corners[j]) / (at[j + 1] - at[j]));
}

fn coefficient(corners, at, v, f) {
    let c = point(0.0, 0.0);
    if f == 0 {
        for j in 0..10 {
            c = c + (corners[j] + corners[(j + 1) % 10]) * ((at[j + 1] - at[j]) / 2.0);
        }
        return c;
    }
    let w = TAU * f.to_float();
    for j in 0..10 {
        c = c + cmul(v[j] - v[(j + 9) % 10], cis(-w * at[j]));
    }
    c * (-1.0 / (w * w))
}

// Frequencies 0, 1, -1, 2, -2, ..., `n` of them.
let freqs = [0];
for k in 1..n {
    freqs.push(if k % 2 == 1 { (k + 1) / 2 } else { -k / 2 });
}
let coeffs = freqs.map(|f| coefficient(corners, at, v, f));

// Vector `k` at time `t`.
fn spin(c, f, t, duration) {
    cmul(c, cis(TAU * f.to_float() * min(t / duration, 1.0)))
}

// The whole drawing, for the trace to show the first part of: an inverse FFT of the
// coefficients onto `m` points, every (m / trace_points)th of them kept. `m` is a power of two
// above `n`, so no two frequencies land on the same point.
fn fft(a) {
    let m = a.len();
    let j = 0;
    for i in 1..m {
        let bit = m >> 1;
        while (j & bit) != 0 {
            j = j ^ bit;
            bit = bit >> 1;
        }
        j = j ^ bit;
        if i < j {
            let x = a[i];
            a[i] = a[j];
            a[j] = x;
        }
    }
    let len = 2;
    while len <= m {
        let step = cis(TAU / len.to_float());
        let half = len / 2;
        for i in range(0, m, len) {
            let w = point(1.0, 0.0);
            for k in 0..half {
                let u = a[i + k];
                let x = cmul(a[i + k + half], w);
                a[i + k] = u + x;
                a[i + k + half] = u - x;
                w = cmul(w, step);
            }
        }
        len = len * 2;
    }
    a
}
let m = trace_points;
while m <= n {
    m = m * 2;
}
let spectrum = [];
spectrum.pad(m, point(0.0, 0.0));
for k in 0..n {
    spectrum[(freqs[k] + m) % m] = coeffs[k];
}
let all = fft(spectrum);
let curve = [];
for i in 0..=trace_points {
    curve.push(all[(i * (m / trace_points)) % m]);
}

// Each circle sits on the tip of the vector before it, read from the state of the circle before
// that: one term per updater rather than a sum over all vectors before it.
let rings = [];
for k in 0..n {
    let c = coeffs[k];
    let f = freqs[k];
    let r = (c.x * c.x + c.y * c.y).sqrt();
    let ring = scene.add(circle(r));
    if k == 0 {
        scene.always(ring, |s, t| circle(r).stroke(BLUE, stroke).stroke_opacity(0.5));
    } else {
        let prev = rings[k - 1];
        let pc = coeffs[k - 1];
        let pf = freqs[k - 1];
        scene.always(ring, |s, t| {
            circle(r)
                .move_to(s[prev].center() + spin(pc, pf, t, duration))
                .stroke(BLUE, stroke)
                .stroke_opacity(0.5)
        });
    }
    rings.push(ring);
    let vector = scene.add(line(ORIGIN, ORIGIN));
    scene.always(vector, |s, t| {
        let a = s[ring].center();
        line(a, a + spin(c, f, t, duration)).stroke(WHITE, stroke)
    });
}

let last = rings[n - 1];
let lc = coeffs[n - 1];
let lf = freqs[n - 1];
let trace = scene.add(line(ORIGIN, ORIGIN));
scene.always(trace, |s, t| {
    let done = curve.extract(0, (trace_points.to_float() * min(t / duration, 1.0)).to_int() + 1);
    done.push(s[last].center() + spin(lc, lf, t, duration));
    polyline(done).stroke(YELLOW, 2.0 * stroke)
});

scene.wait(duration);
