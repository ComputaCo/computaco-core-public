//! Pixel measurements between two crops of the screen. None of them is "are the bytes
//! equal": fonts, antialiasing and a theme's exact shade differ between a real toolkit
//! and a reimplementation, and a comparison has to say *how* two crops differ.
//!
//! - `dhash` / `hamming`: a 64-bit gradient hash for triage and settle detection.
//! - `delta_e`: CIE76 colour difference in L*a*b*, per pixel, summarised.
//! - `ssim`: structural similarity over 8×8 windows with integral images.
//! - `sobel_edges` + `chamfer`: shape agreement as the symmetric mean distance between
//!   edge maps, through a two-pass chamfer distance transform.
//! - `block_diff`: where two whole frames differ, without any semantics.
use crate::schema::Rect;
use serde::{Deserialize, Serialize};

/// Straight RGBA, row-major, no padding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
impl Image {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        assert_eq!(rgba.len(), (width * height * 4) as usize, "rgba length");
        Self {
            width,
            height,
            rgba,
        }
    }
    pub fn blank(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgba: vec![0; (width * height * 4) as usize],
        }
    }
    pub fn rect(&self) -> Rect {
        Rect::new(0, 0, self.width as i32, self.height as i32)
    }
    #[inline]
    pub fn px(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }
    /// The part of the image inside `r`, clipped to the image; `None` if nothing is.
    pub fn crop(&self, r: &Rect) -> Option<Image> {
        let r = r.intersection(&self.rect())?;
        let mut out = Vec::with_capacity((r.w * r.h * 4) as usize);
        for y in r.y..r.bottom() {
            let start = ((y as u32 * self.width + r.x as u32) * 4) as usize;
            out.extend_from_slice(&self.rgba[start..start + (r.w * 4) as usize]);
        }
        Some(Image::new(r.w as u32, r.h as u32, out))
    }
    /// Luma in 0..=255 as f32, Rec. 601.
    pub fn gray(&self) -> Gray {
        let mut data = Vec::with_capacity((self.width * self.height) as usize);
        for p in self.rgba.as_chunks::<4>().0 {
            data.push(0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32);
        }
        Gray {
            width: self.width,
            height: self.height,
            data,
        }
    }
    /// Bilinear resample to `w`×`h`.
    pub fn resize(&self, w: u32, h: u32) -> Image {
        if w == self.width && h == self.height {
            return self.clone();
        }
        let mut out = vec![0u8; (w * h * 4) as usize];
        if w == 0 || h == 0 || self.width == 0 || self.height == 0 {
            return Image {
                width: w,
                height: h,
                rgba: out,
            };
        }
        let sx = self.width as f32 / w as f32;
        let sy = self.height as f32 / h as f32;
        for y in 0..h {
            let fy = ((y as f32 + 0.5) * sy - 0.5).max(0.0);
            let y0 = fy.floor() as u32;
            let y1 = (y0 + 1).min(self.height - 1);
            let ty = fy - y0 as f32;
            for x in 0..w {
                let fx = ((x as f32 + 0.5) * sx - 0.5).max(0.0);
                let x0 = fx.floor() as u32;
                let x1 = (x0 + 1).min(self.width - 1);
                let tx = fx - x0 as f32;
                let (a, b, c, d) = (
                    self.px(x0, y0),
                    self.px(x1, y0),
                    self.px(x0, y1),
                    self.px(x1, y1),
                );
                let o = ((y * w + x) * 4) as usize;
                for ch in 0..4 {
                    let top = a[ch] as f32 * (1.0 - tx) + b[ch] as f32 * tx;
                    let bot = c[ch] as f32 * (1.0 - tx) + d[ch] as f32 * tx;
                    out[o + ch] = (top * (1.0 - ty) + bot * ty).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        Image {
            width: w,
            height: h,
            rgba: out,
        }
    }
    /// FNV hash of the pixels: equality, for settle detection.
    pub fn content_hash(&self) -> u64 {
        crate::hash::fnv_bytes(&self.rgba)
    }
    pub fn sha256(&self) -> String {
        crate::hash::sha256_hex(&self.rgba)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Gray {
    pub width: u32,
    pub height: u32,
    pub data: Vec<f32>,
}
impl Gray {
    #[inline]
    pub fn at(&self, x: u32, y: u32) -> f32 {
        self.data[(y * self.width + x) as usize]
    }
}

/// Difference hash: 8×9 downsample, each bit says whether a pixel is brighter than its
/// right neighbour. Robust to scale and small shifts; two hashes within a few bits are
/// the same picture.
pub fn dhash(img: &Image) -> u64 {
    let small = img.resize(9, 8).gray();
    let mut h = 0u64;
    for y in 0..8 {
        for x in 0..8 {
            h <<= 1;
            if small.at(x, y) > small.at(x + 1, y) {
                h |= 1;
            }
        }
    }
    h
}
pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

fn srgb_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
/// sRGB (D65) to CIE L*a*b*.
pub fn rgb_to_lab(r: u8, g: u8, b: u8) -> [f32; 3] {
    let (rl, gl, bl) = (srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b));
    let x = (0.4124564 * rl + 0.3575761 * gl + 0.1804375 * bl) / 0.95047;
    let y = 0.2126729 * rl + 0.7151522 * gl + 0.072175 * bl;
    let z = (0.0193339 * rl + 0.119192 * gl + 0.9503041 * bl) / 1.08883;
    fn f(t: f32) -> f32 {
        if t > 0.008856 {
            t.cbrt()
        } else {
            7.787 * t + 16.0 / 116.0
        }
    }
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DeltaE {
    pub mean: f32,
    pub p95: f32,
    pub max: f32,
    /// Fraction of pixels whose ΔE exceeds 2.3, the just-noticeable difference.
    pub noticeable: f32,
}
/// Per-pixel CIE76 ΔE between two images of the same size (the second is resampled if
/// not). Alpha is ignored.
pub fn delta_e(a: &Image, b: &Image) -> DeltaE {
    let b = same_size(a, b);
    let n = (a.width * a.height) as usize;
    if n == 0 {
        return DeltaE::default();
    }
    let mut ds = Vec::with_capacity(n);
    for (pa, pb) in a
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.rgba.as_chunks::<4>().0)
    {
        if pa[..3] == pb[..3] {
            ds.push(0.0);
            continue;
        }
        let la = rgb_to_lab(pa[0], pa[1], pa[2]);
        let lb = rgb_to_lab(pb[0], pb[1], pb[2]);
        ds.push(
            ((la[0] - lb[0]).powi(2) + (la[1] - lb[1]).powi(2) + (la[2] - lb[2]).powi(2)).sqrt(),
        );
    }
    let mean = ds.iter().sum::<f32>() / n as f32;
    let max = ds.iter().cloned().fold(0.0, f32::max);
    let noticeable = ds.iter().filter(|d| **d > 2.3).count() as f32 / n as f32;
    let mut sorted = ds;
    sorted.sort_by(|x, y| x.partial_cmp(y).unwrap());
    let p95 = sorted[((n - 1) as f32 * 0.95) as usize];
    DeltaE {
        mean,
        p95,
        max,
        noticeable,
    }
}

fn same_size(a: &Image, b: &Image) -> Image {
    if a.width == b.width && a.height == b.height {
        b.clone()
    } else {
        b.resize(a.width, a.height)
    }
}

struct Integral {
    w: usize,
    s: Vec<f64>,
}
impl Integral {
    fn new(g: &Gray, f: impl Fn(usize) -> f64) -> Self {
        let (w, h) = (g.width as usize, g.height as usize);
        let mut s = vec![0f64; (w + 1) * (h + 1)];
        for y in 0..h {
            let mut row = 0f64;
            for x in 0..w {
                row += f(y * w + x);
                s[(y + 1) * (w + 1) + x + 1] = s[y * (w + 1) + x + 1] + row;
            }
        }
        Self { w: w + 1, s }
    }
    #[inline]
    fn sum(&self, x0: usize, y0: usize, x1: usize, y1: usize) -> f64 {
        self.s[y1 * self.w + x1] - self.s[y0 * self.w + x1] - self.s[y1 * self.w + x0]
            + self.s[y0 * self.w + x0]
    }
}

/// Mean SSIM over all 8×8 windows (stride 4); 1 for identical images. Images smaller
/// than a window are compared as one window of their own size.
pub fn ssim(a: &Image, b: &Image) -> f32 {
    let b = same_size(a, b);
    let ga = a.gray();
    let gb = b.gray();
    let (w, h) = (ga.width as usize, ga.height as usize);
    if w == 0 || h == 0 {
        return 1.0;
    }
    let win = 8.min(w).min(h);
    let stride = (win / 2).max(1);
    let ia = Integral::new(&ga, |i| ga.data[i] as f64);
    let ib = Integral::new(&gb, |i| gb.data[i] as f64);
    let iaa = Integral::new(&ga, |i| (ga.data[i] as f64).powi(2));
    let ibb = Integral::new(&gb, |i| (gb.data[i] as f64).powi(2));
    let iab = Integral::new(&ga, |i| ga.data[i] as f64 * gb.data[i] as f64);
    let c1 = (0.01f64 * 255.0).powi(2);
    let c2 = (0.03f64 * 255.0).powi(2);
    let n = (win * win) as f64;
    let mut total = 0f64;
    let mut count = 0usize;
    let mut y = 0;
    loop {
        let mut x = 0;
        loop {
            let (x1, y1) = (x + win, y + win);
            let ma = ia.sum(x, y, x1, y1) / n;
            let mb = ib.sum(x, y, x1, y1) / n;
            let va = (iaa.sum(x, y, x1, y1) / n - ma * ma).max(0.0);
            let vb = (ibb.sum(x, y, x1, y1) / n - mb * mb).max(0.0);
            let cov = iab.sum(x, y, x1, y1) / n - ma * mb;
            let s = ((2.0 * ma * mb + c1) * (2.0 * cov + c2))
                / ((ma * ma + mb * mb + c1) * (va + vb + c2));
            total += s;
            count += 1;
            if x1 >= w {
                break;
            }
            x = (x + stride).min(w - win);
        }
        if y1_done(y, win, h) {
            break;
        }
        y = (y + stride).min(h - win);
    }
    (total / count as f64).clamp(-1.0, 1.0) as f32
}
#[inline]
fn y1_done(y: usize, win: usize, h: usize) -> bool {
    y + win >= h
}

/// Binary edge map by Sobel magnitude over `threshold` (0..=255 scale).
pub fn sobel_edges(g: &Gray, threshold: f32) -> Vec<bool> {
    let (w, h) = (g.width as usize, g.height as usize);
    let mut out = vec![false; w * h];
    if w < 3 || h < 3 {
        return out;
    }
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let p = |dx: isize, dy: isize| {
                g.data[((y as isize + dy) as usize) * w + (x as isize + dx) as usize]
            };
            let gx = -p(-1, -1) - 2.0 * p(-1, 0) - p(-1, 1) + p(1, -1) + 2.0 * p(1, 0) + p(1, 1);
            let gy = -p(-1, -1) - 2.0 * p(0, -1) - p(1, -1) + p(-1, 1) + 2.0 * p(0, 1) + p(1, 1);
            out[y * w + x] = (gx * gx + gy * gy).sqrt() / 4.0 > threshold;
        }
    }
    out
}

/// Chamfer (3-4) distance transform: distance from each pixel to the nearest edge
/// pixel, in pixels. `f32::INFINITY` where the map has no edges at all.
pub fn distance_transform(edges: &[bool], w: usize, h: usize) -> Vec<f32> {
    const INF: f32 = 1e9;
    let mut d = vec![INF; w * h];
    for (i, &e) in edges.iter().enumerate() {
        if e {
            d[i] = 0.0;
        }
    }
    // Forward pass.
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let mut v = d[i];
            if x > 0 {
                v = v.min(d[i - 1] + 3.0);
            }
            if y > 0 {
                v = v.min(d[i - w] + 3.0);
                if x > 0 {
                    v = v.min(d[i - w - 1] + 4.0);
                }
                if x + 1 < w {
                    v = v.min(d[i - w + 1] + 4.0);
                }
            }
            d[i] = v;
        }
    }
    // Backward pass.
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = y * w + x;
            let mut v = d[i];
            if x + 1 < w {
                v = v.min(d[i + 1] + 3.0);
            }
            if y + 1 < h {
                v = v.min(d[i + w] + 3.0);
                if x + 1 < w {
                    v = v.min(d[i + w + 1] + 4.0);
                }
                if x > 0 {
                    v = v.min(d[i + w - 1] + 4.0);
                }
            }
            d[i] = v;
        }
    }
    for v in &mut d {
        *v = if *v >= INF { f32::INFINITY } else { *v / 3.0 };
    }
    d
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Chamfer {
    /// Mean distance from each edge pixel of one map to the nearest edge of the other,
    /// averaged both ways. 0 when the edge maps coincide.
    pub mean: f32,
    /// Edge pixels in a and in b.
    pub edges_a: u32,
    pub edges_b: u32,
}
/// Symmetric chamfer distance between the edge maps of two images. Both empty → 0;
/// one empty → the other's edge count, so a vanished outline is not "perfect".
pub fn chamfer(a: &Image, b: &Image, threshold: f32) -> Chamfer {
    let b = same_size(a, b);
    let (w, h) = (a.width as usize, a.height as usize);
    let ea = sobel_edges(&a.gray(), threshold);
    let eb = sobel_edges(&b.gray(), threshold);
    let na = ea.iter().filter(|e| **e).count() as u32;
    let nb = eb.iter().filter(|e| **e).count() as u32;
    if na == 0 && nb == 0 {
        return Chamfer {
            mean: 0.0,
            edges_a: 0,
            edges_b: 0,
        };
    }
    if na == 0 || nb == 0 {
        return Chamfer {
            mean: (na.max(nb) as f32).sqrt(),
            edges_a: na,
            edges_b: nb,
        };
    }
    let da = distance_transform(&ea, w, h);
    let db = distance_transform(&eb, w, h);
    let ab: f32 = ea
        .iter()
        .enumerate()
        .filter(|(_, e)| **e)
        .map(|(i, _)| db[i])
        .sum::<f32>()
        / na as f32;
    let ba: f32 = eb
        .iter()
        .enumerate()
        .filter(|(_, e)| **e)
        .map(|(i, _)| da[i])
        .sum::<f32>()
        / nb as f32;
    Chamfer {
        mean: (ab + ba) / 2.0,
        edges_a: na,
        edges_b: nb,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub rect: Rect,
    /// Mean absolute RGB difference in the block, 0..=255.
    pub score: f32,
}
/// Mean absolute difference per `block`×`block` tile; the tiles over `threshold`, and
/// their bounding rectangle. The way to localize a change when no semantic tree says
/// where to look (a canvas).
pub fn block_diff(a: &Image, b: &Image, block: u32, threshold: f32) -> (Vec<Block>, Option<Rect>) {
    let b = same_size(a, b);
    let block = block.max(1);
    let mut out = Vec::new();
    let mut bounds: Option<Rect> = None;
    let mut y = 0;
    while y < a.height {
        let mut x = 0;
        while x < a.width {
            let r = Rect::new(
                x as i32,
                y as i32,
                block.min(a.width - x) as i32,
                block.min(a.height - y) as i32,
            );
            let mut sum = 0u64;
            for yy in r.y..r.bottom() {
                let row = (yy as u32 * a.width) as usize * 4;
                for xx in r.x..r.right() {
                    let i = row + xx as usize * 4;
                    sum += (a.rgba[i] as i32 - b.rgba[i] as i32).unsigned_abs() as u64
                        + (a.rgba[i + 1] as i32 - b.rgba[i + 1] as i32).unsigned_abs() as u64
                        + (a.rgba[i + 2] as i32 - b.rgba[i + 2] as i32).unsigned_abs() as u64;
                }
            }
            let score = sum as f32 / (r.area() as f32 * 3.0);
            if score > threshold {
                bounds = Some(bounds.map_or(r, |b| b.union(&r)));
                out.push(Block { rect: r, score });
            }
            x += block;
        }
        y += block;
    }
    out.sort_by(|p, q| q.score.partial_cmp(&p.score).unwrap());
    (out, bounds)
}

/// Fraction of pixels that differ in any channel.
pub fn differing_fraction(a: &Image, b: &Image) -> f32 {
    let b = same_size(a, b);
    let n = (a.width * a.height) as usize;
    if n == 0 {
        return 0.0;
    }
    let diff = a
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.rgba.as_chunks::<4>().0)
        .filter(|(p, q)| p[..3] != q[..3])
        .count();
    diff as f32 / n as f32
}

/// Whether the last `n` content hashes are all equal: the screen has settled.
pub fn settled(hashes: &[u64], n: usize) -> bool {
    hashes.len() >= n && hashes[hashes.len() - n..].windows(2).all(|w| w[0] == w[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, c: [u8; 3]) -> Image {
        let mut v = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..w * h {
            v.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
        Image::new(w, h, v)
    }
    fn with_box(mut img: Image, r: Rect, c: [u8; 3]) -> Image {
        for y in r.y..r.bottom() {
            for x in r.x..r.right() {
                let i = ((y as u32 * img.width + x as u32) * 4) as usize;
                img.rgba[i..i + 3].copy_from_slice(&c);
            }
        }
        img
    }

    #[test]
    fn identical_images_are_perfect() {
        let a = with_box(
            solid(64, 64, [240, 240, 240]),
            Rect::new(10, 10, 20, 20),
            [30, 60, 200],
        );
        assert_eq!(delta_e(&a, &a).mean, 0.0);
        assert!((ssim(&a, &a) - 1.0).abs() < 1e-4);
        assert_eq!(chamfer(&a, &a, 32.0).mean, 0.0);
        assert_eq!(hamming(dhash(&a), dhash(&a)), 0);
        assert_eq!(differing_fraction(&a, &a), 0.0);
    }

    #[test]
    fn colour_shift_shows_in_delta_e_not_shape() {
        let a = with_box(
            solid(64, 64, [240, 240, 240]),
            Rect::new(10, 10, 20, 20),
            [30, 60, 200],
        );
        let b = with_box(
            solid(64, 64, [240, 240, 240]),
            Rect::new(10, 10, 20, 20),
            [30, 60, 230],
        );
        let de = delta_e(&a, &b);
        assert!(de.mean > 0.5 && de.mean < 10.0, "{de:?}");
        assert!(de.max > 5.0);
        let ch = chamfer(&a, &b, 32.0);
        assert!(ch.mean < 0.01, "{ch:?}");
    }

    #[test]
    fn shift_shows_in_chamfer() {
        let a = with_box(
            solid(64, 64, [255, 255, 255]),
            Rect::new(10, 10, 20, 20),
            [0, 0, 0],
        );
        let b = with_box(
            solid(64, 64, [255, 255, 255]),
            Rect::new(14, 10, 20, 20),
            [0, 0, 0],
        );
        let ch = chamfer(&a, &b, 32.0);
        assert!(ch.mean > 1.0 && ch.mean < 4.5, "{ch:?}");
        assert!(ssim(&a, &b) < 0.95);
        let (blocks, bounds) = block_diff(&a, &b, 8, 1.0);
        assert!(!blocks.is_empty());
        let bounds = bounds.unwrap();
        assert!(bounds.x >= 8 && bounds.right() <= 40, "{bounds:?}");
    }

    #[test]
    fn vanished_outline_is_not_perfect() {
        let a = with_box(
            solid(32, 32, [255, 255, 255]),
            Rect::new(8, 8, 12, 12),
            [0, 0, 0],
        );
        let b = solid(32, 32, [255, 255, 255]);
        assert!(chamfer(&a, &b, 32.0).mean > 1.0);
        assert!(ssim(&a, &b) < 0.9);
    }

    #[test]
    fn resize_preserves_solid_colour() {
        let a = solid(10, 10, [10, 20, 30]);
        let b = a.resize(5, 7);
        assert!(b
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [10, 20, 30, 255]));
    }

    #[test]
    fn distance_transform_counts_pixels() {
        let w = 5;
        let mut e = vec![false; 25];
        e[12] = true; // centre
        let d = distance_transform(&e, w, 5);
        assert_eq!(d[12], 0.0);
        assert_eq!(d[11], 1.0);
        assert!((d[6] - 4.0 / 3.0).abs() < 1e-6); // diagonal
        assert_eq!(d[10], 2.0);
    }

    #[test]
    fn settle_detection() {
        assert!(settled(&[1, 2, 3, 3, 3], 3));
        assert!(!settled(&[1, 2, 3, 3], 3));
        assert!(!settled(&[3, 3], 3));
    }

    #[test]
    fn lab_white_and_black() {
        let w = rgb_to_lab(255, 255, 255);
        assert!(
            (w[0] - 100.0).abs() < 0.1 && w[1].abs() < 0.1 && w[2].abs() < 0.1,
            "{w:?}"
        );
        let k = rgb_to_lab(0, 0, 0);
        assert!(k[0].abs() < 0.01);
    }
}
