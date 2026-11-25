//! Outline sources for the fallback faces, the on-demand font pack, and the web
//! families fetched on first use.
//!
//! **Web families** (Arimo, Tinos, … JetBrains Mono: [`WEB_FACES`]). A build with
//! the `lazy-fonts` feature leaves their 52 files out of the module, and the UI families
//! of the platforms it does not draw ([`UI_FACES`]), fetched the same way. Layout never
//! needs them: it measures with the advance and kerning tables in `cw_scene`, so a
//! page lays out the same with or without the files. Drawing does need them, and a
//! frame is never drawn with another face in place of one: the renderer asks the
//! host's loader ([`set_font_loader`]) for the file the moment a glyph needs it and
//! waits for the answer, and when no loader can give it the frame is refused
//! ([`take_missing_fonts`] names the files) instead of returned. A page fetches the
//! files, installs them ([`install_font`]) and draws again.
//!
//! Hebrew, Arabic, Thai, Devanagari, Bengali, Georgian and Armenian are embedded
//! everywhere but a lazy engine, which fetches them and the pack's layout stubs the first
//! time text is shaped with them (`cw_scene::lazy_faces`). The CJK faces (Simplified Chinese, its bold, the Traditional Chinese,
//! Japanese and Korean locale faces, Hangul), the emoji faces (monochrome and colour)
//! and eight further scripts (Tamil, Gurmukhi, Lao, Khmer, Gujarati, Ethiopic,
//! Myanmar, Sinhala) are the font pack (the crates cw-font-pack-emoji, -sc and -cjk):
//! the Wasm build leaves them out and a page installs each file with
//! [`install_font`] once it has fetched it; native builds embed them and never need
//! to. Layout does not wait for the pack — it shapes the always-embedded stubs (see
//! `cw_scene::text`) — so the only thing that changes when a pack file arrives is
//! that its glyphs stop drawing as `.notdef` boxes (or, for colour emoji, that the
//! monochrome glyphs turn into colour ones). Installed bytes must match the pinned
//! SHA-256 of the file this build was made with, so every installed pack is the same
//! pack and the same text renders the same pixels natively and in Wasm.
use cw_scene::text::FaceId;
use fontdue::{Font, FontSettings};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;

/// One file of the font pack: where it is served from, relative to the package
/// (`fonts/<file>`), and the SHA-256 its bytes must have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackFile {
    pub face: FaceId,
    pub file: &'static str,
    pub sha256: &'static str,
    pub bytes: usize,
}

const fn pack(face: FaceId, file: &'static str, sha256: &'static str, bytes: usize) -> PackFile {
    PackFile {
        face,
        file,
        sha256,
        bytes,
    }
}

/// One web face fetched on first use: the face table's slot (`faces::FAMILIES`
/// order, four styles each), its file under `fonts/`, and the SHA-256 and size its
/// bytes must have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WebFace {
    pub slot: usize,
    pub file: &'static str,
    pub sha256: &'static str,
    pub bytes: usize,
}
const fn web(slot: usize, file: &'static str, sha256: &'static str, bytes: usize) -> WebFace {
    WebFace {
        slot,
        file,
        sha256,
        bytes,
    }
}
/// The thirteen web families, four faces each (4,583,364 bytes).
pub const WEB_FACES: [WebFace; 52] = [
    web(
        16,
        "arimo-regular.ttf",
        "70dcd8763e2228dc7145ce884c72a8ffc7898a1db47acf73f46609e76280e242",
        126_968,
    ),
    web(
        17,
        "arimo-bold.ttf",
        "0683379ddad7590bd4763b72a0dfba9f89be717f108ffb1d5d62fde3d869f19b",
        127_920,
    ),
    web(
        18,
        "arimo-italic.ttf",
        "ad72095f9681f967a586d4890c2b6b3737cb8938e2613180971a8a227f6d55e6",
        137_640,
    ),
    web(
        19,
        "arimo-bold-italic.ttf",
        "cdbbe7dcc527edd7af0bf918ad710721ccf1c1b041ffc936040ed5d236d05560",
        138_544,
    ),
    web(
        20,
        "tinos-regular.ttf",
        "dcd3bab0226cac708b45f742cfdb97f3d4ac202627331a9251507aeee3449f8f",
        138_976,
    ),
    web(
        21,
        "tinos-bold.ttf",
        "d639db1d835936a49be831505b1e3bd9dcc7e3f1483cc4da98ad7c0179026c68",
        135_588,
    ),
    web(
        22,
        "tinos-italic.ttf",
        "6b263217478ad26ece3dfba570055dcf1dd1476eaeb511d3f65af39c255f0284",
        141_312,
    ),
    web(
        23,
        "tinos-bold-italic.ttf",
        "4d1adbf9fb465916bde8316ec935a7c1763dc684352d93c975ded49c83ed8fd1",
        139_672,
    ),
    web(
        24,
        "cousine-regular.ttf",
        "a1d0144bcfe6120a4b11e82eeab061b7a049cb5eebe4f044c226d1e68dcbfbfe",
        103_232,
    ),
    web(
        25,
        "cousine-bold.ttf",
        "2accafad403122e06bd8fa690e74656b9ca7c720ded9e9908e25db9731f846f1",
        103_476,
    ),
    web(
        26,
        "cousine-italic.ttf",
        "46ab107e120f5209f74959321cda0b7583e683556e8a68d4a01ce601abfef024",
        109_660,
    ),
    web(
        27,
        "cousine-bold-italic.ttf",
        "ae061225465ab112a0a1ffa75c60f9d1b5c70ffe353ad27ade3bc7962782e0f2",
        108_948,
    ),
    web(
        28,
        "gelasio-regular.ttf",
        "34fa275a3e930f262edc2f5987efa6c27916bdc2571d800665757b438c72ea16",
        62_464,
    ),
    web(
        29,
        "gelasio-bold.ttf",
        "f15a5a6ea36a83114c41cf56bd6bbfed25b94041afff3860c0ebe877449656b9",
        62_516,
    ),
    web(
        30,
        "gelasio-italic.ttf",
        "ab07e560003c02362c43deed83169ce6604a8be054bbab2dbfe2da7ca67d76d9",
        63_724,
    ),
    web(
        31,
        "gelasio-bold-italic.ttf",
        "01feed9aaaa9ebb1d70378398b1efce0b1e6efcefc9618ff04b5d448dffd6ab3",
        63_652,
    ),
    web(
        32,
        "carlito-regular.ttf",
        "c08912314e500f890270292bb37f5105bf526f45051bc9a7d9b2e516a5d79362",
        138_936,
    ),
    web(
        33,
        "carlito-bold.ttf",
        "238acea90008784c975c551cb3c9a209b682734e3e905dd145bcdb043793dcdf",
        137_752,
    ),
    web(
        34,
        "carlito-italic.ttf",
        "980295ff41c22839e913aa9efb7b40fbd1a94882df9c9d934dadd638b749907f",
        141_544,
    ),
    web(
        35,
        "carlito-bold-italic.ttf",
        "6d19b5a717134973dc813e3e585358c775c52e2fa18380bd0ae6fd703550c685",
        196_660,
    ),
    web(
        36,
        "caladea-regular.ttf",
        "73aed07063e394c26c4191734c1cdfe1635f3477ecca8c8846f900633042d2ed",
        36_228,
    ),
    web(
        37,
        "caladea-bold.ttf",
        "b37dbe406c9521be3e7a3ac8659e2122e9ee37e15db72f02c05bd55b12c6e5e6",
        36_728,
    ),
    web(
        38,
        "caladea-italic.ttf",
        "823f09701aa46cbd574454ee6f6a30385c50253f5c3dc60bc5294deffd9b11bd",
        35_976,
    ),
    web(
        39,
        "caladea-bold-italic.ttf",
        "f3ba5cbed8ff96ac97625d7e0caeeb7598ecd3157de1a44cdb3d12b1448796f9",
        35_756,
    ),
    web(
        40,
        "lato-regular.ttf",
        "941732d0e17b555e33f9419b79946b44c581570338c81beab630cd05da4cb7f3",
        142_740,
    ),
    web(
        41,
        "lato-bold.ttf",
        "2d242a248875a177bffdcbba190cd68fdd757c5593be41585d39a68748bf65c6",
        140_580,
    ),
    web(
        42,
        "lato-italic.ttf",
        "35316c7c4a3ec9c95d628a7dd501ae0c1732eee74deb99714ed47131a771acb3",
        158_940,
    ),
    web(
        43,
        "lato-bold-italic.ttf",
        "73b9950a14ce9a4cf51d13429abb0af0a35492d91057aa3a2031a5ba1ec64062",
        153_556,
    ),
    web(
        44,
        "sourcesans-regular.ttf",
        "6d298bf953417a0edf749d4a6c0f191a03db6f4c2d9386ff6e8b20c4f9385f3a",
        78_612,
    ),
    web(
        45,
        "sourcesans-bold.ttf",
        "b17d1e2432f971a0de6543dc5c9a2c01a642db1d9956a7de328a099bcae0c6f8",
        78_604,
    ),
    web(
        46,
        "sourcesans-italic.ttf",
        "cff86d85b26ca009c37424021ec1b8ff82951e8d52f485bc58becf7717029eec",
        82_408,
    ),
    web(
        47,
        "sourcesans-bold-italic.ttf",
        "6376317d7322559c48ff09399213c8a73b4e5fcb5b80fd47877983d0009e717e",
        82_300,
    ),
    web(
        48,
        "sourceserif-regular.ttf",
        "41b60f4994cb845f62c89e788bd60f7b222d546833679922b6ecf40b20b537ce",
        73_884,
    ),
    web(
        49,
        "sourceserif-bold.ttf",
        "863b70d1505f9e6d5cb99798edea121b256f8072fed71b4de6cb081f0a106a2d",
        73_988,
    ),
    web(
        50,
        "sourceserif-italic.ttf",
        "f7ffefde5e4b8c8ec971058606f6bfd371f47063a3233e186011c2b59d0d86d5",
        74_380,
    ),
    web(
        51,
        "sourceserif-bold-italic.ttf",
        "4dd017e45f5541a46b263272b19e37aa2ae8510a398ab78a21c1fd8ba5a46406",
        74_436,
    ),
    web(
        52,
        "poppins-regular.ttf",
        "66cee03e736e7f73cac6a33981f1eaaa714f1d68791addb7ab1760dd1e497039",
        25_136,
    ),
    web(
        53,
        "poppins-bold.ttf",
        "089129a08f51802c640afecd165143bef4777e565913f5e78c0cf0c7337b941d",
        24_728,
    ),
    web(
        54,
        "poppins-italic.ttf",
        "5a3c1942c729456c519ecc05722186aa3f8fc74a6e87f47243c0f8292ac32210",
        27_196,
    ),
    web(
        55,
        "poppins-bold-italic.ttf",
        "adab280043d64523e8b67bb031751c5f62aa2a88904a5aa269d26d37e8713d45",
        26_824,
    ),
    web(
        56,
        "montserrat-regular.ttf",
        "de8f30409a8aed69800710ec2ada6d222b8075acf401a6b873e85b075b7dec23",
        66_484,
    ),
    web(
        57,
        "montserrat-bold.ttf",
        "34e7655e96d030fe9b1097bd2006bdafa151f8b4ecdd82bcda30fe0b00457636",
        66_444,
    ),
    web(
        58,
        "montserrat-italic.ttf",
        "f7f9720116523918af28bb96599e075bb02dc0c0682be3920215c208252a8331",
        67_376,
    ),
    web(
        59,
        "montserrat-bold-italic.ttf",
        "b926921121f5f38c130a516b3de99d0863304cee3f0fe9f1d1fedd8d97344964",
        67_356,
    ),
    web(
        60,
        "playfair-regular.ttf",
        "0e913357adeb53e8a39376be7aae0e9dc255d70c1aeadb9d94cc73c6ff62f68e",
        58_860,
    ),
    web(
        61,
        "playfair-bold.ttf",
        "000eaa508636b6561557d5efafbb880a57aa7bc08c4e11b0e89b06656f2b4704",
        58_968,
    ),
    web(
        62,
        "playfair-italic.ttf",
        "5520209f2f20e65695070fed4541513b54be1bbf46239636c625412664a13832",
        61_444,
    ),
    web(
        63,
        "playfair-bold-italic.ttf",
        "5766ce86b2c384ee82c02e3ef846d586df827a366f37c4f3a6a1e04044c4a63a",
        61_520,
    ),
    web(
        64,
        "jetbrainsmono-regular.ttf",
        "446bbd98d41627d4237ca94ddf53c93f11f9f8cb415bbb292e400a640db51d2a",
        56_884,
    ),
    web(
        65,
        "jetbrainsmono-bold.ttf",
        "511558e6fb9502152395d0d5e121a0f5487b563d768f61570c8f78bff7496bad",
        56_840,
    ),
    web(
        66,
        "jetbrainsmono-italic.ttf",
        "db8d5e552dca9780d2f6fec87409a62847e3dcb197f640b6283c12d7d2c4534b",
        59_520,
    ),
    web(
        67,
        "jetbrainsmono-bold-italic.ttf",
        "45d3e13aa22236d17fe604e1baed57ce1ca4a171368f060e5f714ff6bea2f9e7",
        59_484,
    ),
];

pub const FONT_PACK: [PackFile; 20] = [
    pack(
        FaceId::Han,
        "noto-sans-sc.ttf",
        "34379072e545d67c4b22de7ccb2a60bdb02c82e8144eb705ab73356d1b65cec7",
        3_522_836,
    ),
    pack(
        FaceId::Hangul,
        "noto-sans-kr.ttf",
        "b7328d27e2cda3fd8c6198df10ba6a780102b7a5698ee67f6153005f26077c73",
        2_366_072,
    ),
    pack(
        FaceId::Emoji,
        "noto-emoji.ttf",
        "f2ca5cf2d5d68e2e98920db332ecb333a50c747d4bc2d151aa3357d2fe8b1ce8",
        862_788,
    ),
    pack(
        FaceId::HanBold,
        "noto-sans-sc-bold.ttf",
        "4d039b1401203d59c6b75da5e5d31b5b317a2651eb5f66d35d7347216dc42f4e",
        3_518_020,
    ),
    pack(
        FaceId::HangulBold,
        "noto-sans-kr-bold.ttf",
        "845529246ddc318926853f028279ebbcde1c6a72a8460d5ae11e2b34e6e9790e",
        2_365_908,
    ),
    pack(
        FaceId::HanTc,
        "noto-sans-tc.ttf",
        "ed345467119202ca3bde9eec4e11fb161a2ba480a2b23d30874f8e80c06e5f55",
        976_604,
    ),
    pack(
        FaceId::HanTcBold,
        "noto-sans-tc-bold.ttf",
        "ce6e458c0002c7233568a1f2510b31c6b46cb7008cb8de2d007e04fefe7ed6df",
        975_532,
    ),
    pack(
        FaceId::HanJp,
        "noto-sans-jp.ttf",
        "8fb8077988ce42be8590a7afd0a178fd173b06959244b2cfaa185ca893274b95",
        570_584,
    ),
    pack(
        FaceId::HanJpBold,
        "noto-sans-jp-bold.ttf",
        "87048d2bf6c4464e57de95aa7801016f72bf22ee767b8666875f2d08640db293",
        569_656,
    ),
    pack(
        FaceId::HanKr,
        "noto-sans-kr-han.ttf",
        "d31454e913aee276076e2b3473f24a2408ff9263c555369f80e6c448ad4d0def",
        980_500,
    ),
    pack(
        FaceId::HanKrBold,
        "noto-sans-kr-han-bold.ttf",
        "5fec17d1cea51433a462073554c7d6fd34016481327badb7f839ad8d049116d3",
        978_976,
    ),
    pack(
        FaceId::Tamil,
        "noto-tamil.ttf",
        "a7ec5158820dbb20b50224de63b3f409e0fc0eaa7c172f5964b9277ebcb27e2e",
        38_496,
    ),
    pack(
        FaceId::Gurmukhi,
        "noto-gurmukhi.ttf",
        "629fbe717f2546a74dc41f1f42c94ff667f1792231b29f16cedd1e479d2a6064",
        24_228,
    ),
    pack(
        FaceId::Lao,
        "noto-lao.ttf",
        "9ff6157c510f22389373aff217a7fe095aebffc1ec3e7055a5c2a1a368b8dcbb",
        20_704,
    ),
    pack(
        FaceId::Khmer,
        "noto-khmer.ttf",
        "67651a225bb763e44e64fb0ef91f74a06e5c7c183d946c1dc68287e770039145",
        70_744,
    ),
    pack(
        FaceId::Gujarati,
        "noto-gujarati.ttf",
        "17dab1ae66cb5ce757aa102d76acb0e19a2037aa9f0f98741d90648987fadad2",
        128_692,
    ),
    pack(
        FaceId::Ethiopic,
        "noto-ethiopic.ttf",
        "4974a303fb3575620db2ad0d1d343acd5de3bae52396998ef9b7fb4a56064600",
        76_924,
    ),
    pack(
        FaceId::Myanmar,
        "noto-myanmar.ttf",
        "0db45818c27e6d759341b65b3135e6949eb1ef3cbab57c85f7acad78fc81e776",
        127_552,
    ),
    pack(
        FaceId::Sinhala,
        "noto-sinhala.ttf",
        "1cff7973112e1c16d110b953aa7b12cdabf538374cce62b43ef2260e12385f93",
        196_940,
    ),
    pack(
        FaceId::ColorEmoji,
        "noto-color-emoji.ttf",
        "0ae57fe58645638523ba35f388d93739d292539a9acb84df5700c81b1e1a28d2",
        4_991_984,
    ),
];

fn slot(face: FaceId) -> Option<usize> {
    FONT_PACK.iter().position(|p| p.face == face)
}

// Only the native build embeds the pack; a Wasm bundle fetches it, so the macro is
// gated with the static it fills.
#[cfg(not(target_family = "wasm"))]
macro_rules! pack_bytes {
    ($($dir:literal $name:literal),* $(,)?) => {
        [$(include_bytes!(concat!(env!($dir), "/", $name)) as &[u8]),*]
    };
}
/// In statics rather than constants so each file is in the binary exactly once.
#[cfg(not(target_family = "wasm"))]
static EMBEDDED: [&[u8]; 20] = pack_bytes![
    "CW_FONT_PACK_SC_DIR" "noto-sans-sc.ttf",
    "CW_FONT_PACK_CJK_DIR" "noto-sans-kr.ttf",
    "CW_FONT_PACK_EMOJI_DIR" "noto-emoji.ttf",
    "CW_FONT_PACK_SC_DIR" "noto-sans-sc-bold.ttf",
    "CW_FONT_PACK_CJK_DIR" "noto-sans-kr-bold.ttf",
    "CW_FONT_PACK_CJK_DIR" "noto-sans-tc.ttf",
    "CW_FONT_PACK_CJK_DIR" "noto-sans-tc-bold.ttf",
    "CW_FONT_PACK_CJK_DIR" "noto-sans-jp.ttf",
    "CW_FONT_PACK_CJK_DIR" "noto-sans-jp-bold.ttf",
    "CW_FONT_PACK_CJK_DIR" "noto-sans-kr-han.ttf",
    "CW_FONT_PACK_CJK_DIR" "noto-sans-kr-han-bold.ttf",
    "CW_FONT_PACK_EMOJI_DIR" "noto-tamil.ttf",
    "CW_FONT_PACK_EMOJI_DIR" "noto-gurmukhi.ttf",
    "CW_FONT_PACK_EMOJI_DIR" "noto-lao.ttf",
    "CW_FONT_PACK_EMOJI_DIR" "noto-khmer.ttf",
    "CW_FONT_PACK_EMOJI_DIR" "noto-gujarati.ttf",
    "CW_FONT_PACK_EMOJI_DIR" "noto-ethiopic.ttf",
    "CW_FONT_PACK_EMOJI_DIR" "noto-myanmar.ttf",
    "CW_FONT_PACK_EMOJI_DIR" "noto-sinhala.ttf",
    "CW_FONT_PACK_EMOJI_DIR" "noto-color-emoji.ttf",
];
#[cfg(not(target_family = "wasm"))]
fn embedded(face: FaceId) -> Option<&'static [u8]> {
    slot(face).map(|i| EMBEDDED[i])
}
#[cfg(target_family = "wasm")]
fn embedded(_: FaceId) -> Option<&'static [u8]> {
    None
}

/// The four platforms' UI families (Inter, Open Sans, Ubuntu, Roboto), four faces each,
/// in face-table slots 0-15. A lazy build (`lazy-fonts`) carries the families of the
/// platforms it draws and fetches the others on first use, as the web families are.
pub const UI_FACES: [WebFace; 16] = [
    web(
        0,
        "inter-regular.ttf",
        "cb40a8531097553c63086c2b7f08ccc22ff5db6fa6955570364908d9c37217a1",
        26_232,
    ),
    web(
        1,
        "inter-bold.ttf",
        "bd9b73a81a94f90b51a82e3f9b91102527000a587a1cafc2111df88fc22c10db",
        26_352,
    ),
    web(
        2,
        "inter-italic.ttf",
        "96ac87e280c326af66fe03c82dd4fe961d463fb40b4db194dbe1de29567b91e7",
        27_216,
    ),
    web(
        3,
        "inter-bold-italic.ttf",
        "ad7965a4eaf43e1d802c733fb0c7b663414202ffc1571b49f3b19dbc2dac6c0c",
        27_328,
    ),
    web(
        4,
        "opensans-regular.ttf",
        "0bc480b64fdd30e0854b9120591689006b5880848615daab4cbb6190f34e79ae",
        24_028,
    ),
    web(
        5,
        "opensans-bold.ttf",
        "e6a292f28585e91f6a775e9d7be72d9d6113f219b7613d74198aeabc363d5630",
        24_044,
    ),
    web(
        6,
        "opensans-italic.ttf",
        "2b59ea06edd7620fecf3c8937203736b8ebb8fa86e7629579e7a2d1cd57def2b",
        24_948,
    ),
    web(
        7,
        "opensans-bold-italic.ttf",
        "df19db481468a87bd06cb8e95876a97214b6b342139eca09ac816c43b3144a74",
        24_936,
    ),
    web(
        8,
        "ubuntu-regular.ttf",
        "f6cbfc26a928daac1f8c5e9a82d84b0aa15610e028dfca4270cd011ac3fff8cc",
        25_828,
    ),
    web(
        9,
        "ubuntu-bold.ttf",
        "de9bd0a075d06b8720e258c757b1566d577ac4f86e3dbaec081cc31f8bcb7cfb",
        25_864,
    ),
    web(
        10,
        "ubuntu-italic.ttf",
        "f2d09dc1ed119b5ba6cf64fbd9588a6f05ead6b2c896f26d972f0d9f16889c8f",
        25_920,
    ),
    web(
        11,
        "ubuntu-bold-italic.ttf",
        "db7ee5cdbab34727a38056bf14215f99972adb36e2f82c2bcba338004138f64b",
        25_904,
    ),
    web(
        12,
        "roboto-regular.ttf",
        "3996bccd994268a81b69686f4789344f39c6f4367b95e311c2d4b7d0149d361e",
        25_624,
    ),
    web(
        13,
        "roboto-bold.ttf",
        "f863df393060eeb9af94df1fda8fc386390ec44bef12cf226514856b11a61378",
        25_640,
    ),
    web(
        14,
        "roboto-italic.ttf",
        "6fa5fd965b3476c1cd672f5b2b0be304e374d97025e9fd93c5ff5dc210a3acf4",
        27_196,
    ),
    web(
        15,
        "roboto-bold-italic.ttf",
        "52e1620a2ab37c4ad8ae8898cbaa721956ab20daa02de2c87713201d0239abdc",
        27_196,
    ),
];

static INSTALLED: [OnceLock<&'static [u8]>; 20] = [const { OnceLock::new() }; 20];
static WEB_INSTALLED: [OnceLock<&'static [u8]>; 52] = [const { OnceLock::new() }; 52];
static UI_INSTALLED: [OnceLock<&'static [u8]>; 16] = [const { OnceLock::new() }; 16];

/// Fetches one file of this build's on-demand fonts by name and SHA-256, and waits
/// for it: the host's side of [`set_font_loader`]. `None` when it cannot.
pub type FontLoader = fn(file: &str, sha256: &str) -> Option<Vec<u8>>;
static LOADER: std::sync::RwLock<Option<FontLoader>> = std::sync::RwLock::new(None);
/// Files a frame needed and could not have: the frame was refused, not drawn.
static MISSING: std::sync::Mutex<std::collections::BTreeSet<&'static str>> =
    std::sync::Mutex::new(std::collections::BTreeSet::new());

/// Install the host's font loader (or remove it with `None`). With one, a web face or
/// a pack file is fetched the first time a glyph needs it, before the glyph is drawn.
pub fn set_font_loader(loader: Option<FontLoader>) {
    *LOADER.write().unwrap_or_else(|e| e.into_inner()) = loader;
}
pub(crate) fn loader() -> Option<FontLoader> {
    let set = *LOADER.read().unwrap_or_else(|e| e.into_inner());
    // Tests of a build that fetches the web faces read them from the assets.
    #[cfg(test)]
    if set.is_none() {
        return Some(tests::assets);
    }
    set
}
/// Ask the loader for `file`; keep it only when its bytes are the pinned ones.
fn fetch(
    file: &'static str,
    sha256: &'static str,
    into: &OnceLock<&'static [u8]>,
) -> Option<&'static [u8]> {
    fetch_with(loader(), file, sha256, into)
}
/// Ask `loader` for `file`; keep it only when its bytes are the pinned ones.
pub(crate) fn fetch_with(
    loader: Option<FontLoader>,
    file: &str,
    sha256: &str,
    into: &OnceLock<&'static [u8]>,
) -> Option<&'static [u8]> {
    let bytes = loader?(file, sha256)?;
    if format!("{:x}", Sha256::digest(&bytes)) != sha256 {
        return None;
    }
    let _ = into.set(Box::leak(bytes.into_boxed_slice()));
    GENERATION.fetch_add(1, Ordering::Relaxed);
    into.get().copied()
}
pub(crate) fn required(file: &'static str) {
    MISSING
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(file);
}
/// The files frames drawn since the last call needed and could not have, emptied.
/// Non-empty means a frame was not drawn as it must be and is to be thrown away.
pub fn take_missing_fonts() -> Vec<&'static str> {
    std::mem::take(&mut *MISSING.lock().unwrap_or_else(|e| e.into_inner()))
        .into_iter()
        .collect()
}

/// A face this build fetches for a face-table slot (a web face, or a UI face it does
/// not carry), and where its bytes are kept once here.
fn fetched(slot: usize) -> Option<(&'static WebFace, &'static OnceLock<&'static [u8]>)> {
    if let Some(i) = WEB_FACES.iter().position(|w| w.slot == slot) {
        return Some((&WEB_FACES[i], &WEB_INSTALLED[i]));
    }
    let i = UI_FACES.iter().position(|w| w.slot == slot)?;
    crate::faces::FACE_BYTES[slot]
        .is_none()
        .then(|| (&UI_FACES[i], &UI_INSTALLED[i]))
}
/// The UI faces this build does not carry, which it fetches on first use.
pub fn fetched_ui_faces() -> impl Iterator<Item = &'static WebFace> {
    UI_FACES
        .iter()
        .filter(|w| crate::faces::FACE_BYTES[w.slot].is_none())
}
/// The fetched face for a face-table slot, if the slot is one.
pub(crate) fn web_face(slot: usize) -> Option<&'static WebFace> {
    fetched(slot).map(|(face, _)| face)
}
/// A fetched face's bytes: installed, or fetched now. `None` records the file as missing.
pub(crate) fn web_face_bytes(slot: usize) -> Option<&'static [u8]> {
    let (face, held) = fetched(slot)?;
    #[cfg(test)]
    if HIDE_WEB.with(|h| h.get()) {
        required(face.file);
        return None;
    }
    if let Some(bytes) = held.get() {
        return Some(bytes);
    }
    let fetched = fetch(face.file, face.sha256, held);
    if fetched.is_none() {
        required(face.file);
    }
    fetched
}
/// Bit per pack slot: a renderer needed that face and did not have it.
static WANTED: AtomicU32 = AtomicU32::new(0);
static GENERATION: AtomicU32 = AtomicU32::new(0);

#[cfg(test)]
thread_local! {
    /// Lets tests exercise the Wasm "pack not yet fetched" path natively.
    pub(crate) static HIDE_PACK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Hides only the colour emoji file, to exercise the monochrome fallback.
    pub(crate) static HIDE_COLOR: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Makes every web face unfetchable, embedded or not.
    pub(crate) static HIDE_WEB: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Outline bytes for a face, if this process has them.
pub(crate) fn face_bytes(face: FaceId) -> Option<&'static [u8]> {
    if let Some(bytes) = face.embedded_bytes() {
        return Some(bytes);
    }
    #[cfg(test)]
    if HIDE_PACK.with(|h| h.get()) || (face == FaceId::ColorEmoji && HIDE_COLOR.with(|h| h.get())) {
        return None;
    }
    let i = slot(face)?;
    embedded(face)
        .or_else(|| INSTALLED[i].get().copied())
        .or_else(|| {
            // With a loader the pack file is fetched now and never drawn as boxes; a
            // loader that cannot fetch it refuses the frame. Without one (a page that
            // installs the pack itself) the glyphs draw as boxes until it arrives.
            loader()?;
            let file = &FONT_PACK[i];
            let fetched = fetch(file.file, file.sha256, &INSTALLED[i]);
            if fetched.is_none() {
                required(file.file);
            }
            fetched
        })
}

/// Parsed faces are immutable and shared by every renderer in the process.
pub(crate) fn face_font(face: FaceId) -> Option<&'static Font> {
    static FONTS: [OnceLock<Font>; FaceId::COUNT] = [const { OnceLock::new() }; FaceId::COUNT];
    let bytes = face_bytes(face)?;
    Some(FONTS[face as usize].get_or_init(|| {
        Font::from_bytes(bytes, FontSettings::default()).expect("bundled fallback face is valid")
    }))
}

/// The colour emoji face, parsed for shaping and for its colour tables.
pub(crate) fn color_emoji() -> Option<&'static rustybuzz::Face<'static>> {
    static FACE: OnceLock<rustybuzz::Face<'static>> = OnceLock::new();
    let bytes = face_bytes(FaceId::ColorEmoji)?;
    Some(FACE.get_or_init(|| {
        rustybuzz::Face::from_slice(bytes, 0).expect("bundled colour emoji face is valid")
    }))
}

pub(crate) fn note_missing(face: FaceId) {
    if let Some(i) = slot(face) {
        WANTED.fetch_or(1 << i, Ordering::Relaxed);
    }
}

/// Changes whenever a pack file is installed; renderers drop cached text then.
pub(crate) fn generation() -> u32 {
    GENERATION.load(Ordering::Relaxed)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontPackError {
    /// The bytes are not any file of this build's pack.
    Unknown { sha256: String },
}
impl std::fmt::Display for FontPackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown { sha256 } => write!(
                f,
                "font data with SHA-256 {sha256} is not a file of this build's font pack"
            ),
        }
    }
}
impl std::error::Error for FontPackError {}

/// Install one on-demand font file (a pack file or a web face), identified by its
/// SHA-256; returns its file name. Installing a file twice, or installing into a
/// build that already embeds it, is a harmless no-op.
pub fn install_font(bytes: &[u8]) -> Result<&'static str, FontPackError> {
    let sha256 = format!("{:x}", Sha256::digest(bytes));
    let leak = || -> &'static [u8] { Box::leak(bytes.to_vec().into_boxed_slice()) };
    if let Some(i) = FONT_PACK.iter().position(|p| p.sha256 == sha256) {
        if embedded(FONT_PACK[i].face).is_none()
            && INSTALLED[i].get().is_none()
            && INSTALLED[i].set(leak()).is_ok()
        {
            GENERATION.fetch_add(1, Ordering::Relaxed);
        }
        return Ok(FONT_PACK[i].file);
    }
    if let Some(i) = WEB_FACES.iter().position(|w| w.sha256 == sha256) {
        if crate::faces::FACE_BYTES[WEB_FACES[i].slot].is_none()
            && WEB_INSTALLED[i].get().is_none()
            && WEB_INSTALLED[i].set(leak()).is_ok()
        {
            GENERATION.fetch_add(1, Ordering::Relaxed);
        }
        return Ok(WEB_FACES[i].file);
    }
    if let Some(i) = UI_FACES.iter().position(|w| w.sha256 == sha256) {
        if crate::faces::FACE_BYTES[UI_FACES[i].slot].is_none()
            && UI_INSTALLED[i].get().is_none()
            && UI_INSTALLED[i].set(leak()).is_ok()
        {
            GENERATION.fetch_add(1, Ordering::Relaxed);
        }
        return Ok(UI_FACES[i].file);
    }
    Err(FontPackError::Unknown { sha256 })
}

/// Which on-demand files this process can draw, and which ones renderers have
/// needed but not had (so a page can fetch exactly those).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontPackStatus {
    pub installed: Vec<&'static str>,
    pub missing: Vec<&'static str>,
}
pub fn font_pack_status() -> FontPackStatus {
    let wanted = WANTED.load(Ordering::Relaxed);
    let mut status = FontPackStatus {
        installed: Vec::new(),
        missing: Vec::new(),
    };
    let here = |i: usize, face: FaceId| {
        #[cfg(test)]
        if HIDE_PACK.with(|h| h.get())
            || (face == FaceId::ColorEmoji && HIDE_COLOR.with(|h| h.get()))
        {
            return false;
        }
        embedded(face).is_some() || INSTALLED[i].get().is_some()
    };
    for (i, file) in FONT_PACK.iter().enumerate() {
        if here(i, file.face) {
            status.installed.push(file.file);
        } else if wanted & (1 << i) != 0 {
            status.missing.push(file.file);
        }
    }
    for (i, face) in WEB_FACES.iter().enumerate() {
        if crate::faces::FACE_BYTES[face.slot].is_some() || WEB_INSTALLED[i].get().is_some() {
            status.installed.push(face.file);
        }
    }
    for (i, face) in UI_FACES.iter().enumerate() {
        if crate::faces::FACE_BYTES[face.slot].is_none() && UI_INSTALLED[i].get().is_some() {
            status.installed.push(face.file);
        }
    }
    let missing = MISSING.lock().unwrap_or_else(|e| e.into_inner());
    for file in missing.iter() {
        if !status.missing.contains(file) {
            status.missing.push(file);
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;
    /// The loader tests use: the files in the assets folder.
    pub(super) fn assets(file: &str, _sha256: &str) -> Option<Vec<u8>> {
        [env!("CW_FONTS_DIR"), env!("CW_FONT_PACK_EMOJI_DIR"), env!("CW_FONT_PACK_SC_DIR"), env!("CW_FONT_PACK_CJK_DIR")]
            .iter()
            .find_map(|dir| std::fs::read(format!("{dir}/{file}")).ok())
    }
    #[test]
    fn ui_faces_match_their_pinned_hashes() {
        for (slot, face) in UI_FACES.iter().enumerate() {
            assert_eq!(face.slot, slot, "{}", face.file);
            let bytes = assets(face.file, face.sha256).unwrap();
            assert_eq!(bytes.len(), face.bytes, "{}", face.file);
            assert_eq!(
                format!("{:x}", Sha256::digest(&bytes)),
                face.sha256,
                "{}",
                face.file
            );
            // Embedded, or (another platform's family, or any in a `lazy-render` build)
            // fetched on first use.
            assert_eq!(
                crate::faces::face_bytes(slot),
                Some(&bytes[..]),
                "{}",
                face.file
            );
        }
    }
    #[test]
    fn pack_files_match_their_pinned_hashes() {
        assert_eq!(FONT_PACK.len(), FaceId::PACK.len());
        for (file, face) in FONT_PACK.iter().zip(FaceId::PACK) {
            assert_eq!(file.face, face);
            let bytes = embedded(file.face).unwrap();
            assert_eq!(bytes.len(), file.bytes, "{}", file.file);
            assert_eq!(format!("{:x}", Sha256::digest(bytes)), file.sha256);
            assert_eq!(file.face.file_name(), file.file);
            assert_eq!(install_font(bytes).unwrap(), file.file);
        }
        assert!(matches!(
            install_font(b"not a font"),
            Err(FontPackError::Unknown { .. })
        ));
        // Fetched UI faces count as installed once another test has drawn with them.
        let installed = font_pack_status().installed;
        let ui = installed
            .iter()
            .filter(|f| UI_FACES.iter().any(|u| u.file == **f))
            .count();
        assert_eq!(installed.len() - ui, FONT_PACK.len() + WEB_FACES.len());
    }
    /// A web face that cannot be fetched draws nothing and names itself, so the frame
    /// that needed it is refused rather than drawn with another face.
    #[test]
    fn a_web_face_that_cannot_be_fetched_is_named_missing() {
        take_missing_fonts();
        HIDE_WEB.with(|h| h.set(true));
        let bytes = crate::faces::face_bytes(16);
        HIDE_WEB.with(|h| h.set(false));
        assert!(bytes.is_none());
        assert!(take_missing_fonts().contains(&"arimo-regular.ttf"));
        assert!(crate::faces::face_bytes(16).is_some());
    }
}
