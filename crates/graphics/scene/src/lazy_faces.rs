//! The script faces and the font pack's layout stubs, fetched on first use (feature
//! `lazy-faces`, a lazy engine's build; the engine's design notes § fonts on first use).
//!
//! A lazy engine carries none of the faces text is shaped with but the emoji stub, which
//! every run of text consults. The first time a run of Hebrew, Arabic, Devanagari, Han
//! … is shaped, or a module asks the core for the face, the engine asks the host for the
//! file by name and SHA-256 ([`set_face_loader`]) and waits. Bytes that do not hash to
//! the pin are refused. Text is never shaped with another face in place of one: an
//! engine whose host cannot give a face stops (a panic), as it would without its code.
use crate::text::FaceId;
use sha2::{Digest, Sha256};
use std::sync::{OnceLock, RwLock};

/// One face this build fetches: the face, its file under `fonts/` beside the engine,
/// the SHA-256 it is pinned to and its size.
#[derive(Debug, Clone, Copy)]
pub struct LazyFace {
    pub face: FaceId,
    pub file: &'static str,
    pub sha256: &'static str,
    pub bytes: usize,
}
const fn lazy(face: FaceId, file: &'static str, sha256: &'static str, bytes: usize) -> LazyFace {
    LazyFace {
        face,
        file,
        sha256,
        bytes,
    }
}
/// The fourteen embedded script faces, then the stubs of the pack faces (by the regular
/// twin, as layout asks for them) but `Emoji`'s.
pub const LAZY_FACES: [LazyFace; 27] = [
    lazy(
        FaceId::Hebrew,
        "noto-hebrew-regular.ttf",
        "c569ca5340b744c044d65211eb66b18742fec981f272157f35719d17e21d6f73",
        15012,
    ),
    lazy(
        FaceId::HebrewBold,
        "noto-hebrew-bold.ttf",
        "415f2f367494fedc2e42e120f176a8eb26eb867dff3193fcd3037356bfe87c6b",
        15048,
    ),
    lazy(
        FaceId::Arabic,
        "noto-arabic-regular.ttf",
        "b47b4e67898954df8f24a97d6ef228fa66275c82a5a150f41da7a03bc925a869",
        58088,
    ),
    lazy(
        FaceId::ArabicBold,
        "noto-arabic-bold.ttf",
        "33a86b9331c265555e3c8e780e9073e103b997c1b77545c811da9c3ccba71307",
        58208,
    ),
    lazy(
        FaceId::Thai,
        "noto-thai-regular.ttf",
        "44fb93b093448570d721d56826ce18a209567b038fdff96b81c6ff0b41b6e42a",
        19084,
    ),
    lazy(
        FaceId::ThaiBold,
        "noto-thai-bold.ttf",
        "e7f09faaff5a348173c16c59a4256ad6c607ea1c53a68898116d75359d58728d",
        19144,
    ),
    lazy(
        FaceId::Devanagari,
        "noto-devanagari-regular.ttf",
        "8c0cde2842c55b7cdc80f3b4187550ea9c26c41c8f1d9d5f0d591ced3a946b9a",
        163848,
    ),
    lazy(
        FaceId::DevanagariBold,
        "noto-devanagari-bold.ttf",
        "4a6b9d62f08779ef24f34c31caba22e7027e836753a74b1871cd981db14e46b0",
        164428,
    ),
    lazy(
        FaceId::Bengali,
        "noto-bengali-regular.ttf",
        "a6f6c2f3edfb04e19ef2d9a64db0cdc271405c4aec618430c81ada363e54b33d",
        96648,
    ),
    lazy(
        FaceId::BengaliBold,
        "noto-bengali-bold.ttf",
        "4172185fcfbf149faf067f5c52f18dee878b7ac37df31c1aa410b91f382b553e",
        97092,
    ),
    lazy(
        FaceId::Georgian,
        "noto-georgian-regular.ttf",
        "33655636b21fc40e1003c557a7f1f1370be9cd5310983de4e651e0b60ea4f82b",
        33096,
    ),
    lazy(
        FaceId::GeorgianBold,
        "noto-georgian-bold.ttf",
        "6f3c002d56d9f25a770af5dfa509254fdd020f3b632f9a8e7bd6dbb26440bf91",
        33108,
    ),
    lazy(
        FaceId::Armenian,
        "noto-armenian-regular.ttf",
        "d3a715e88cd0427ef9756d819acde9c9a6eecb5264b8e801492e907de878f52d",
        18544,
    ),
    lazy(
        FaceId::ArmenianBold,
        "noto-armenian-bold.ttf",
        "b4f56423acb286b6425a4884a8ec988b6b9429341a2f70f82885cc87bcb54bc0",
        18580,
    ),
    lazy(
        FaceId::Han,
        "stubs/noto-sans-sc.ttf",
        "1815465c98b9aea3b2cdfdd35ca44bea0acc50b130f4340e47fe035386562f31",
        104392,
    ),
    lazy(
        FaceId::Hangul,
        "stubs/noto-sans-kr.ttf",
        "4f3fb9c68e6822f1bd440687d03973124a8d00ab9710d40c0bb378967e916fde",
        46600,
    ),
    lazy(
        FaceId::HanTc,
        "stubs/noto-sans-tc.ttf",
        "73d774c9b795b5083df25e9d7368e946c59b0edcf5ecaba4e3436488a4dad92a",
        28468,
    ),
    lazy(
        FaceId::HanJp,
        "stubs/noto-sans-jp.ttf",
        "899c7316cdf1a3d6eb55e8cc98fb03360b8d3679a885c6d0fea44242dda13329",
        20616,
    ),
    lazy(
        FaceId::HanKr,
        "stubs/noto-sans-kr-han.ttf",
        "31a3d902668e57228874927f0e85d72293716cb237ada28d5faa1a500ab26db0",
        31424,
    ),
    lazy(
        FaceId::Tamil,
        "stubs/noto-tamil.ttf",
        "361a6e76c2b5ebe98d2df541cf9391e54f626d6edc35e208fb8cedb76aa22b8a",
        8320,
    ),
    lazy(
        FaceId::Gurmukhi,
        "stubs/noto-gurmukhi.ttf",
        "b8d5f5f882c32e56d745666bf230f987f6ab5dd6186c2a1f8b74f4429e8f00ec",
        7088,
    ),
    lazy(
        FaceId::Lao,
        "stubs/noto-lao.ttf",
        "e1b6e91b8ee19b156b1e381084f3a38703ed5e1d38f1bab8c0e17451f189ea2a",
        7116,
    ),
    lazy(
        FaceId::Khmer,
        "stubs/noto-khmer.ttf",
        "f76bfd140b6610664756d448896aa8c42c4a0224ca621cab0997f33318346f9e",
        15832,
    ),
    lazy(
        FaceId::Gujarati,
        "stubs/noto-gujarati.ttf",
        "e07fb92edb1bbe04d6e8a57ed61883e17f31df5113c605c5e2b3d2b0c0f4885a",
        42220,
    ),
    lazy(
        FaceId::Ethiopic,
        "stubs/noto-ethiopic.ttf",
        "3baaf079636c640ecfbf677b3554d0b96531d6625f6752b08ce94477605b03b8",
        4804,
    ),
    lazy(
        FaceId::Myanmar,
        "stubs/noto-myanmar.ttf",
        "dcac51e3e9e0b150152fbb5c0bf4e2c615e9a350ec4cdd2a28d8c0a373449a1b",
        52796,
    ),
    lazy(
        FaceId::Sinhala,
        "stubs/noto-sinhala.ttf",
        "696314e12bff137cd7a0440b01a590b0bb8e5cabcff3672b6ea29d8558ece70a",
        19792,
    ),
];

/// Fetches one font file by name and SHA-256 for the engine, waiting for it; `None`
/// when the host cannot.
pub type FaceLoader = fn(file: &str, sha256: &str) -> Option<Vec<u8>>;
static LOADER: RwLock<Option<FaceLoader>> = RwLock::new(None);
/// Install (or remove) the host the faces are fetched from.
pub fn set_face_loader(loader: Option<FaceLoader>) {
    *LOADER.write().unwrap_or_else(|e| e.into_inner()) = loader;
}

/// One table file a `lazy-metrics` build fetches: its path under `fonts/`, the SHA-256
/// it is pinned to and its size (build.rs writes the list).
#[derive(Debug, Clone, Copy)]
pub struct MetricFile {
    pub file: &'static str,
    pub sha256: &'static str,
    pub bytes: usize,
}
include!(concat!(env!("OUT_DIR"), "/metric_files.rs"));

/// The bytes of the table file `file` (`metrics/<name>.bin`), fetched the first time.
/// Text is never measured with other tables: without them the engine stops (a panic).
#[cfg(feature = "lazy-metrics")]
pub(crate) fn metric_bytes(file: &str) -> &'static [u8] {
    static HELD: [OnceLock<&'static [u8]>; METRIC_FILES.len()] =
        [const { OnceLock::new() }; METRIC_FILES.len()];
    let i = METRIC_FILES
        .iter()
        .position(|f| f.file == file)
        .unwrap_or_else(|| panic!("{file} is no table this engine pins"));
    HELD[i].get_or_init(|| fetch(METRIC_FILES[i].file, METRIC_FILES[i].sha256))
}
fn fetch(file: &str, sha256: &str) -> &'static [u8] {
    let set = *LOADER.read().unwrap_or_else(|e| e.into_inner());
    // Tests read the files where the build keeps them.
    #[cfg(test)]
    let set = set.or(Some(tests::files as FaceLoader));
    let loader = set.unwrap_or_else(|| {
        panic!("fonts/{file} is fetched on first use and this engine has no host to fetch it")
    });
    let bytes =
        loader(file, sha256).unwrap_or_else(|| panic!("the host could not give fonts/{file}"));
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        sha256,
        "fonts/{file} is not the file this engine was built with"
    );
    Box::leak(bytes.into_boxed_slice())
}

/// The bytes of `face` (a script face, or a pack face's stub), fetched the first time.
pub(crate) fn bytes(face: FaceId) -> &'static [u8] {
    static HELD: [OnceLock<&'static [u8]>; FaceId::COUNT] =
        [const { OnceLock::new() }; FaceId::COUNT];
    HELD[face as usize].get_or_init(|| {
        let pin = LAZY_FACES
            .iter()
            .find(|f| f.face == face)
            .unwrap_or_else(|| panic!("{face:?} has no face file"));
        fetch(pin.file, pin.sha256)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A face from the assets, or a table from the folder the build pinned them from.
    pub(super) fn files(file: &str, _sha256: &str) -> Option<Vec<u8>> {
        let path = match file.strip_prefix("metrics/") {
            Some(name) => std::path::Path::new(option_env!("CW_METRICS_DIR")?).join(name),
            None => std::path::Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../render/assets/fonts"
            ))
            .join(file),
        };
        std::fs::read(path).ok()
    }

    #[test]
    fn every_pin_is_the_file_in_the_assets() {
        let fonts = concat!(env!("CARGO_MANIFEST_DIR"), "/../render/assets/fonts/");
        for pin in LAZY_FACES {
            let bytes = std::fs::read(format!("{fonts}{}", pin.file)).unwrap();
            assert_eq!(bytes.len(), pin.bytes, "{}", pin.file);
            assert_eq!(
                format!("{:x}", Sha256::digest(&bytes)),
                pin.sha256,
                "{}",
                pin.file
            );
        }
    }

    #[test]
    fn a_fetched_face_is_the_embedded_one() {
        fn from_assets(file: &str, sha256: &str) -> Option<Vec<u8>> {
            let bytes = files(file, sha256)?;
            (format!("{:x}", Sha256::digest(&bytes)) == sha256).then_some(bytes)
        }
        set_face_loader(Some(from_assets));
        for pin in LAZY_FACES {
            assert!(
                rustybuzz::Face::from_slice(bytes(pin.face), 0).is_some(),
                "{}",
                pin.file
            );
        }
    }
}
