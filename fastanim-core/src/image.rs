//! Raster images (ROADMAP item 2): pixels shared between frames, placed on a shape by an
//! affine transform so they follow every move, scale and rotate the shape does.

use std::fmt;
use std::path::Path;
use std::sync::Arc;

use kurbo::Affine;

use crate::FRAME_HEIGHT;

/// Straight-alpha RGBA8 pixels, row by row from the top-left. Cloning shares the pixels.
#[derive(Clone)]
pub struct Image {
    width: u32,
    height: u32,
    pixels: Arc<Vec<u8>>,
}

impl Image {
    /// An image from `width * height * 4` bytes of straight-alpha RGBA, top row first.
    pub fn from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, String> {
        if width == 0 || height == 0 {
            return Err(format!("an image can't be {width}x{height} pixels"));
        }
        let want = width as usize * height as usize * 4;
        if pixels.len() != want {
            return Err(format!(
                "a {width}x{height} RGBA image is {want} bytes, got {}",
                pixels.len()
            ));
        }
        Ok(Self {
            width,
            height,
            pixels: Arc::new(pixels),
        })
    }

    /// Decodes a PNG or JPEG file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::decode(&bytes).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Decodes PNG or JPEG bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let img = ::image::load_from_memory(bytes).map_err(|e| e.to_string())?;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        Self::from_rgba(w, h, rgba.into_raw())
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The RGBA bytes.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// The RGBA bytes, shared rather than copied.
    pub fn shared_pixels(&self) -> Arc<Vec<u8>> {
        self.pixels.clone()
    }

    /// Identifies the pixels while any clone of this image is alive, for renderer caches.
    pub fn key(&self) -> usize {
        Arc::as_ptr(&self.pixels) as usize
    }

    /// The image as a PNG file.
    pub fn to_png(&self) -> Vec<u8> {
        use ::image::ImageEncoder;
        let mut out = Vec::new();
        ::image::codecs::png::PngEncoder::new(&mut out)
            .write_image(
                &self.pixels,
                self.width,
                self.height,
                ::image::ExtendedColorType::Rgba8,
            )
            .expect("encoding a valid RGBA image to memory can't fail");
        out
    }

    /// manim's default size: an image as many pixels tall as a 1080p frame fills its height.
    pub fn default_height(&self) -> f64 {
        f64::from(self.height) / 1080.0 * FRAME_HEIGHT
    }
}

impl PartialEq for Image {
    fn eq(&self, o: &Self) -> bool {
        Arc::ptr_eq(&self.pixels, &o.pixels)
            || (self.width == o.width && self.height == o.height && self.pixels == o.pixels)
    }
}

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Image({}x{})", self.width, self.height)
    }
}

/// An image drawn on a shape, under its fill and stroke.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageFill {
    /// The pixels.
    pub image: Image,
    /// Maps the unit square, `(0, 0)` the image's top-left corner and `(1, 1)` its
    /// bottom-right, into scene units.
    pub transform: Affine,
    /// Linear (smooth) or nearest-neighbour (pixelated) resampling.
    pub smooth: bool,
}

impl ImageFill {
    /// The opacity to draw the image at, given the shape's: `create` and `uncreate` fade it
    /// with the fraction of the outline drawn.
    pub fn alpha(opacity: f32, draw_range: &std::ops::Range<f32>) -> f32 {
        opacity * (draw_range.end - draw_range.start).clamp(0.0, 1.0)
    }
}

/// Blends two placements coefficient by coefficient, as manim interpolates points.
pub(crate) fn lerp_affine(a: Affine, b: Affine, t: f64) -> Affine {
    let (x, y) = (a.as_coeffs(), b.as_coeffs());
    Affine::new(std::array::from_fn(|i| x[i] + (y[i] - x[i]) * t))
}
