//! The table-driven proportional faces: the four platform UI families and the
//! thirteen web families, four styles each, and what the renderer does when a
//! family lacks a style.
//!
//! Every family has a slot for regular, bold, italic and bold italic
//! (`bold + 2 * italic`). A slot that is `None` is drawn from the nearest face the
//! family does have, in the order [`face_source`] fixes — the same order
//! `cw_scene::Typeface::web_face_index` measures with, so layout and pixels agree —
//! with the missing weight or slant synthesised on the rasterised coverage:
//! [`embolden`] smears each row one pixel to the right, [`oblique`] shears the bitmap
//! by about 12° about the baseline. Both are integer arithmetic over coverage bytes,
//! so a synthetic glyph is the same bits on every target. Every bundled family
//! currently ships all four files, so the synthetic path is exercised by its tests
//! rather than by any bundled face.
use cw_scene::Typeface;
use fontdue::Metrics;

/// Families in the face table, in slot order.
pub(crate) const FAMILIES: [Typeface; 17] = [
    Typeface::Inter,
    Typeface::OpenSans,
    Typeface::Ubuntu,
    Typeface::Roboto,
    Typeface::Arimo,
    Typeface::Tinos,
    Typeface::Cousine,
    Typeface::Gelasio,
    Typeface::Carlito,
    Typeface::Caladea,
    Typeface::Lato,
    Typeface::SourceSans,
    Typeface::SourceSerif,
    Typeface::Poppins,
    Typeface::Montserrat,
    Typeface::Playfair,
    Typeface::JetBrainsMono,
];
pub(crate) const FACE_COUNT: usize = FAMILIES.len() * 4;

#[allow(unused_macros)]
macro_rules! face {
    ($name:literal) => {
        Some(include_bytes!(concat!(env!("CW_FONTS_DIR"), "/", $name)) as &[u8])
    };
}
/// The thirteen web families are in the module only without `lazy-fonts`; with it
/// they are fetched on first use (`font_pack::web_face_bytes`), pinned by SHA-256.
#[cfg(not(feature = "lazy-fonts"))]
macro_rules! web_face {
    ($name:literal) => {
        face!($name)
    };
}
#[cfg(feature = "lazy-fonts")]
macro_rules! web_face {
    ($name:literal) => {
        None
    };
}
/// A platform's UI family is in the module when the build draws that platform, or when it
/// fetches nothing (`lazy-fonts` off); a lazy build fetches the other platforms' families
/// on first use (a page names them), pinned by SHA-256 (`font_pack::UI_FACES`). A build
/// that fetches what it draws with (`lazy-render`) fetches its own platform's family too,
/// the first time a frame draws with it.
macro_rules! ui_face {
    ($name:literal, $($platform:literal),+) => {{
        #[cfg(any(not(feature = "lazy-fonts"), all(not(feature = "lazy-render"), any($(feature = $platform),+))))]
        let face = face!($name);
        #[cfg(not(any(not(feature = "lazy-fonts"), all(not(feature = "lazy-render"), any($(feature = $platform),+)))))]
        let face = None;
        face
    }};
}
/// Whether face-table slot `slot` (a UI family's face) is a family of a platform this
/// build draws: in a `lazy-render` build, a file every frame of that platform draws with.
pub fn draws_ui_slot(slot: usize) -> bool {
    let drawn = [
        cfg!(any(feature = "os-macos", feature = "os-ios")),
        cfg!(feature = "os-windows"),
        cfg!(feature = "os-ubuntu"),
        cfg!(feature = "os-android"),
    ];
    drawn.get(slot / 4).copied().unwrap_or(false)
}
/// Subsets of the families above, four per family (regular, bold, italic, bold
/// italic); `None` where a family has no such file, or where the file is fetched on
/// first use (a web family, or another platform's UI family, under `lazy-fonts`; see
/// [`face_bytes`]). In a static so each face is in
/// the binary once. The platform families are Latin subsets; the web families are
/// subset to DejaVu's coverage (see `assets/build-fonts.py --web`).
pub(crate) static FACE_BYTES: [Option<&[u8]>; FACE_COUNT] = [
    ui_face!("inter-regular.ttf", "os-macos", "os-ios"),
    ui_face!("inter-bold.ttf", "os-macos", "os-ios"),
    ui_face!("inter-italic.ttf", "os-macos", "os-ios"),
    ui_face!("inter-bold-italic.ttf", "os-macos", "os-ios"),
    ui_face!("opensans-regular.ttf", "os-windows"),
    ui_face!("opensans-bold.ttf", "os-windows"),
    ui_face!("opensans-italic.ttf", "os-windows"),
    ui_face!("opensans-bold-italic.ttf", "os-windows"),
    ui_face!("ubuntu-regular.ttf", "os-ubuntu"),
    ui_face!("ubuntu-bold.ttf", "os-ubuntu"),
    ui_face!("ubuntu-italic.ttf", "os-ubuntu"),
    ui_face!("ubuntu-bold-italic.ttf", "os-ubuntu"),
    ui_face!("roboto-regular.ttf", "os-android"),
    ui_face!("roboto-bold.ttf", "os-android"),
    ui_face!("roboto-italic.ttf", "os-android"),
    ui_face!("roboto-bold-italic.ttf", "os-android"),
    web_face!("arimo-regular.ttf"),
    web_face!("arimo-bold.ttf"),
    web_face!("arimo-italic.ttf"),
    web_face!("arimo-bold-italic.ttf"),
    web_face!("tinos-regular.ttf"),
    web_face!("tinos-bold.ttf"),
    web_face!("tinos-italic.ttf"),
    web_face!("tinos-bold-italic.ttf"),
    web_face!("cousine-regular.ttf"),
    web_face!("cousine-bold.ttf"),
    web_face!("cousine-italic.ttf"),
    web_face!("cousine-bold-italic.ttf"),
    web_face!("gelasio-regular.ttf"),
    web_face!("gelasio-bold.ttf"),
    web_face!("gelasio-italic.ttf"),
    web_face!("gelasio-bold-italic.ttf"),
    web_face!("carlito-regular.ttf"),
    web_face!("carlito-bold.ttf"),
    web_face!("carlito-italic.ttf"),
    web_face!("carlito-bold-italic.ttf"),
    web_face!("caladea-regular.ttf"),
    web_face!("caladea-bold.ttf"),
    web_face!("caladea-italic.ttf"),
    web_face!("caladea-bold-italic.ttf"),
    web_face!("lato-regular.ttf"),
    web_face!("lato-bold.ttf"),
    web_face!("lato-italic.ttf"),
    web_face!("lato-bold-italic.ttf"),
    web_face!("sourcesans-regular.ttf"),
    web_face!("sourcesans-bold.ttf"),
    web_face!("sourcesans-italic.ttf"),
    web_face!("sourcesans-bold-italic.ttf"),
    web_face!("sourceserif-regular.ttf"),
    web_face!("sourceserif-bold.ttf"),
    web_face!("sourceserif-italic.ttf"),
    web_face!("sourceserif-bold-italic.ttf"),
    web_face!("poppins-regular.ttf"),
    web_face!("poppins-bold.ttf"),
    web_face!("poppins-italic.ttf"),
    web_face!("poppins-bold-italic.ttf"),
    web_face!("montserrat-regular.ttf"),
    web_face!("montserrat-bold.ttf"),
    web_face!("montserrat-italic.ttf"),
    web_face!("montserrat-bold-italic.ttf"),
    web_face!("playfair-regular.ttf"),
    web_face!("playfair-bold.ttf"),
    web_face!("playfair-italic.ttf"),
    web_face!("playfair-bold-italic.ttf"),
    web_face!("jetbrainsmono-regular.ttf"),
    web_face!("jetbrainsmono-bold.ttf"),
    web_face!("jetbrainsmono-italic.ttf"),
    web_face!("jetbrainsmono-bold-italic.ttf"),
];

/// Whether the family ships a file for `slot`, embedded or fetched on first use.
pub(crate) fn has_file(slot: usize) -> bool {
    FACE_BYTES[slot].is_some() || crate::font_pack::web_face(slot).is_some()
}

/// The outline bytes of `slot`: embedded, or a web face fetched on first use. `None`
/// while a fetched face is not here; the frame drawing it is then refused (see
/// `font_pack::take_missing_fonts`), never drawn with another face in its place.
pub(crate) fn face_bytes(slot: usize) -> Option<&'static [u8]> {
    #[cfg(test)]
    if crate::font_pack::HIDE_WEB.with(|h| h.get()) && crate::font_pack::web_face(slot).is_some() {
        return crate::font_pack::web_face_bytes(slot);
    }
    FACE_BYTES[slot].or_else(|| crate::font_pack::web_face_bytes(slot))
}

/// Slot of a family's face for `bold`/`italic`; `None` for DejaVu and the
/// monospace terminal face, which have their own fonts.
pub(crate) fn face_index(typeface: Typeface, bold: bool, italic: bool) -> Option<usize> {
    let family = FAMILIES.iter().position(|f| *f == typeface)?;
    Some(family * 4 + usize::from(bold) + 2 * usize::from(italic))
}

/// The file a requested slot is drawn from, and what must be synthesised on top:
/// `(slot of the file, synthetic bold, synthetic oblique)`. A present slot is drawn
/// as is; a missing one falls to the upright of the same weight, then the regular
/// of the same slant, then the regular — the order `Typeface::web_face_index`
/// measures with.
pub(crate) fn face_source(requested: usize) -> (usize, bool, bool) {
    for candidate in [requested, requested & !2, requested & !1, requested & !3] {
        if has_file(candidate) {
            let missing = requested & !candidate;
            return (candidate, missing & 1 != 0, missing & 2 != 0);
        }
    }
    unreachable!("every family in FACE_BYTES has a regular face")
}

/// Synthetic bold: every coverage row is smeared one pixel to the right (each pixel
/// takes the greater of itself and its left neighbour), which thickens vertical
/// stems by one pixel and leaves horizontals as they were. The advance is not
/// changed — layout measured the regular face — so a synthetic bold word sets a
/// little tighter than a designed one would.
pub(crate) fn embolden(metrics: Metrics, alpha: &[u8]) -> (Metrics, Vec<u8>) {
    if metrics.width == 0 || metrics.height == 0 {
        return (metrics, alpha.to_vec());
    }
    let (w, h) = (metrics.width, metrics.height);
    let mut out = vec![0u8; (w + 1) * h];
    for y in 0..h {
        let row = &alpha[y * w..(y + 1) * w];
        let dst = &mut out[y * (w + 1)..(y + 1) * (w + 1)];
        for x in 0..=w {
            let here = if x < w { row[x] } else { 0 };
            let left = if x > 0 { row[x - 1] } else { 0 };
            dst[x] = here.max(left);
        }
    }
    (
        Metrics {
            width: w + 1,
            ..metrics
        },
        out,
    )
}

/// Slope of the synthetic oblique in 1/64 pixel of x per pixel of y: 14/64, close to
/// the 12° most italics lean at (tan 12° = 0.2126).
const SHEAR_64: i64 = 14;

/// Synthetic oblique: the coverage is sheared about the baseline, each row moved
/// right by `SHEAR_64/64` of its height above the baseline (and left below it),
/// with the fractional part of the shift spread over two pixels so the slant is
/// smooth. Everything is integer arithmetic on the coverage bytes.
pub(crate) fn oblique(metrics: Metrics, alpha: &[u8]) -> (Metrics, Vec<u8>) {
    if metrics.width == 0 || metrics.height == 0 {
        return (metrics, alpha.to_vec());
    }
    let (w, h) = (metrics.width, metrics.height);
    // Shift of a row, in 1/64 pixel, from the height of the row's centre above the
    // baseline: row 0 is the top of the bitmap, `ymin + height` pixels up.
    let shift = |row: usize| -> i64 {
        let centre_64 = (i64::from(metrics.ymin) + h as i64 - row as i64) * 64 - 32;
        centre_64 * SHEAR_64 / 64
    };
    let lowest = shift(h - 1).div_euclid(64);
    let highest = shift(0).div_euclid(64) + 1;
    let out_w = w + (highest - lowest) as usize + 1;
    let mut out = vec![0u8; out_w * h];
    for y in 0..h {
        let s = shift(y);
        let whole = (s.div_euclid(64) - lowest) as usize;
        let frac = s.rem_euclid(64) as u32;
        let row = &alpha[y * w..(y + 1) * w];
        let dst = &mut out[y * out_w..(y + 1) * out_w];
        for (x, &a) in row.iter().enumerate() {
            let a = u32::from(a);
            let near = (a * (64 - frac) + 32) / 64;
            let far = (a * frac + 32) / 64;
            dst[x + whole] = dst[x + whole].saturating_add(near as u8);
            dst[x + whole + 1] = dst[x + whole + 1].saturating_add(far as u8);
        }
    }
    (
        Metrics {
            xmin: metrics.xmin + lowest as i32,
            width: out_w,
            ..metrics
        },
        out,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use fontdue::{Font, FontSettings};
    use sha2::{Digest, Sha256};

    #[test]
    fn web_faces_match_their_pinned_hashes_and_carry_their_names() {
        // The pins are `crate::font_pack::WEB_FACES`, which a lazy build fetches by:
        // `assets/build-fonts.py --web` produced the files from the masters
        // `fetch-web-sources.py` pins.
        let faces = crate::font_pack::WEB_FACES;
        assert_eq!(faces.len(), Typeface::WEB.len() * 4);
        assert_eq!(faces.iter().map(|f| f.bytes).sum::<usize>(), 4_583_364);
        for (i, web) in faces.iter().enumerate() {
            let file = web.file;
            let typeface = Typeface::WEB[i / 4];
            let slot = face_index(typeface, i % 4 == 1 || i % 4 == 3, i % 4 >= 2).unwrap();
            assert_eq!(slot, web.slot, "{file}");
            let bytes = face_bytes(slot).unwrap_or_else(|| panic!("{file} is bundled"));
            assert_eq!(bytes.len(), web.bytes, "{file}");
            assert_eq!(format!("{:x}", Sha256::digest(bytes)), web.sha256, "{file}");
            // The file keeps its name table, so it still says what it is.
            let face = rustybuzz::ttf_parser::Face::parse(bytes, 0).unwrap();
            let names: Vec<String> = face
                .names()
                .into_iter()
                .map(|n| {
                    if n.is_unicode() {
                        let units: Vec<u16> = n
                            .name
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|&b| u16::from_be_bytes(b))
                            .collect();
                        String::from_utf16_lossy(&units)
                    } else {
                        n.name.iter().map(|&b| char::from(b)).collect()
                    }
                })
                .collect();
            let family = typeface.family_name();
            assert!(
                names.iter().any(|n| n.starts_with(family)),
                "{file}: {names:?} lacks {family}"
            );
            assert!(
                names.iter().any(|n| n.contains("SIL Open Font License")
                    || n.contains("http://scripts.sil.org/OFL")
                    || n.contains("openfontlicense.org")),
                "{file}: no licence in the name table: {names:?}"
            );
            assert!(face.is_monospaced() == typeface.is_monospace(), "{file}");
            assert_eq!(face.is_italic() || face.is_oblique(), i % 4 >= 2, "{file}");
            // Instanced faces carry their weight in OS/2 (the instancer does not set
            // the style bits), static ones in both.
            let bold = i % 4 == 1 || i % 4 == 3;
            assert_eq!(face.weight().to_number() >= 600, bold, "{file}");
        }
    }
    #[test]
    fn every_family_has_a_regular_face_and_present_slots_draw_themselves() {
        for (family, typeface) in FAMILIES.iter().enumerate() {
            assert!(has_file(family * 4), "{typeface:?} regular");
            for slot in family * 4..family * 4 + 4 {
                if has_file(slot) {
                    assert_eq!(face_source(slot), (slot, false, false));
                }
                // What the renderer picks is what layout measured with.
                if let Some(i) = typeface.web_face_index(slot % 2 == 1, slot % 4 >= 2) {
                    assert_eq!(face_source(slot).0, family * 4 + i, "{typeface:?}");
                }
            }
        }
        assert_eq!(face_index(Typeface::DejaVu, true, true), None);
        assert_eq!(face_index(Typeface::Mono, false, false), None);
        assert_eq!(face_index(Typeface::Inter, false, false), Some(0));
        assert_eq!(face_index(Typeface::Arimo, true, true), Some(19));
    }
    #[test]
    fn synthetic_bold_thickens_stems_and_synthetic_oblique_leans_right() {
        let font = Font::from_bytes(face_bytes(16).unwrap(), FontSettings::default()).unwrap();
        let (metrics, alpha) = font.rasterize('l', 24.0);
        let ink = |a: &[u8]| a.iter().map(|&v| u32::from(v)).sum::<u32>();
        let (bold, bold_alpha) = embolden(metrics, &alpha);
        assert_eq!(
            (bold.width, bold.height),
            (metrics.width + 1, metrics.height)
        );
        assert!(ink(&bold_alpha) > ink(&alpha) * 5 / 4, "bold has more ink");
        assert_eq!(embolden(bold, &bold_alpha).0.width, metrics.width + 2);
        let (slant, slant_alpha) = oblique(metrics, &alpha);
        assert_eq!(slant.height, metrics.height);
        assert!(slant.width > metrics.width);
        // The shear conserves ink (to rounding) and moves the top of the stem right
        // of its foot.
        let total = ink(&slant_alpha);
        assert!(total.abs_diff(ink(&alpha)) <= metrics.height as u32 * 2);
        let centre = |row: usize| {
            let r = &slant_alpha[row * slant.width..(row + 1) * slant.width];
            let weight: u32 = r.iter().map(|&v| u32::from(v)).sum();
            r.iter()
                .enumerate()
                .map(|(x, &v)| x as u32 * u32::from(v))
                .sum::<u32>()
                * 64
                / weight.max(1)
        };
        let lean = centre(0) as i64 - centre(metrics.height - 1) as i64;
        let expected = (metrics.height as i64 - 1) * SHEAR_64;
        assert!(
            (lean - expected).abs() <= 64,
            "lean {lean} expected {expected}"
        );
        // Deterministic: the same input is the same bits.
        assert_eq!(oblique(metrics, &alpha), (slant, slant_alpha));
        // Empty glyphs pass through.
        let (space, none) = font.rasterize(' ', 24.0);
        assert_eq!(embolden(space, &none).1, none);
        assert_eq!(oblique(space, &none).1, none);
    }
}
