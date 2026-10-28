//! The files the renderer draws with and nothing else reads: DejaVu (the shells' and
//! the terminal's faces), the icons and wallpapers of the platforms this build draws,
//! and the symbol masks. Layout never reads them: it measures with the advance and
//! kerning tables in `cw_scene`, and the scene names an icon by id. So a world driven
//! through its scene alone needs none of them.
//!
//! A build with `lazy-render` carries none of them. The first time a frame needs one,
//! the renderer asks the host for it ([`set_asset_loader`] for images, the font loader
//! for faces) and waits; bytes that do not hash to the pin below are refused. A frame
//! that needs a file the host cannot give is refused (`take_missing_fonts` names it),
//! never drawn without it.
use std::sync::OnceLock;

/// One file the renderer draws with: its path under `assets/` (a face under `fonts/`),
/// the SHA-256 and size its bytes must have, and its bytes when the build embeds them.
#[derive(Clone, Copy, Debug)]
pub struct RenderFile {
    pub file: &'static str,
    pub sha256: &'static str,
    pub bytes: usize,
    embedded: Option<&'static [u8]>,
}
impl RenderFile {
    /// A face (served under `fonts/`), not an image (served under `assets/`).
    pub fn is_font(&self) -> bool {
        self.file.starts_with("fonts/")
    }
    /// Where a host serves it: `fonts/<face>` or `assets/<image>`.
    pub fn path(&self) -> String {
        if self.is_font() {
            self.file.to_owned()
        } else {
            format!("assets/{}", self.file)
        }
    }
}

include!(concat!(env!("OUT_DIR"), "/render_files.rs"));

static HELD: [OnceLock<&'static [u8]>; RENDER_FILES.len()] =
    [const { OnceLock::new() }; RENDER_FILES.len()];

/// The bytes of a render file (`fonts/dejavu-sans.ttf`, `icons/ubuntu-files.png` …):
/// embedded, held, or fetched now. `None` records it as missing, so the frame is refused.
pub(crate) fn get(file: &str) -> Option<&'static [u8]> {
    let i = RENDER_FILES
        .binary_search_by(|f| f.file.cmp(file))
        .ok()?;
    let entry = &RENDER_FILES[i];
    if let Some(bytes) = entry.embedded {
        return Some(bytes);
    }
    if let Some(bytes) = HELD[i].get() {
        return Some(bytes);
    }
    let fetched = match entry.file.strip_prefix("fonts/") {
        Some(face) => crate::font_pack::fetch_with(crate::font_pack::loader(), face, entry.sha256, &HELD[i]),
        None => crate::font_pack::fetch_with(asset_loader(), entry.file, entry.sha256, &HELD[i]),
    };
    if fetched.is_none() {
        crate::font_pack::required(entry.file);
    }
    fetched
}

/// Whether this build fetches the files the renderer draws with.
pub fn fetches_render_files() -> bool {
    RENDER_FILES.iter().any(|f| f.embedded.is_none())
}

static ASSET_LOADER: std::sync::RwLock<Option<crate::FontLoader>> = std::sync::RwLock::new(None);
/// Install the host's image loader (or remove it with `None`): given a path under
/// `assets/` and its SHA-256, it returns the file's bytes and waits for them.
pub fn set_asset_loader(loader: Option<crate::FontLoader>) {
    *ASSET_LOADER.write().unwrap_or_else(|e| e.into_inner()) = loader;
}
fn asset_loader() -> Option<crate::FontLoader> {
    let set = *ASSET_LOADER.read().unwrap_or_else(|e| e.into_inner());
    #[cfg(test)]
    if set.is_none() {
        return Some(tests::read);
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn read(file: &str, _sha256: &str) -> Option<Vec<u8>> {
        std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join(file)).ok()
    }

    #[test]
    fn every_render_file_is_pinned_and_resolves() {
        assert!(RENDER_FILES.windows(2).all(|w| w[0].file < w[1].file), "sorted for lookup");
        for entry in RENDER_FILES.iter() {
            let bytes = get(entry.file).unwrap_or_else(|| panic!("{} does not resolve", entry.file));
            assert_eq!(bytes.len(), entry.bytes, "{}", entry.file);
        }
    }
}
