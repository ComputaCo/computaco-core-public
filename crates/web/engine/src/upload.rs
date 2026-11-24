//! A file chosen for an `<input type=file>`: its name, its media type and its exact
//! bytes. The browser gets the bytes from the machine's disk (through a file chooser
//! or the agent API) and a form submission carries them in a `multipart/form-data`
//! body, as a browser sends an upload.
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize, cw_snapshot::Encode, cw_snapshot::Decode,
)]
pub struct FormFile {
    /// The file's name without its folder, as `File.name` and the part's `filename`.
    pub name: String,
    /// The media type guessed from the name's extension, as browsers do.
    #[serde(rename = "type")]
    pub content_type: String,
    #[serde(with = "base64_bytes")]
    pub bytes: Vec<u8>,
}

impl FormFile {
    /// A file named for the last segment of `path`, typed from its extension.
    pub fn new(path: &str, bytes: Vec<u8>) -> FormFile {
        let name = path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(path)
            .to_owned();
        FormFile {
            content_type: media_type_for(&name).to_owned(),
            name,
            bytes,
        }
    }
}

mod base64_bytes {
    pub fn serialize<S: serde::Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&cw_protocol::encode_base64(v))
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(d)?;
        cw_protocol::decode_base64(&text).map_err(serde::de::Error::custom)
    }
}

/// The media type a browser gives a file by its extension (Chromium's
/// `net::GetWellKnownMimeTypeFromExtension`, the common part); unknown is
/// `application/octet-stream`.
pub fn media_type_for(name: &str) -> &'static str {
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "txt" | "text" | "log" => "text/plain",
        "csv" => "text/csv",
        "md" => "text/markdown",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "json" => "application/json",
        "xml" => "text/xml",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "png" => "image/png",
        "apng" => "image/apng",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        _ => "application/octet-stream",
    }
}

/// Whether a file passes an `accept` attribute: a comma-separated list of
/// extensions (`.pdf`), media types (`image/png`) and wildcards (`image/*`). An
/// empty list accepts everything. A chooser only lists what passes, as the
/// platform's dialog filters by it.
pub fn accepts(accept: &str, name: &str) -> bool {
    let media = media_type_for(name);
    let lower = name.to_ascii_lowercase();
    let mut any = false;
    for token in accept.split(',').map(|t| t.trim().to_ascii_lowercase()) {
        if token.is_empty() {
            continue;
        }
        any = true;
        let ok = if token.starts_with('.') {
            lower.ends_with(&token)
        } else if let Some(major) = token.strip_suffix("/*") {
            media.split('/').next() == Some(major)
        } else {
            token == media
        };
        if ok {
            return true;
        }
    }
    !any
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chosen_file_is_named_and_typed_from_its_path_and_keeps_its_bytes() {
        let f = FormFile::new("/home/alice/Documents/plan.PDF", vec![0, 255, 7]);
        assert_eq!(
            (f.name.as_str(), f.content_type.as_str()),
            ("plan.PDF", "application/pdf")
        );
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(
            json,
            r#"{"name":"plan.PDF","type":"application/pdf","bytes":"AP8H"}"#
        );
        assert_eq!(serde_json::from_str::<FormFile>(&json).unwrap(), f);
    }

    #[test]
    fn accept_filters_by_extension_type_and_wildcard() {
        assert!(accepts("", "x.bin"));
        assert!(accepts(".pdf, .txt", "notes.TXT"));
        assert!(!accepts(".pdf", "notes.txt"));
        assert!(accepts("image/*", "logo.png"));
        assert!(accepts("audio/wav,image/apng", "cut.apng"));
        assert!(!accepts("video/*", "cut.apng"));
    }
}
