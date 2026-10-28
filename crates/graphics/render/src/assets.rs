//! Embedded resources: deterministic image decode, no host access.
//!
//! Icons and symbol masks are PNG. Wallpapers — the only photographic assets —
//! are baseline JPEG, which stores them at about a fifth of the PNG size that
//! `gzip` could not compress further. Both decoders are integer-only on every
//! target: `png` always is, and `jpeg-decoder` is pinned to its
//! `platform_independent` feature, which compiles out the SSSE3 / NEON /
//! `simd128` IDCT and colour-convert paths that would otherwise make an x86
//! host disagree with Wasm. `wallpapers_decode_to_pinned_pixels` below is the
//! tripwire: it hashes the decoded pixels, so any decoder or target that
//! diverges fails loudly rather than silently shifting the rendered frame.
use super::Frame;
pub use crate::symbols::SYMBOLS;
use std::sync::{Arc, OnceLock};
pub const ASSET_IDS: &[&str] = &[
    "wallpaper/macos",
    "wallpaper/windows",
    "wallpaper/ubuntu",
    "wallpaper/ios",
    "wallpaper/android",
    "icon/macos/files",
    "icon/macos/browser",
    "icon/macos/terminal",
    "icon/macos/docs",
    "icon/macos/mail",
    "icon/macos/calendar",
    "icon/macos/chat",
    "icon/macos/settings",
    "icon/macos/camera",
    "icon/macos/photos",
    "icon/macos/phone",
    "icon/macos/store",
    "icon/macos/launcher",
    "icon/macos/trash",
    "icon/macos/notes",
    "icon/macos/contacts",
    "icon/macos/clock",
    "icon/macos/calculator",
    "icon/macos/music",
    "icon/macos/maps",
    "icon/macos/weather",
    "icon/macos/code",
    "icon/macos/preview",
    "icon/macos/pixelmator",
    "icon/macos/imovie",
    "icon/macos/freecad",
    "icon/macos/spreadsheet",
    "icon/macos/excel",
    "icon/macos/database",
    "icon/macos/kicad",
    "icon/windows/files",
    "icon/windows/browser",
    "icon/windows/terminal",
    "icon/windows/docs",
    "icon/windows/mail",
    "icon/windows/calendar",
    "icon/windows/chat",
    "icon/windows/settings",
    "icon/windows/camera",
    "icon/windows/photos",
    "icon/windows/phone",
    "icon/windows/store",
    "icon/windows/launcher",
    "icon/windows/trash",
    "icon/windows/notes",
    "icon/windows/contacts",
    "icon/windows/clock",
    "icon/windows/calculator",
    "icon/windows/music",
    "icon/windows/maps",
    "icon/windows/weather",
    "icon/windows/code",
    "icon/windows/paint",
    "icon/windows/clipchamp",
    "icon/windows/freecad",
    "icon/windows/spreadsheet",
    "icon/windows/database",
    "icon/windows/kicad",
    "icon/ubuntu/files",
    "icon/ubuntu/browser",
    "icon/ubuntu/terminal",
    "icon/ubuntu/docs",
    "icon/ubuntu/mail",
    "icon/ubuntu/calendar",
    "icon/ubuntu/chat",
    "icon/ubuntu/settings",
    "icon/ubuntu/camera",
    "icon/ubuntu/photos",
    "icon/ubuntu/phone",
    "icon/ubuntu/store",
    "icon/ubuntu/launcher",
    "icon/ubuntu/trash",
    "icon/ubuntu/notes",
    "icon/ubuntu/contacts",
    "icon/ubuntu/clock",
    "icon/ubuntu/calculator",
    "icon/ubuntu/music",
    "icon/ubuntu/maps",
    "icon/ubuntu/weather",
    "icon/ubuntu/code",
    "icon/ubuntu/gimp",
    "icon/ubuntu/kdenlive",
    "icon/ubuntu/pinta",
    "icon/ubuntu/freecad",
    "icon/ubuntu/spreadsheet",
    "icon/ubuntu/database",
    "icon/ubuntu/kicad",
    "icon/ios/files",
    "icon/ios/browser",
    "icon/ios/terminal",
    "icon/ios/docs",
    "icon/ios/mail",
    "icon/ios/calendar",
    "icon/ios/chat",
    "icon/ios/settings",
    "icon/ios/camera",
    "icon/ios/photos",
    "icon/ios/phone",
    "icon/ios/store",
    "icon/ios/launcher",
    "icon/ios/trash",
    "icon/ios/notes",
    "icon/ios/contacts",
    "icon/ios/clock",
    "icon/ios/calculator",
    "icon/ios/music",
    "icon/ios/maps",
    "icon/ios/weather",
    "icon/ios/code",
    "icon/ios/spreadsheet",
    "icon/ios/imovie",
    "icon/android/files",
    "icon/android/browser",
    "icon/android/terminal",
    "icon/android/docs",
    "icon/android/mail",
    "icon/android/calendar",
    "icon/android/chat",
    "icon/android/settings",
    "icon/android/camera",
    "icon/android/photos",
    "icon/android/phone",
    "icon/android/store",
    "icon/android/launcher",
    "icon/android/trash",
    "icon/android/notes",
    "icon/android/contacts",
    "icon/android/clock",
    "icon/android/calculator",
    "icon/android/music",
    "icon/android/maps",
    "icon/android/weather",
    "icon/android/code",
    "icon/android/sketchbook",
    "icon/android/videoeditor",
    "icon/android/spreadsheet",
];
fn bytes(id: &str) -> Option<&'static [u8]> {
    Some(match id {
        #[cfg(feature = "os-macos")]
        "wallpaper/macos" => crate::render_pack::get("wallpapers/macos.jpg")?,
        #[cfg(feature = "os-macos")]
        "icon/macos/files" | "icon/files" => crate::render_pack::get("icons/macos-files.png")?,
        #[cfg(feature = "os-macos")]
        "icon/macos/browser" | "icon/browser" => {
            crate::render_pack::get("icons/macos-browser.png")?
        }
        #[cfg(feature = "os-macos")]
        "icon/macos/terminal" | "icon/terminal" => {
            crate::render_pack::get("icons/macos-terminal.png")?
        }
        #[cfg(feature = "os-macos")]
        "icon/macos/docs" | "icon/macos/editor" | "icon/docs" => {
            crate::render_pack::get("icons/macos-docs.png")?
        }
        #[cfg(feature = "os-macos")]
        "icon/macos/mail" | "icon/mail" => crate::render_pack::get("icons/macos-mail.png")?,
        #[cfg(feature = "os-macos")]
        "icon/macos/calendar" | "icon/calendar" => {
            crate::render_pack::get("icons/macos-calendar.png")?
        }
        #[cfg(feature = "os-macos")]
        "icon/macos/chat" | "icon/macos/messages" | "icon/chat" => {
            crate::render_pack::get("icons/macos-chat.png")?
        }
        #[cfg(feature = "os-macos")]
        "icon/macos/settings" | "icon/settings" => {
            crate::render_pack::get("icons/macos-settings.png")?
        }
        #[cfg(feature = "os-macos")]
        "icon/macos/camera" | "icon/camera" => crate::render_pack::get("icons/macos-camera.png")?,
        #[cfg(feature = "os-macos")]
        "icon/macos/photos" | "icon/photos" => crate::render_pack::get("icons/macos-photos.png")?,
        #[cfg(feature = "os-macos")]
        "icon/macos/phone" | "icon/phone" => crate::render_pack::get("icons/macos-phone.png")?,
        #[cfg(feature = "os-macos")]
        "icon/macos/store" | "icon/store" => crate::render_pack::get("icons/macos-store.png")?,
        #[cfg(feature = "os-macos")]
        "icon/macos/launcher" | "icon/launcher" => {
            crate::render_pack::get("icons/macos-launcher.png")?
        }
        #[cfg(feature = "os-macos")]
        "icon/macos/trash" | "icon/trash" => crate::render_pack::get("icons/macos-trash.png")?,
        #[cfg(feature = "os-macos")]
        "icon/macos/notes" | "icon/macos/notepad" | "icon/notes" => {
            crate::render_pack::get("icons/macos-notes.png")?
        }
        #[cfg(feature = "os-macos")]
        "icon/macos/contacts" | "icon/macos/addressbook" | "icon/contacts" => {
            crate::render_pack::get("icons/macos-contacts.png")?
        }
        #[cfg(feature = "os-macos")]
        "icon/macos/clock" | "icon/macos/clocks" | "icon/clock" => {
            crate::render_pack::get("icons/macos-clock.png")?
        }
        #[cfg(feature = "os-macos")]
        "icon/macos/calculator" | "icon/macos/calc" | "icon/calculator" => {
            crate::render_pack::get("icons/macos-calculator.png")?
        }
        #[cfg(feature = "os-macos")]
        "icon/macos/music" | "icon/music" => crate::render_pack::get("icons/macos-music.png")?,
        #[cfg(feature = "os-macos")]
        "icon/macos/maps" | "icon/maps" => crate::render_pack::get("icons/macos-maps.png")?,
        #[cfg(feature = "os-macos")]
        "icon/macos/weather" | "icon/weather" => {
            crate::render_pack::get("icons/macos-weather.png")?
        }
        #[cfg(feature = "os-macos")]
        "icon/macos/code" | "icon/code" => crate::render_pack::get("icons/macos-code.png")?,
        #[cfg(feature = "os-macos")]
        "icon/macos/preview" => crate::render_pack::get("icons/macos-preview.png")?,
        #[cfg(feature = "os-macos")]
        "icon/macos/pixelmator" => crate::render_pack::get("icons/macos-pixelmator.png")?,
        #[cfg(feature = "os-macos")]
        "icon/macos/imovie" => crate::render_pack::get("icons/macos-imovie.png")?,
        #[cfg(feature = "os-macos")]
        "icon/macos/freecad" => crate::render_pack::get("icons/macos-freecad.png")?,
        #[cfg(feature = "os-macos")]
        "icon/macos/spreadsheet" => crate::render_pack::get("icons/macos-spreadsheet.png")?,
        #[cfg(feature = "os-macos")]
        "icon/macos/excel" => crate::render_pack::get("icons/macos-excel.png")?,
        #[cfg(feature = "os-macos")]
        "icon/macos/database" => crate::render_pack::get("icons/macos-database.png")?,
        #[cfg(feature = "os-macos")]
        "icon/macos/kicad" => crate::render_pack::get("icons/macos-kicad.png")?,
        #[cfg(feature = "os-windows")]
        "wallpaper/windows" => crate::render_pack::get("wallpapers/windows.jpg")?,
        #[cfg(feature = "os-windows")]
        "icon/windows/files" => crate::render_pack::get("icons/windows-files.png")?,
        #[cfg(feature = "os-windows")]
        "icon/windows/browser" => crate::render_pack::get("icons/windows-browser.png")?,
        #[cfg(feature = "os-windows")]
        "icon/windows/terminal" => crate::render_pack::get("icons/windows-terminal.png")?,
        #[cfg(feature = "os-windows")]
        "icon/windows/docs" | "icon/windows/editor" => {
            crate::render_pack::get("icons/windows-docs.png")?
        }
        #[cfg(feature = "os-windows")]
        "icon/windows/mail" => crate::render_pack::get("icons/windows-mail.png")?,
        #[cfg(feature = "os-windows")]
        "icon/windows/calendar" => crate::render_pack::get("icons/windows-calendar.png")?,
        #[cfg(feature = "os-windows")]
        "icon/windows/chat" | "icon/windows/messages" => {
            crate::render_pack::get("icons/windows-chat.png")?
        }
        #[cfg(feature = "os-windows")]
        "icon/windows/settings" => crate::render_pack::get("icons/windows-settings.png")?,
        #[cfg(feature = "os-windows")]
        "icon/windows/camera" => crate::render_pack::get("icons/windows-camera.png")?,
        #[cfg(feature = "os-windows")]
        "icon/windows/photos" => crate::render_pack::get("icons/windows-photos.png")?,
        #[cfg(feature = "os-windows")]
        "icon/windows/phone" => crate::render_pack::get("icons/windows-phone.png")?,
        #[cfg(feature = "os-windows")]
        "icon/windows/store" => crate::render_pack::get("icons/windows-store.png")?,
        #[cfg(feature = "os-windows")]
        "icon/windows/launcher" => crate::render_pack::get("icons/windows-launcher.png")?,
        #[cfg(feature = "os-windows")]
        "icon/windows/trash" => crate::render_pack::get("icons/windows-trash.png")?,
        #[cfg(feature = "os-windows")]
        "icon/windows/notes" | "icon/windows/notepad" => {
            crate::render_pack::get("icons/windows-notes.png")?
        }
        #[cfg(feature = "os-windows")]
        "icon/windows/contacts" | "icon/windows/addressbook" => {
            crate::render_pack::get("icons/windows-contacts.png")?
        }
        #[cfg(feature = "os-windows")]
        "icon/windows/clock" | "icon/windows/clocks" => {
            crate::render_pack::get("icons/windows-clock.png")?
        }
        #[cfg(feature = "os-windows")]
        "icon/windows/calculator" | "icon/windows/calc" => {
            crate::render_pack::get("icons/windows-calculator.png")?
        }
        #[cfg(feature = "os-windows")]
        "icon/windows/music" => crate::render_pack::get("icons/windows-music.png")?,
        #[cfg(feature = "os-windows")]
        "icon/windows/maps" => crate::render_pack::get("icons/windows-maps.png")?,
        #[cfg(feature = "os-windows")]
        "icon/windows/weather" => crate::render_pack::get("icons/windows-weather.png")?,
        #[cfg(feature = "os-windows")]
        "icon/windows/code" => crate::render_pack::get("icons/windows-code.png")?,
        #[cfg(feature = "os-windows")]
        "icon/windows/paint" => crate::render_pack::get("icons/windows-paint.png")?,
        #[cfg(feature = "os-windows")]
        "icon/windows/clipchamp" => crate::render_pack::get("icons/windows-clipchamp.png")?,
        #[cfg(feature = "os-windows")]
        "icon/windows/freecad" => crate::render_pack::get("icons/windows-freecad.png")?,
        #[cfg(feature = "os-windows")]
        "icon/windows/spreadsheet" => crate::render_pack::get("icons/windows-spreadsheet.png")?,
        #[cfg(feature = "os-windows")]
        "icon/windows/database" => crate::render_pack::get("icons/windows-database.png")?,
        #[cfg(feature = "os-windows")]
        "icon/windows/kicad" => crate::render_pack::get("icons/windows-kicad.png")?,
        #[cfg(feature = "os-ubuntu")]
        "wallpaper/ubuntu" => crate::render_pack::get("wallpapers/ubuntu.jpg")?,
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/files" => crate::render_pack::get("icons/ubuntu-files.png")?,
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/browser" => crate::render_pack::get("icons/ubuntu-browser.png")?,
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/terminal" => crate::render_pack::get("icons/ubuntu-terminal.png")?,
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/docs" | "icon/ubuntu/editor" => {
            crate::render_pack::get("icons/ubuntu-docs.png")?
        }
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/mail" => crate::render_pack::get("icons/ubuntu-mail.png")?,
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/calendar" => crate::render_pack::get("icons/ubuntu-calendar.png")?,
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/chat" | "icon/ubuntu/messages" => {
            crate::render_pack::get("icons/ubuntu-chat.png")?
        }
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/settings" => crate::render_pack::get("icons/ubuntu-settings.png")?,
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/camera" => crate::render_pack::get("icons/ubuntu-camera.png")?,
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/photos" => crate::render_pack::get("icons/ubuntu-photos.png")?,
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/phone" => crate::render_pack::get("icons/ubuntu-phone.png")?,
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/store" => crate::render_pack::get("icons/ubuntu-store.png")?,
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/launcher" => crate::render_pack::get("icons/ubuntu-launcher.png")?,
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/trash" => crate::render_pack::get("icons/ubuntu-trash.png")?,
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/notes" | "icon/ubuntu/notepad" => {
            crate::render_pack::get("icons/ubuntu-notes.png")?
        }
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/contacts" | "icon/ubuntu/addressbook" => {
            crate::render_pack::get("icons/ubuntu-contacts.png")?
        }
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/clock" | "icon/ubuntu/clocks" => {
            crate::render_pack::get("icons/ubuntu-clock.png")?
        }
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/calculator" | "icon/ubuntu/calc" => {
            crate::render_pack::get("icons/ubuntu-calculator.png")?
        }
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/music" => crate::render_pack::get("icons/ubuntu-music.png")?,
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/maps" => crate::render_pack::get("icons/ubuntu-maps.png")?,
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/weather" => crate::render_pack::get("icons/ubuntu-weather.png")?,
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/code" => crate::render_pack::get("icons/ubuntu-code.png")?,
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/gimp" => crate::render_pack::get("icons/ubuntu-gimp.png")?,
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/kdenlive" => crate::render_pack::get("icons/ubuntu-kdenlive.png")?,
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/kicad" => crate::render_pack::get("icons/ubuntu-kicad.png")?,
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/pinta" => crate::render_pack::get("icons/ubuntu-pinta.png")?,
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/freecad" => crate::render_pack::get("icons/ubuntu-freecad.png")?,
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/spreadsheet" => crate::render_pack::get("icons/ubuntu-spreadsheet.png")?,
        #[cfg(feature = "os-ubuntu")]
        "icon/ubuntu/database" => crate::render_pack::get("icons/ubuntu-database.png")?,
        #[cfg(feature = "os-ios")]
        "wallpaper/ios" => crate::render_pack::get("wallpapers/ios.jpg")?,
        #[cfg(feature = "os-ios")]
        "icon/ios/files" => crate::render_pack::get("icons/ios-files.png")?,
        #[cfg(feature = "os-ios")]
        "icon/ios/browser" => crate::render_pack::get("icons/ios-browser.png")?,
        #[cfg(feature = "os-ios")]
        "icon/ios/terminal" => crate::render_pack::get("icons/ios-terminal.png")?,
        #[cfg(feature = "os-ios")]
        "icon/ios/docs" | "icon/ios/editor" => crate::render_pack::get("icons/ios-docs.png")?,
        #[cfg(feature = "os-ios")]
        "icon/ios/mail" => crate::render_pack::get("icons/ios-mail.png")?,
        #[cfg(feature = "os-ios")]
        "icon/ios/calendar" => crate::render_pack::get("icons/ios-calendar.png")?,
        #[cfg(feature = "os-ios")]
        "icon/ios/chat" | "icon/ios/messages" => crate::render_pack::get("icons/ios-chat.png")?,
        #[cfg(feature = "os-ios")]
        "icon/ios/settings" => crate::render_pack::get("icons/ios-settings.png")?,
        #[cfg(feature = "os-ios")]
        "icon/ios/camera" => crate::render_pack::get("icons/ios-camera.png")?,
        #[cfg(feature = "os-ios")]
        "icon/ios/photos" => crate::render_pack::get("icons/ios-photos.png")?,
        #[cfg(feature = "os-ios")]
        "icon/ios/phone" => crate::render_pack::get("icons/ios-phone.png")?,
        #[cfg(feature = "os-ios")]
        "icon/ios/store" => crate::render_pack::get("icons/ios-store.png")?,
        #[cfg(feature = "os-ios")]
        "icon/ios/launcher" => crate::render_pack::get("icons/ios-launcher.png")?,
        #[cfg(feature = "os-ios")]
        "icon/ios/trash" => crate::render_pack::get("icons/ios-trash.png")?,
        #[cfg(feature = "os-ios")]
        "icon/ios/notes" | "icon/ios/notepad" => {
            crate::render_pack::get("icons/ios-notes.png")?
        }
        #[cfg(feature = "os-ios")]
        "icon/ios/contacts" | "icon/ios/addressbook" => {
            crate::render_pack::get("icons/ios-contacts.png")?
        }
        #[cfg(feature = "os-ios")]
        "icon/ios/clock" | "icon/ios/clocks" => {
            crate::render_pack::get("icons/ios-clock.png")?
        }
        #[cfg(feature = "os-ios")]
        "icon/ios/calculator" | "icon/ios/calc" => {
            crate::render_pack::get("icons/ios-calculator.png")?
        }
        #[cfg(feature = "os-ios")]
        "icon/ios/music" => crate::render_pack::get("icons/ios-music.png")?,
        #[cfg(feature = "os-ios")]
        "icon/ios/maps" => crate::render_pack::get("icons/ios-maps.png")?,
        #[cfg(feature = "os-ios")]
        "icon/ios/weather" => crate::render_pack::get("icons/ios-weather.png")?,
        #[cfg(feature = "os-ios")]
        "icon/ios/code" => crate::render_pack::get("icons/ios-code.png")?,
        #[cfg(feature = "os-ios")]
        "icon/ios/spreadsheet" => crate::render_pack::get("icons/ios-spreadsheet.png")?,
        #[cfg(feature = "os-ios")]
        "icon/ios/imovie" => crate::render_pack::get("icons/ios-imovie.png")?,
        #[cfg(feature = "os-android")]
        "wallpaper/android" => crate::render_pack::get("wallpapers/android.jpg")?,
        #[cfg(feature = "os-android")]
        "icon/android/files" => crate::render_pack::get("icons/android-files.png")?,
        #[cfg(feature = "os-android")]
        "icon/android/browser" => crate::render_pack::get("icons/android-browser.png")?,
        #[cfg(feature = "os-android")]
        "icon/android/terminal" => crate::render_pack::get("icons/android-terminal.png")?,
        #[cfg(feature = "os-android")]
        "icon/android/docs" | "icon/android/editor" => {
            crate::render_pack::get("icons/android-docs.png")?
        }
        #[cfg(feature = "os-android")]
        "icon/android/mail" => crate::render_pack::get("icons/android-mail.png")?,
        #[cfg(feature = "os-android")]
        "icon/android/calendar" => crate::render_pack::get("icons/android-calendar.png")?,
        #[cfg(feature = "os-android")]
        "icon/android/chat" | "icon/android/messages" => {
            crate::render_pack::get("icons/android-chat.png")?
        }
        #[cfg(feature = "os-android")]
        "icon/android/settings" => crate::render_pack::get("icons/android-settings.png")?,
        #[cfg(feature = "os-android")]
        "icon/android/camera" => crate::render_pack::get("icons/android-camera.png")?,
        #[cfg(feature = "os-android")]
        "icon/android/photos" => crate::render_pack::get("icons/android-photos.png")?,
        #[cfg(feature = "os-android")]
        "icon/android/phone" => crate::render_pack::get("icons/android-phone.png")?,
        #[cfg(feature = "os-android")]
        "icon/android/store" => crate::render_pack::get("icons/android-store.png")?,
        #[cfg(feature = "os-android")]
        "icon/android/launcher" => crate::render_pack::get("icons/android-launcher.png")?,
        #[cfg(feature = "os-android")]
        "icon/android/trash" => crate::render_pack::get("icons/android-trash.png")?,
        #[cfg(feature = "os-android")]
        "icon/android/notes" | "icon/android/notepad" => {
            crate::render_pack::get("icons/android-notes.png")?
        }
        #[cfg(feature = "os-android")]
        "icon/android/contacts" | "icon/android/addressbook" => {
            crate::render_pack::get("icons/android-contacts.png")?
        }
        #[cfg(feature = "os-android")]
        "icon/android/clock" | "icon/android/clocks" => {
            crate::render_pack::get("icons/android-clock.png")?
        }
        #[cfg(feature = "os-android")]
        "icon/android/calculator" | "icon/android/calc" => {
            crate::render_pack::get("icons/android-calculator.png")?
        }
        #[cfg(feature = "os-android")]
        "icon/android/music" => crate::render_pack::get("icons/android-music.png")?,
        #[cfg(feature = "os-android")]
        "icon/android/maps" => crate::render_pack::get("icons/android-maps.png")?,
        #[cfg(feature = "os-android")]
        "icon/android/weather" => crate::render_pack::get("icons/android-weather.png")?,
        #[cfg(feature = "os-android")]
        "icon/android/code" => crate::render_pack::get("icons/android-code.png")?,
        #[cfg(feature = "os-android")]
        "icon/android/sketchbook" => crate::render_pack::get("icons/android-sketchbook.png")?,
        #[cfg(feature = "os-android")]
        "icon/android/videoeditor" => {
            crate::render_pack::get("icons/android-videoeditor.png")?
        }
        #[cfg(feature = "os-android")]
        "icon/android/spreadsheet" => crate::render_pack::get("icons/android-spreadsheet.png")?,
        _ => {
            let (_, file) = SYMBOLS.iter().find(|(name, _)| *name == id)?;
            return crate::render_pack::get(file);
        }
    })
}
pub fn decode(id: &str) -> Option<Arc<Frame>> {
    // Each immutable resource is decoded once per process/Wasm instance and shared
    // across all worlds; renderer reset/fork never duplicates wallpaper storage.
    static CACHE: OnceLock<Vec<OnceLock<Arc<Frame>>>> = OnceLock::new();
    let parts: Vec<&str> = id.split('/').collect();
    let canonical = if parts.first() == Some(&"icon") {
        let (platform, name) = match parts.as_slice() {
            [_, name] => ("macos", *name),
            // Platform-neutral content; shells substitute their own artwork.
            [_, "common", name] => ("macos", *name),
            [_, platform, name] => (*platform, *name),
            _ => return None,
        };
        let name = match name {
            "editor" => "docs",
            "messages" => "chat",
            "notepad" => "notes",
            "addressbook" => "contacts",
            "clocks" => "clock",
            "calc" => "calculator",
            other => other,
        };
        format!("icon/{platform}/{name}")
    } else {
        id.to_owned()
    };
    let index = ASSET_IDS
        .iter()
        .position(|candidate| *candidate == canonical)
        .or_else(|| {
            SYMBOLS
                .iter()
                .position(|(name, _)| *name == canonical)
                .map(|i| ASSET_IDS.len() + i)
        })?;
    let cache = CACHE.get_or_init(|| {
        (0..ASSET_IDS.len() + SYMBOLS.len())
            .map(|_| OnceLock::new())
            .collect()
    });
    if let Some(frame) = cache[index].get() {
        return Some(frame.clone());
    }
    // A file this build fetches and the host cannot give resolves to nothing (and is
    // recorded as missing, so the frame is refused); it is not cached, so the next
    // frame asks again.
    let bytes = bytes(&canonical)?;
    let frame = decode_bytes(bytes).expect("pinned image validated by asset test");
    Some(cache[index].get_or_init(|| Arc::new(frame)).clone())
}
/// Dispatch on the container's magic number rather than on the identifier, so
/// re-encoding an asset never needs a matching edit here.
fn decode_bytes(bytes: &[u8]) -> Option<Frame> {
    match bytes {
        [0xFF, 0xD8, 0xFF, ..] => decode_jpeg(bytes),
        _ => decode_png(bytes),
    }
}
/// Baseline JPEG, integer-only. The wallpapers are encoded 4:4:4, so
/// `jpeg-decoder`'s chroma upsampler — the one part of that crate that uses
/// floating point — is never reached; `build-wallpapers.py` explains why.
fn decode_jpeg(bytes: &[u8]) -> Option<Frame> {
    let mut decoder = jpeg_decoder::Decoder::new(std::io::Cursor::new(bytes));
    let pixels = decoder.decode().ok()?;
    let info = decoder.info()?;
    let (width, height) = (u32::from(info.width), u32::from(info.height));
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => {
            for p in pixels.as_chunks::<3>().0 {
                rgba.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
        }
        jpeg_decoder::PixelFormat::L8 => {
            for p in &pixels {
                rgba.extend_from_slice(&[*p, *p, *p, 255]);
            }
        }
        _ => return None,
    }
    Some(Frame {
        width,
        height,
        rgba,
    })
}
fn decode_png(bytes: &[u8]) -> Option<Frame> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut raw = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut raw).ok()?;
    let raw = &raw[..info.buffer_size()];
    let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
    match info.color_type {
        png::ColorType::Rgba => rgba.extend_from_slice(raw),
        png::ColorType::Rgb => {
            for p in raw.as_chunks::<3>().0 {
                rgba.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
        }
        png::ColorType::Grayscale => {
            for p in raw {
                rgba.extend_from_slice(&[*p, *p, *p, 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for p in raw.as_chunks::<2>().0 {
                rgba.extend_from_slice(&[p[0], p[0], p[0], p[1]]);
            }
        }
        png::ColorType::Indexed => return None,
    }
    Some(Frame {
        width: info.width,
        height: info.height,
        rgba,
    })
}

#[cfg(test)]
mod icon_table_tests {
    use super::*;

    /// Original vector artwork rasterizes to 128 px; Yaru PNGs ship at 256 px.
    fn expected_side(id: &str) -> u32 {
        if id.starts_with("icon/ubuntu/") {
            256
        } else {
            128
        }
    }

    #[test]
    fn every_icon_decodes_at_its_expected_size_and_is_not_blank() {
        for id in ASSET_IDS.iter().filter(|id| id.starts_with("icon/")) {
            let frame = decode(id).unwrap_or_else(|| panic!("{id} resolves to no bytes"));
            let side = expected_side(id);
            assert_eq!((frame.width, frame.height), (side, side), "{id}");
            assert_eq!(frame.rgba.len(), (side * side * 4) as usize, "{id}");
            let opaque = frame
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|px| px[3] >= 128)
                .count();
            // An id that decodes but draws nothing is the failure this catches;
            // the thinnest artwork bundled (a Windows silhouette) covers a tenth.
            assert!(
                opaque * 20 >= frame.rgba.len() / 4,
                "{id} is {opaque} opaque pixels of {}",
                frame.rgba.len() / 4
            );
        }
    }

    #[test]
    fn icon_aliases_share_one_decoded_resource() {
        for platform in ["macos", "windows", "ubuntu", "ios", "android"] {
            for (alias, canonical) in [
                ("editor", "docs"),
                ("messages", "chat"),
                ("notepad", "notes"),
                ("addressbook", "contacts"),
                ("clocks", "clock"),
                ("calc", "calculator"),
            ] {
                let aliased = decode(&format!("icon/{platform}/{alias}"))
                    .unwrap_or_else(|| panic!("icon/{platform}/{alias}"));
                let named = decode(&format!("icon/{platform}/{canonical}")).unwrap();
                assert!(
                    Arc::ptr_eq(&aliased, &named),
                    "icon/{platform}/{alias} is decoded twice"
                );
            }
        }
    }

    #[test]
    fn every_advertised_id_has_embedded_bytes() {
        for id in ASSET_IDS {
            assert!(bytes(id).is_some(), "{id} has no embedded bytes");
        }
        for (name, _) in SYMBOLS {
            assert!(decode(name).is_some(), "{name} does not resolve");
        }
    }

    /// The wallpapers are the only assets that go through the JPEG decoder, and
    /// that decoder is the only place in the renderer where a target could
    /// plausibly disagree with another: `jpeg-decoder` ships SSSE3, NEON and
    /// `simd128` IDCT kernels that are *not* bit-identical to its scalar one.
    /// They are compiled out by the `platform_independent` feature, and these
    /// digests are what proves it — on x86, on aarch64 and under Wasm alike.
    /// A mismatch here means a rendered desktop has silently moved.
    #[test]
    fn wallpapers_decode_to_pinned_pixels() {
        use sha2::{Digest, Sha256};
        for (id, side, digest) in [
            (
                "wallpaper/macos",
                (1586u32, 992u32),
                "0c4ee56e008ffba63f90c471b3179c26fffda660e93e336133a45e3499bb71ba",
            ),
            (
                "wallpaper/windows",
                (1586, 992),
                "4aeea3cfd20992b1c54a824c044d30d81c5ed274df3fd1e709131a741623c2ef",
            ),
            (
                "wallpaper/ubuntu",
                (1600, 900),
                "8de818e28f09f273e5485b18f9d062706eebc0769c9f0985e2253a1492fbe68d",
            ),
            (
                "wallpaper/ios",
                (853, 1844),
                "ce600fdf77039d52b04338b0f1837e9c73a2394f4451cf30ce2da2d0e70e1bf2",
            ),
            (
                "wallpaper/android",
                (853, 1844),
                "b51aac4e98c71da36321e6a0567b33f2f1046b06ed23b2a1b239ad693d661ada",
            ),
        ] {
            let frame = decode(id).unwrap_or_else(|| panic!("{id} resolves to no bytes"));
            assert_eq!((frame.width, frame.height), side, "{id} dimensions");
            assert_eq!(
                frame.rgba.len(),
                (side.0 * side.1 * 4) as usize,
                "{id} buffer size"
            );
            // Wallpapers are opaque: JPEG carries no alpha and the renderer
            // composites them as a fully covering background.
            assert!(
                frame.rgba.as_chunks::<4>().0.iter().all(|px| px[3] == 255),
                "{id} decoded a non-opaque pixel"
            );
            assert_eq!(format!("{:x}", Sha256::digest(&frame.rgba)), digest, "{id}");
        }
    }
}
