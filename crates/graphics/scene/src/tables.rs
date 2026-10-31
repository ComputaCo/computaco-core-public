//! The tables text is measured with (advances, kern pairs) and the faces it is shaped
//! with, wherever they are kept.
//!
//! An engine embeds them. An application module (feature `core-text`) does not: it asks
//! the core for each one the first time it measures with it ([`set_source`]) and keeps
//! it for the life of the instance. The core answers with [`export`], from the very
//! tables it measures with, so a module lays text out exactly as the engine does
//! (`ops/engineering/app-modules.md` § text through the core).
#[cfg(not(feature = "core-text"))]
use crate::{metrics::Typeface, text::FaceId};

/// A face's advance table and units per em.
pub type Table = (&'static [(u32, u16)], u32);
/// A face a family may not have.
pub type Face = Option<Table>;
/// A face's kern pairs: (left codepoint, start), right codepoints, adjustments.
pub type Pairs = Option<(&'static [(u16, u32)], &'static [u16], &'static [i16])>;

/// What a module asks the core for, one byte of kind and its arguments.
mod key {
    pub const WEB_FACES: u8 = b'W';
    pub const WEB_KERNING: u8 = b'K';
    pub const DEJAVU_KERNING: u8 = b'k';
    pub const BASE_TABLE: u8 = b'T';
    pub const DEJAVU_WEB: u8 = b'D';
    pub const FACE: u8 = b'F';
}

/// The thirteen web families, whose tables a `lazy-metrics` build fetches.
pub fn is_web(t: crate::metrics::Typeface) -> bool {
    use crate::metrics::Typeface::*;
    matches!(
        t,
        Arimo
            | Tinos
            | Cousine
            | Gelasio
            | Carlito
            | Caladea
            | Lato
            | SourceSans
            | SourceSerif
            | Poppins
            | Montserrat
            | Playfair
            | JetBrainsMono
    )
}
/// The file a web family's `kind` of table (`faces`, `kerning`) is served as, under `fonts/metrics/`.
pub fn web_file(t: crate::metrics::Typeface, kind: &str) -> String {
    format!("{}-{kind}.bin", format!("{t:?}").to_lowercase())
}

#[cfg(not(feature = "core-text"))]
pub use local::*;
#[cfg(not(feature = "core-text"))]
mod local {
    #[cfg(feature = "lazy-metrics")]
    use super::wire;
    use super::{Face, Pairs};
    #[cfg(not(feature = "lazy-metrics"))]
    use crate::kerning_data as kerning;
    #[cfg(not(feature = "lazy-metrics"))]
    use crate::kerning_dejavu;
    use crate::metrics::Typeface;
    use crate::metrics_data as data;
    use crate::metrics_dejavu as dejavu_web;
    use crate::metrics_italic as italic;
    #[cfg(not(feature = "lazy-metrics"))]
    use crate::metrics_web as web;
    use crate::text::FaceId;
    #[cfg(feature = "lazy-metrics")]
    use std::sync::OnceLock;

    /// A web family's four faces by `bold + 2 * italic`; `None` for the others.
    #[cfg(feature = "lazy-metrics")]
    pub fn web_faces(t: Typeface) -> Option<&'static [Face; 4]> {
        static HELD: [OnceLock<Option<&'static [Face; 4]>>; Typeface::ALL.len()] =
            [const { OnceLock::new() }; Typeface::ALL.len()];
        if !super::is_web(t) {
            return None;
        }
        *HELD[t as usize].get_or_init(|| {
            let mut r = wire::Reader::new(crate::lazy_faces::metric_bytes(&format!(
                "metrics/{}",
                super::web_file(t, "faces")
            )));
            (r.u8() != 0).then(|| leak([r.face(), r.face(), r.face(), r.face()]))
        })
    }
    /// A web family's kern pairs by `bold + 2 * italic`; `None` for the others.
    #[cfg(feature = "lazy-metrics")]
    pub fn web_kerning(t: Typeface) -> Option<&'static [Pairs; 4]> {
        static HELD: [OnceLock<Option<&'static [Pairs; 4]>>; Typeface::ALL.len()] =
            [const { OnceLock::new() }; Typeface::ALL.len()];
        if !super::is_web(t) {
            return None;
        }
        *HELD[t as usize].get_or_init(|| {
            let mut r = wire::Reader::new(crate::lazy_faces::metric_bytes(&format!(
                "metrics/{}",
                super::web_file(t, "kerning")
            )));
            (r.u8() != 0).then(|| leak([r.pairs(), r.pairs(), r.pairs(), r.pairs()]))
        })
    }
    /// DejaVu Sans's kern pairs, which only web content uses.
    #[cfg(feature = "lazy-metrics")]
    pub fn dejavu_kerning() -> &'static [Pairs; 4] {
        static HELD: OnceLock<&'static [Pairs; 4]> = OnceLock::new();
        HELD.get_or_init(|| {
            let mut r = wire::Reader::new(crate::lazy_faces::metric_bytes(
                "metrics/dejavu-kerning.bin",
            ));
            leak([r.pairs(), r.pairs(), r.pairs(), r.pairs()])
        })
    }
    #[cfg(feature = "lazy-metrics")]
    fn leak<T>(value: T) -> &'static T {
        Box::leak(Box::new(value))
    }
    /// A web family's four faces by `bold + 2 * italic`; `None` for the others.
    #[cfg(not(feature = "lazy-metrics"))]
    pub fn web_faces(t: Typeface) -> Option<&'static [Face; 4]> {
        Some(match t {
            Typeface::Arimo => &web::ARIMO,
            Typeface::Tinos => &web::TINOS,
            Typeface::Cousine => &web::COUSINE,
            Typeface::Gelasio => &web::GELASIO,
            Typeface::Carlito => &web::CARLITO,
            Typeface::Caladea => &web::CALADEA,
            Typeface::Lato => &web::LATO,
            Typeface::SourceSans => &web::SOURCESANS,
            Typeface::SourceSerif => &web::SOURCESERIF,
            Typeface::Poppins => &web::POPPINS,
            Typeface::Montserrat => &web::MONTSERRAT,
            Typeface::Playfair => &web::PLAYFAIR,
            Typeface::JetBrainsMono => &web::JETBRAINSMONO,
            _ => return None,
        })
    }
    /// A web family's kern pairs by `bold + 2 * italic`; `None` for the others.
    #[cfg(not(feature = "lazy-metrics"))]
    pub fn web_kerning(t: Typeface) -> Option<&'static [Pairs; 4]> {
        Some(match t {
            Typeface::Arimo => &kerning::ARIMO,
            Typeface::Tinos => &kerning::TINOS,
            Typeface::Cousine => &kerning::COUSINE,
            Typeface::Gelasio => &kerning::GELASIO,
            Typeface::Carlito => &kerning::CARLITO,
            Typeface::Caladea => &kerning::CALADEA,
            Typeface::Lato => &kerning::LATO,
            Typeface::SourceSans => &kerning::SOURCESANS,
            Typeface::SourceSerif => &kerning::SOURCESERIF,
            Typeface::Poppins => &kerning::POPPINS,
            Typeface::Montserrat => &kerning::MONTSERRAT,
            Typeface::Playfair => &kerning::PLAYFAIR,
            Typeface::JetBrainsMono => &kerning::JETBRAINSMONO,
            _ => return None,
        })
    }
    /// DejaVu Sans's kern pairs, which only web content uses.
    #[cfg(not(feature = "lazy-metrics"))]
    pub fn dejavu_kerning() -> &'static [Pairs; 4] {
        &kerning_dejavu::DEJAVU
    }
    /// The advance table of a platform or DejaVu face.
    pub fn base_table(t: Typeface, bold: bool, slanted: bool) -> (&'static [(u32, u16)], u32) {
        match (t, bold, slanted) {
            // One face serves every weight and slant of the monospace family.
            (Typeface::Mono, _, _) => (data::DEJAVU_MONO, data::DEJAVU_MONO_UPEM),
            (Typeface::DejaVu, false, false) => (data::DEJAVU_REGULAR, data::DEJAVU_REGULAR_UPEM),
            (Typeface::DejaVu, true, false) => (data::DEJAVU_BOLD, data::DEJAVU_BOLD_UPEM),
            (Typeface::Inter, false, false) => (data::INTER_REGULAR, data::INTER_REGULAR_UPEM),
            (Typeface::Inter, true, false) => (data::INTER_BOLD, data::INTER_BOLD_UPEM),
            (Typeface::OpenSans, false, false) => {
                (data::OPENSANS_REGULAR, data::OPENSANS_REGULAR_UPEM)
            }
            (Typeface::OpenSans, true, false) => (data::OPENSANS_BOLD, data::OPENSANS_BOLD_UPEM),
            (Typeface::Ubuntu, false, false) => (data::UBUNTU_REGULAR, data::UBUNTU_REGULAR_UPEM),
            (Typeface::Ubuntu, true, false) => (data::UBUNTU_BOLD, data::UBUNTU_BOLD_UPEM),
            (Typeface::Roboto, false, false) => (data::ROBOTO_REGULAR, data::ROBOTO_REGULAR_UPEM),
            (Typeface::Roboto, true, false) => (data::ROBOTO_BOLD, data::ROBOTO_BOLD_UPEM),
            (Typeface::DejaVu, false, true) => (italic::DEJAVU_ITALIC, italic::DEJAVU_ITALIC_UPEM),
            (Typeface::DejaVu, true, true) => {
                (italic::DEJAVU_BOLD_ITALIC, italic::DEJAVU_BOLD_ITALIC_UPEM)
            }
            (Typeface::Inter, false, true) => (italic::INTER_ITALIC, italic::INTER_ITALIC_UPEM),
            (Typeface::Inter, true, true) => {
                (italic::INTER_BOLD_ITALIC, italic::INTER_BOLD_ITALIC_UPEM)
            }
            (Typeface::OpenSans, false, true) => {
                (italic::OPENSANS_ITALIC, italic::OPENSANS_ITALIC_UPEM)
            }
            (Typeface::OpenSans, true, true) => (
                italic::OPENSANS_BOLD_ITALIC,
                italic::OPENSANS_BOLD_ITALIC_UPEM,
            ),
            (Typeface::Ubuntu, false, true) => (italic::UBUNTU_ITALIC, italic::UBUNTU_ITALIC_UPEM),
            (Typeface::Ubuntu, true, true) => {
                (italic::UBUNTU_BOLD_ITALIC, italic::UBUNTU_BOLD_ITALIC_UPEM)
            }
            (Typeface::Roboto, false, true) => (italic::ROBOTO_ITALIC, italic::ROBOTO_ITALIC_UPEM),
            (Typeface::Roboto, true, true) => {
                (italic::ROBOTO_BOLD_ITALIC, italic::ROBOTO_BOLD_ITALIC_UPEM)
            }
            _ => unreachable!("web families are table-driven through web_faces"),
        }
    }
    /// For web content, the upright DejaVu Sans and Mono faces' advances beyond the
    /// `WIDE` ranges the native tables carry; `None` for every other face.
    pub fn dejavu_web(family: Typeface, bold: bool) -> Option<(&'static [(u32, u16)], u32)> {
        Some(match family {
            Typeface::DejaVu if bold => (
                dejavu_web::DEJAVU_BOLD_WEB,
                dejavu_web::DEJAVU_BOLD_WEB_UPEM,
            ),
            Typeface::DejaVu => (
                dejavu_web::DEJAVU_REGULAR_WEB,
                dejavu_web::DEJAVU_REGULAR_WEB_UPEM,
            ),
            Typeface::Mono => (
                dejavu_web::DEJAVU_MONO_WEB,
                dejavu_web::DEJAVU_MONO_WEB_UPEM,
            ),
            _ => return None,
        })
    }
    /// The bytes `face` is shaped with: an embedded face whole, a pack face's stub.
    #[cfg(feature = "lazy-faces")]
    pub fn face_bytes(face: FaceId) -> &'static [u8] {
        match face {
            FaceId::Emoji => crate::text::EMOJI_STUB,
            face => crate::lazy_faces::bytes(face),
        }
    }
    /// The bytes `face` is shaped with: an embedded face whole, a pack face's stub.
    #[cfg(not(feature = "lazy-faces"))]
    pub fn face_bytes(face: FaceId) -> &'static [u8] {
        use crate::text::{EMBEDDED, STUBS};
        if let Some(bytes) = EMBEDDED.get(face as usize) {
            return bytes;
        }
        STUBS
            .iter()
            .find(|(f, _)| *f == face)
            .map(|(_, bytes)| *bytes)
            .expect("every pack face has a stub")
    }
}

/// The core's answer to a module asking for one table or face (see [`set_source`]):
/// the bytes the module decodes back into exactly what this build measures with.
/// `None` for a request this build does not know.
#[cfg(not(feature = "core-text"))]
pub fn export(request: &[u8]) -> Option<Vec<u8>> {
    let typeface = |i: usize| {
        request
            .get(i)
            .and_then(|&t| Typeface::ALL.get(usize::from(t)).copied())
    };
    let flag = |i: usize| request.get(i).is_some_and(|&b| b != 0);
    let mut out = Vec::new();
    match *request.first()? {
        key::WEB_FACES => match web_faces(typeface(1)?) {
            Some(faces) => {
                out.push(1);
                faces.iter().for_each(|f| wire::put_face(&mut out, *f));
            }
            None => out.push(0),
        },
        key::WEB_KERNING => match web_kerning(typeface(1)?) {
            Some(pairs) => {
                out.push(1);
                pairs.iter().for_each(|p| wire::put_pairs(&mut out, *p));
            }
            None => out.push(0),
        },
        key::DEJAVU_KERNING => dejavu_kerning()
            .iter()
            .for_each(|p| wire::put_pairs(&mut out, *p)),
        key::BASE_TABLE => {
            let t = typeface(1)?;
            if web_faces(t).is_some() {
                return None;
            }
            wire::put_face(&mut out, Some(base_table(t, flag(2), flag(3))));
        }
        key::DEJAVU_WEB => wire::put_face(&mut out, dejavu_web(typeface(1)?, flag(2))),
        key::FACE => {
            let face = FaceId::ALL.get(usize::from(*request.get(1)?)).copied()?;
            if face == FaceId::ColorEmoji {
                return None;
            }
            out.extend_from_slice(face_bytes(face.layout_twin()));
        }
        _ => return None,
    }
    Some(out)
}

#[cfg(feature = "core-text")]
pub use remote::*;
#[cfg(feature = "core-text")]
mod remote {
    use super::{key, wire, Face, Pairs, Table};
    use crate::metrics::Typeface;
    use crate::text::FaceId;
    use std::sync::OnceLock;

    /// How a module asks the core: the request bytes in, the answer out (`export`).
    pub type Source = fn(&[u8]) -> Vec<u8>;
    static SOURCE: OnceLock<Source> = OnceLock::new();
    /// Install the module's way of asking the core (its binding's import).
    pub fn set_source(source: Source) {
        let _ = SOURCE.set(source);
    }
    fn ask(request: &[u8]) -> Vec<u8> {
        let source = SOURCE
            .get()
            .expect("a module measuring text needs the core (tables::set_source)");
        source(request)
    }
    /// Kept for the life of the instance: what the core answered never changes.
    fn leak<T>(value: T) -> &'static T {
        Box::leak(Box::new(value))
    }
    const FAMILIES: usize = Typeface::ALL.len();

    pub fn web_faces(t: Typeface) -> Option<&'static [Face; 4]> {
        static HELD: [OnceLock<Option<&'static [Face; 4]>>; FAMILIES] =
            [const { OnceLock::new() }; FAMILIES];
        *HELD[t as usize].get_or_init(|| {
            let bytes = ask(&[key::WEB_FACES, t as u8]);
            let mut r = wire::Reader::new(&bytes);
            (r.u8() != 0).then(|| leak([r.face(), r.face(), r.face(), r.face()]))
        })
    }
    pub fn web_kerning(t: Typeface) -> Option<&'static [Pairs; 4]> {
        static HELD: [OnceLock<Option<&'static [Pairs; 4]>>; FAMILIES] =
            [const { OnceLock::new() }; FAMILIES];
        *HELD[t as usize].get_or_init(|| {
            let bytes = ask(&[key::WEB_KERNING, t as u8]);
            let mut r = wire::Reader::new(&bytes);
            (r.u8() != 0).then(|| leak([r.pairs(), r.pairs(), r.pairs(), r.pairs()]))
        })
    }
    pub fn dejavu_kerning() -> &'static [Pairs; 4] {
        static HELD: OnceLock<&'static [Pairs; 4]> = OnceLock::new();
        HELD.get_or_init(|| {
            let bytes = ask(&[key::DEJAVU_KERNING]);
            let mut r = wire::Reader::new(&bytes);
            leak([r.pairs(), r.pairs(), r.pairs(), r.pairs()])
        })
    }
    pub fn base_table(t: Typeface, bold: bool, slanted: bool) -> (&'static [(u32, u16)], u32) {
        static HELD: [OnceLock<Table>; FAMILIES * 4] = [const { OnceLock::new() }; FAMILIES * 4];
        *HELD[t as usize * 4 + usize::from(bold) + 2 * usize::from(slanted)].get_or_init(|| {
            let bytes = ask(&[key::BASE_TABLE, t as u8, u8::from(bold), u8::from(slanted)]);
            wire::Reader::new(&bytes)
                .face()
                .expect("the core has the table of every platform and DejaVu face")
        })
    }
    pub fn dejavu_web(family: Typeface, bold: bool) -> Option<(&'static [(u32, u16)], u32)> {
        static HELD: [OnceLock<Face>; FAMILIES * 2] = [const { OnceLock::new() }; FAMILIES * 2];
        *HELD[family as usize * 2 + usize::from(bold)].get_or_init(|| {
            let bytes = ask(&[key::DEJAVU_WEB, family as u8, u8::from(bold)]);
            wire::Reader::new(&bytes).face()
        })
    }
    pub fn face_bytes(face: FaceId) -> &'static [u8] {
        static HELD: [OnceLock<&'static [u8]>; FaceId::COUNT] =
            [const { OnceLock::new() }; FaceId::COUNT];
        HELD[face as usize].get_or_init(|| ask(&[key::FACE, face as u8]).leak())
    }
}

/// Tables as bytes, little-endian, lengths as `u32`.
mod wire {
    #[cfg(any(feature = "core-text", feature = "lazy-metrics", test))]
    use super::{Face, Pairs};

    #[cfg(not(feature = "core-text"))]
    pub fn put_face(out: &mut Vec<u8>, face: super::Face) {
        let Some((table, upem)) = face else {
            out.push(0);
            return;
        };
        out.push(1);
        out.extend_from_slice(&upem.to_le_bytes());
        out.extend_from_slice(&(table.len() as u32).to_le_bytes());
        for (c, a) in table {
            out.extend_from_slice(&c.to_le_bytes());
            out.extend_from_slice(&a.to_le_bytes());
        }
    }
    #[cfg(not(feature = "core-text"))]
    pub fn put_pairs(out: &mut Vec<u8>, pairs: super::Pairs) {
        let Some((lefts, rights, values)) = pairs else {
            out.push(0);
            return;
        };
        out.push(1);
        out.extend_from_slice(&(lefts.len() as u32).to_le_bytes());
        for (l, start) in lefts {
            out.extend_from_slice(&l.to_le_bytes());
            out.extend_from_slice(&start.to_le_bytes());
        }
        out.extend_from_slice(&(rights.len() as u32).to_le_bytes());
        rights
            .iter()
            .for_each(|r| out.extend_from_slice(&r.to_le_bytes()));
        out.extend_from_slice(&(values.len() as u32).to_le_bytes());
        values
            .iter()
            .for_each(|v| out.extend_from_slice(&v.to_le_bytes()));
    }

    #[cfg(any(feature = "core-text", feature = "lazy-metrics", test))]
    pub struct Reader<'a>(&'a [u8]);
    #[cfg(any(feature = "core-text", feature = "lazy-metrics", test))]
    impl<'a> Reader<'a> {
        pub fn new(bytes: &'a [u8]) -> Self {
            Self(bytes)
        }
        fn take<const N: usize>(&mut self) -> [u8; N] {
            let (head, rest) = self.0.split_at(N);
            self.0 = rest;
            head.try_into().expect("split at N")
        }
        pub fn u8(&mut self) -> u8 {
            self.take::<1>()[0]
        }
        fn u16(&mut self) -> u16 {
            u16::from_le_bytes(self.take())
        }
        fn u32(&mut self) -> u32 {
            u32::from_le_bytes(self.take())
        }
        fn i16(&mut self) -> i16 {
            i16::from_le_bytes(self.take())
        }
        fn list<T>(&mut self, mut item: impl FnMut(&mut Self) -> T) -> &'static [T] {
            let n = self.u32() as usize;
            (0..n).map(|_| item(self)).collect::<Vec<T>>().leak()
        }
        pub fn face(&mut self) -> Face {
            if self.u8() == 0 {
                return None;
            }
            let upem = self.u32();
            Some((self.list(|r| (r.u32(), r.u16())), upem))
        }
        pub fn pairs(&mut self) -> Pairs {
            if self.u8() == 0 {
                return None;
            }
            Some((
                self.list(|r| (r.u16(), r.u32())),
                self.list(Self::u16),
                self.list(Self::i16),
            ))
        }
    }
}

#[cfg(all(test, not(feature = "core-text")))]
mod tests {
    use super::*;

    /// What a module decodes from the core's answer is the table the core measures
    /// with, for every key a module can ask: so text in a module lays out as in the engine.
    #[test]
    fn a_module_reads_back_every_table_the_core_measures_with() {
        for t in Typeface::ALL {
            let i = t as u8;
            let bytes = export(&[key::WEB_FACES, i]).unwrap();
            let mut r = wire::Reader::new(&bytes);
            match web_faces(t) {
                Some(faces) => {
                    assert_eq!(r.u8(), 1);
                    for face in faces {
                        assert_eq!(r.face(), *face, "{t:?}");
                    }
                    assert!(export(&[key::BASE_TABLE, i, 0, 0]).is_none());
                }
                None => {
                    assert_eq!(r.u8(), 0);
                    for (bold, slanted) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let bytes = export(&[key::BASE_TABLE, i, bold, slanted]).unwrap();
                        let got = wire::Reader::new(&bytes).face();
                        assert_eq!(got, Some(base_table(t, bold == 1, slanted == 1)));
                    }
                }
            }
            let bytes = export(&[key::WEB_KERNING, i]).unwrap();
            let mut r = wire::Reader::new(&bytes);
            match web_kerning(t) {
                Some(pairs) => {
                    assert_eq!(r.u8(), 1);
                    for p in pairs {
                        assert_eq!(r.pairs(), *p, "{t:?}");
                    }
                }
                None => assert_eq!(r.u8(), 0),
            }
            for bold in [0, 1] {
                let bytes = export(&[key::DEJAVU_WEB, i, bold]).unwrap();
                assert_eq!(wire::Reader::new(&bytes).face(), dejavu_web(t, bold == 1));
            }
        }
        let bytes = export(&[key::DEJAVU_KERNING]).unwrap();
        let mut r = wire::Reader::new(&bytes);
        for p in dejavu_kerning() {
            assert_eq!(r.pairs(), *p);
        }
        for face in FaceId::ALL {
            match export(&[key::FACE, face as u8]) {
                Some(bytes) => assert_eq!(bytes, face_bytes(face.layout_twin()), "{face:?}"),
                None => assert_eq!(face, FaceId::ColorEmoji),
            }
        }
    }
}
