//! sRGB colors that interpolate in Oklab (SPEC §4.3).

use crate::Interpolate;

/// An sRGB color with straight alpha, all channels in `0..=1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    /// Red.
    pub r: f32,
    /// Green.
    pub g: f32,
    /// Blue.
    pub b: f32,
    /// Alpha (opacity).
    pub a: f32,
}

impl Color {
    /// Fully transparent black.
    pub const TRANSPARENT: Color = Color::rgba(0.0, 0.0, 0.0, 0.0);

    /// An opaque color from channels in `0..=1`.
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// A color from channels in `0..=1`.
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// An opaque color from `0xRRGGBB`.
    pub const fn hex(rgb: u32) -> Self {
        let [_, r, g, b] = rgb.to_be_bytes();
        Self::rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
    }

    /// The same color with alpha `a`.
    pub const fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    /// `#rrggbb`, ignoring alpha.
    pub fn to_hex(self) -> String {
        let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("#{:02x}{:02x}{:02x}", c(self.r), c(self.g), c(self.b))
    }

    fn to_oklab(self) -> [f32; 3] {
        let lin = |c: f32| {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let (r, g, b) = (lin(self.r), lin(self.g), lin(self.b));
        let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
        let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
        let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
        [
            0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
            1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
            0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
        ]
    }

    fn from_oklab([ok_l, ok_a, ok_b]: [f32; 3], a: f32) -> Self {
        let l = (ok_l + 0.396_337_78 * ok_a + 0.215_803_76 * ok_b).powi(3);
        let m = (ok_l - 0.105_561_346 * ok_a - 0.063_854_17 * ok_b).powi(3);
        let s = (ok_l - 0.089_484_18 * ok_a - 1.291_485_5 * ok_b).powi(3);
        let enc = |c: f32| {
            let c = c.clamp(0.0, 1.0);
            if c <= 0.003_130_8 {
                12.92 * c
            } else {
                1.055 * c.powf(1.0 / 2.4) - 0.055
            }
        };
        Self {
            r: enc(4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s),
            g: enc(-1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s),
            b: enc(-0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s),
            a,
        }
    }
}

impl Interpolate for Color {
    fn lerp(a: &Self, b: &Self, t: f32) -> Self {
        // Exact endpoints: the Oklab round trip is not bit-exact.
        if t <= 0.0 {
            return *a;
        }
        if t >= 1.0 {
            return *b;
        }
        let (la, lb) = (a.to_oklab(), b.to_oklab());
        let mix = std::array::from_fn(|i| la[i] + (lb[i] - la[i]) * t);
        Color::from_oklab(mix, a.a + (b.a - a.a) * t)
    }
}

/// White.
pub const WHITE: Color = Color::rgb(1.0, 1.0, 1.0);
/// Black.
pub const BLACK: Color = Color::rgb(0.0, 0.0, 0.0);
/// manim's `BLUE`.
pub const BLUE: Color = Color::hex(0x58c4dd);
/// manim's `RED`.
pub const RED: Color = Color::hex(0xfc6255);
/// manim's `GREEN`.
pub const GREEN: Color = Color::hex(0x83c167);
/// manim's `YELLOW`.
pub const YELLOW: Color = Color::hex(0xffff00);
