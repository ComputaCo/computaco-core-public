//! Shared wire helpers, without service domain semantics. `html` is the template
//! layer for services that answer with `text/html`, and `audit` is the dead-control
//! check a service's tests run over what that layer emitted.
/// Dead-control auditing for a service's tests. Needs the engine, so it rides the
/// same `validate` feature `html::validate_strict` does.
#[cfg(any(test, feature = "validate"))]
pub mod audit;
pub mod civil;
pub mod desk;
pub mod html;
#[cfg(feature = "validate")]
pub mod reference;
/// What a service's tests share (the open services' and the look-alikes'), so their checks have one home.
#[cfg(feature = "validate")]
pub mod testkit;
use cw_protocol::{
    HttpRequest, HttpResponse, Page, PageAction, PageElement, PageTheme, Result, SimError, Style,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
pub fn load<T: DeserializeOwned + Default>(value: &Value) -> Result<T> {
    if value.is_null() {
        Ok(T::default())
    } else {
        Ok(serde_json::from_value(value.clone())?)
    }
}
pub fn save<T: Serialize>(state: &mut Value, value: &T) -> Result<()> {
    *state = serde_json::to_value(value)?;
    Ok(())
}
pub fn path(req: &HttpRequest) -> String {
    url::Url::parse(&req.url)
        .map(|u| u.path().to_owned())
        .unwrap_or_else(|_| req.url.split('?').next().unwrap_or("/").to_owned())
}
pub fn query(req: &HttpRequest, key: &str) -> Option<String> {
    url::Url::parse(&req.url)
        .ok()?
        .query_pairs()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
}
/// A file a browser sent in a `multipart/form-data` body: its name, its declared
/// media type and its exact bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Upload {
    pub field: String,
    pub filename: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

/// One part of a `multipart/form-data` body (RFC 7578): a text field or a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    Text { name: String, value: String },
    File(Upload),
}

/// The parts of a `multipart/form-data` request, in order; `None` when the request
/// is not one or its body does not parse. A file input left empty arrives as a file
/// part with no name and no bytes and is dropped, as servers do.
pub fn multipart(req: &HttpRequest) -> Option<Vec<Part>> {
    let ct = req.header("content-type")?;
    if !ct.to_ascii_lowercase().starts_with("multipart/form-data") {
        return None;
    }
    let boundary = ct.split(';').find_map(|p| {
        let p = p.trim();
        p.strip_prefix("boundary=")
            .map(|b| b.trim_matches('"').to_owned())
    })?;
    let delimiter = format!("--{boundary}").into_bytes();
    let body = &req.body[..];
    let find = |hay: &[u8], needle: &[u8], from: usize| -> Option<usize> {
        hay.get(from..)?
            .windows(needle.len())
            .position(|w| w == needle)
            .map(|i| i + from)
    };
    let mut at = find(body, &delimiter, 0)? + delimiter.len();
    let mut out = vec![];
    loop {
        if body.get(at..at + 2) == Some(b"--") {
            return Some(out);
        }
        at += 2; // CRLF after the delimiter
        let head_end = find(body, b"\r\n\r\n", at)?;
        let head = String::from_utf8_lossy(&body[at..head_end]).into_owned();
        let content_start = head_end + 4;
        let next = find(body, &delimiter, content_start)?;
        // The part's content ends with the CRLF before the next delimiter.
        let content = &body[content_start..next.saturating_sub(2).max(content_start)];
        let mut name = String::new();
        let mut filename = None;
        let mut content_type = "text/plain".to_owned();
        for line in head.split("\r\n") {
            let (key, value) = line.split_once(':').unwrap_or((line, ""));
            match key.trim().to_ascii_lowercase().as_str() {
                "content-disposition" => {
                    for param in value.split(';').skip(1) {
                        let Some((k, v)) = param.trim().split_once('=') else {
                            continue;
                        };
                        let v = v.trim().trim_matches('"').replace("%22", "\"");
                        match k.trim() {
                            "name" => name = v,
                            "filename" => filename = Some(v),
                            _ => {}
                        }
                    }
                }
                "content-type" => content_type = value.trim().to_owned(),
                _ => {}
            }
        }
        match filename {
            Some(f) if f.is_empty() && content.is_empty() => {}
            Some(filename) => out.push(Part::File(Upload {
                field: name,
                filename,
                content_type,
                bytes: content.to_vec(),
            })),
            None => out.push(Part::Text {
                name,
                value: String::from_utf8_lossy(content).into_owned(),
            }),
        }
        at = next + delimiter.len();
    }
}

/// The files of a `multipart/form-data` request, in the order they were sent.
pub fn uploads(req: &HttpRequest) -> Vec<Upload> {
    multipart(req)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| match p {
            Part::File(u) => Some(u),
            Part::Text { .. } => None,
        })
        .collect()
}

pub fn body(req: &HttpRequest) -> Result<Value> {
    if req.body.is_empty() {
        return Ok(json!({}));
    }
    // A multipart body reads like a urlencoded one for its text fields; a file field
    // is `{"filename", "type", "size"}` (its bytes are `uploads`').
    if let Some(parts) = multipart(req) {
        let mut out = serde_json::Map::new();
        for p in parts {
            match p {
                Part::Text { name, value } => {
                    out.insert(name, Value::String(value));
                }
                Part::File(u) => {
                    out.insert(
                        u.field,
                        json!({"filename": u.filename, "type": u.content_type, "size": u.bytes.len()}),
                    );
                }
            }
        }
        return Ok(Value::Object(out));
    }
    if req
        .header("content-type")
        .is_some_and(|h| h.starts_with("application/x-www-form-urlencoded"))
    {
        Ok(Value::Object(
            url::form_urlencoded::parse(&req.body)
                .map(|(k, v)| (k.into_owned(), Value::String(v.into_owned())))
                .collect(),
        ))
    } else {
        Ok(serde_json::from_slice(&req.body)?)
    }
}
pub fn text(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").into()
}
pub fn number(v: &Value, key: &str) -> Result<u64> {
    v.get(key)
        .and_then(|x| x.as_u64().or_else(|| x.as_str()?.parse().ok()))
        .ok_or_else(|| SimError::invalid(format!("missing or invalid {key}")))
}
pub fn strings(v: &Value, key: &str) -> Vec<String> {
    match v.get(key) {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        Some(Value::String(s)) => s
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect(),
        _ => vec![],
    }
}
pub fn error(status: u16, message: impl Into<String>) -> Result<HttpResponse> {
    HttpResponse::json(status, &json!({"error":message.into()}))
}
pub fn domain<T: Serialize>(result: std::result::Result<T, String>) -> Result<HttpResponse> {
    match result {
        Ok(v) => HttpResponse::json(200, &v),
        Err(e) => error(
            if e.contains("unavailable") || e.contains("writable") {
                403
            } else if e.contains("conflict") {
                409
            } else {
                400
            },
            e,
        ),
    }
}
pub fn heading(id: &str, text: impl Into<String>) -> PageElement {
    PageElement::Heading {
        id: id.into(),
        text: text.into(),
        level: 1,
    }
}
pub fn paragraph(id: &str, text: impl Into<String>) -> PageElement {
    PageElement::Text {
        id: id.into(),
        text: text.into(),
    }
}
pub fn link(id: &str, text: impl Into<String>, url: impl Into<String>) -> PageElement {
    PageElement::Link {
        id: id.into(),
        text: text.into(),
        url: url.into(),
        style: None,
    }
}
/// A link with explicit presentation: a nav item, a tab, a bordered button that
/// navigates. `style.color` replaces the accent, `background`/`border` box it.
pub fn styled_link(
    id: &str,
    text: impl Into<String>,
    url: impl Into<String>,
    style: Style,
) -> PageElement {
    PageElement::Link {
        id: id.into(),
        text: text.into(),
        url: url.into(),
        style: Some(style),
    }
}
/// A link that reads as part of the text around it: `size` pixels, in `color`, on one
/// line at its own width. A username, a channel name, a repository in a sentence.
pub fn inline_link(
    id: &str,
    text: impl Into<String>,
    url: impl Into<String>,
    size: u16,
    color: &str,
) -> PageElement {
    styled_link(
        id,
        text,
        url,
        Style::default().size(size).color(color).one_line(),
    )
}
/// A button with explicit presentation; `submit` is what it performs.
pub fn styled_button(
    id: &str,
    text: impl Into<String>,
    action: PageAction,
    style: Style,
) -> PageElement {
    PageElement::Button {
        id: id.into(),
        text: text.into(),
        action,
        style: Some(style),
    }
}
/// A small rounded label at its own width: a tag, a filter, a language pill. `fill`
/// is its background and `ink` its text colour; `style` is chained on top (a border,
/// a radius other than fully round, a size other than 12).
pub fn chip(id: &str, text: impl Into<String>, fill: &str, ink: &str, style: Style) -> PageElement {
    let size = style.size.unwrap_or(12);
    let pad = style.padding.unwrap_or(10);
    PageElement::Badge {
        id: id.into(),
        text: text.into(),
        style: Style {
            size: Some(size),
            padding: Some(pad),
            // No radius: a badge is fully round unless told otherwise.
            radius: style.radius,
            background: Some(fill.into()),
            color: Some(ink.into()),
            ..style
        },
    }
}
/// A round avatar `size` pixels across, tinted from `name` and showing its initials:
/// the stand-in for a profile picture on every site that has one.
pub fn avatar(id: &str, name: &str, size: u32) -> PageElement {
    let initials: String = name
        .split(|c: char| c.is_whitespace() || c == '-' || c == '_' || c == '.')
        .filter(|w| !w.is_empty())
        .take(2)
        .filter_map(|w| w.chars().next())
        .flat_map(char::to_uppercase)
        .collect();
    PageElement::Thumbnail {
        id: id.into(),
        label: initials,
        style: Style::default()
            .width(size)
            .height(size)
            .radius(size / 2)
            .size((size * 2 / 5).clamp(6, 96) as u16)
            .background(avatar_tint(name))
            .color("#ffffff"),
        action: None,
    }
}
/// A saturated, readable tint chosen from a name, stable across sites and sessions.
pub fn avatar_tint(name: &str) -> String {
    const PALETTE: [&str; 8] = [
        "#5b6dcd", "#2f8f6f", "#c2603a", "#8a4fb8", "#2e86ab", "#b8536b", "#5f7d2e", "#9a6b1f",
    ];
    let mut h = 0xcbf29ce484222325u64;
    for b in name.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x100000001b3);
    }
    PALETTE[(h % 8) as usize].into()
}
/// Soaks up a row's spare width so the chips before it keep their own size, as chips
/// and button rows do on every site instead of stretching edge to edge.
pub fn rest(id: &str) -> PageElement {
    styled(id, "", style().flex(64))
}
/// `children` laid out at their natural widths, left-aligned, the rest of the row empty.
pub fn pills(id: &str, gap: u32, mut children: Vec<PageElement>) -> PageElement {
    children.push(rest(&format!("{id}-rest")));
    styled_row(id, gap, "center", style(), children)
}
pub fn form(id: &str, url: &str, fields: &[(&str, &str, &str)]) -> PageElement {
    let action = PageAction {
        method: "POST".into(),
        url: url.into(),
        fields: fields
            .iter()
            .map(|(key, _, _)| ((*key).into(), format!("${id}-{key}")))
            .collect(),
    };
    PageElement::Form {
        id: id.into(),
        action: action.clone(),
        children: fields
            .iter()
            .map(|(key, label, value)| PageElement::Input {
                id: format!("{id}-{key}"),
                label: (*label).into(),
                value: (*value).into(),
                placeholder: String::new(),
            })
            .chain(std::iter::once(PageElement::Button {
                id: format!("{id}-submit"),
                text: "Submit".into(),
                action,
                style: None,
            }))
            .collect(),
    }
}
pub fn page(title: &str, elements: Vec<PageElement>) -> Result<HttpResponse> {
    HttpResponse::page(&Page {
        version: 1,
        title: title.into(),
        elements,
        theme: None,
        lang: None,
    })
}
pub fn themed_page(
    title: &str,
    theme: PageTheme,
    elements: Vec<PageElement>,
) -> Result<HttpResponse> {
    HttpResponse::page(&Page {
        version: 1,
        title: title.into(),
        elements,
        theme: Some(theme),
        lang: None,
    })
}
/// Fresh presentation hints; chain `Style`'s setters onto it.
pub fn style() -> Style {
    Style::default()
}
/// GET navigation, the action a whole card or thumbnail usually performs.
pub fn visit(url: impl Into<String>) -> PageAction {
    PageAction {
        method: "GET".into(),
        url: url.into(),
        fields: Default::default(),
    }
}
pub fn row(id: &str, gap: u32, align: &str, children: Vec<PageElement>) -> PageElement {
    PageElement::Row {
        id: id.into(),
        children,
        gap,
        align: align.into(),
        style: Style::default(),
    }
}
pub fn styled_row(
    id: &str,
    gap: u32,
    align: &str,
    style: Style,
    children: Vec<PageElement>,
) -> PageElement {
    PageElement::Row {
        id: id.into(),
        children,
        gap,
        align: align.into(),
        style,
    }
}
pub fn grid(id: &str, columns: u32, gap: u32, children: Vec<PageElement>) -> PageElement {
    PageElement::Grid {
        id: id.into(),
        columns,
        children,
        gap,
        style: Style::default(),
    }
}
/// Children stacked top to bottom: a one-column grid, carrying a style so it can take a
/// flex share inside a row the way a page column does.
pub fn column(id: &str, gap: u32, style: Style, children: Vec<PageElement>) -> PageElement {
    PageElement::Grid {
        id: id.into(),
        columns: 1,
        children,
        gap,
        style,
    }
}
pub fn card(id: &str, style: Style, children: Vec<PageElement>) -> PageElement {
    PageElement::Card {
        id: id.into(),
        children,
        style,
        action: None,
    }
}
/// A card that is genuinely one click target; without an action a card is inert.
pub fn card_action(
    id: &str,
    style: Style,
    action: PageAction,
    children: Vec<PageElement>,
) -> PageElement {
    PageElement::Card {
        id: id.into(),
        children,
        style,
        action: Some(action),
    }
}
pub fn styled(id: &str, text: impl Into<String>, style: Style) -> PageElement {
    PageElement::Styled {
        id: id.into(),
        text: text.into(),
        style,
    }
}
/// A picture served by this site: `source` is a same-origin URL the browser fetches,
/// expecting `cw_protocol::RGBA_MEDIA_TYPE`, and `alt` is its accessible name.
pub fn image(
    id: &str,
    source: impl Into<String>,
    alt: impl Into<String>,
    width: u32,
    height: u32,
) -> PageElement {
    PageElement::Image {
        id: id.into(),
        source: source.into(),
        alt: alt.into(),
        width,
        height,
        style: None,
        action: None,
    }
}
/// A `{width, height, rgba}` response in `cw_protocol::RGBA_MEDIA_TYPE`, the form an
/// `Image` element's URL must answer with. `rgba` is straight-alpha RGBA8, row-major.
pub fn rgba_response(width: u32, height: u32, rgba: &[u8]) -> Result<HttpResponse> {
    #[derive(Serialize)]
    struct Asset<'a> {
        width: u32,
        height: u32,
        rgba: &'a [u8],
    }
    Ok(HttpResponse {
        status: 200,
        headers: BTreeMap::from([("content-type".into(), cw_protocol::RGBA_MEDIA_TYPE.into())]),
        body: serde_json::to_vec(&Asset {
            width,
            height,
            rgba,
        })?,
    })
}
pub fn thumbnail(id: &str, label: impl Into<String>, style: Style) -> PageElement {
    PageElement::Thumbnail {
        id: id.into(),
        label: label.into(),
        style,
        action: None,
    }
}
pub fn thumbnail_action(
    id: &str,
    label: impl Into<String>,
    style: Style,
    action: PageAction,
) -> PageElement {
    PageElement::Thumbnail {
        id: id.into(),
        label: label.into(),
        style,
        action: Some(action),
    }
}
pub fn badge(id: &str, text: impl Into<String>, style: Style) -> PageElement {
    PageElement::Badge {
        id: id.into(),
        text: text.into(),
        style,
    }
}
/// A glyph from `cw_protocol::PAGE_ICONS`, as a picture named `label`.
pub fn icon(id: &str, name: &str, label: impl Into<String>, style: Style) -> PageElement {
    PageElement::Icon {
        id: id.into(),
        name: name.into(),
        label: label.into(),
        style,
        action: None,
    }
}
/// An icon that is a button: the padded square is one click target named `label`.
pub fn icon_action(
    id: &str,
    name: &str,
    label: impl Into<String>,
    style: Style,
    action: PageAction,
) -> PageElement {
    PageElement::Icon {
        id: id.into(),
        name: name.into(),
        label: label.into(),
        style,
        action: Some(action),
    }
}
pub fn divider(id: &str) -> PageElement {
    PageElement::Divider {
        id: id.into(),
        style: Style::default(),
    }
}
pub fn spacer(id: &str, height: u32) -> PageElement {
    PageElement::Spacer {
        id: id.into(),
        height,
    }
}
/// Presentation-only variant name. `plain` is the original rendering, and it is omitted from
/// serialised state so worlds and checkpoints written before a skin existed stay byte-identical.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Skin(pub String);
impl Default for Skin {
    fn default() -> Self {
        Self("plain".into())
    }
}
impl Skin {
    pub fn is_plain(&self) -> bool {
        self.0 == "plain"
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    /// Each crate owns its own skin vocabulary; an unknown name is a seed typo, not a fallback.
    pub fn check(&self, allowed: &[&str]) -> Result<()> {
        if allowed.contains(&self.0.as_str()) {
            Ok(())
        } else {
            Err(SimError::invalid(format!(
                "unknown skin {}; expected one of {}",
                self.0,
                allowed.join(", ")
            )))
        }
    }
}
/// Gate a data-driven service's seed state: an object whose documented keys, when present, carry
/// the documented container type. Catches authoring typos at world load rather than at render.
pub fn shape(state: Value, objects: &[&str], arrays: &[&str]) -> Result<Value> {
    let state = if state.is_null() { json!({}) } else { state };
    let map = state
        .as_object()
        .ok_or_else(|| SimError::invalid("service state must be an object"))?;
    for (keys, ok, label) in [
        (objects, Value::is_object as fn(&Value) -> bool, "an object"),
        (arrays, Value::is_array as fn(&Value) -> bool, "an array"),
    ] {
        for key in keys {
            if map.get(*key).is_some_and(|v| !ok(v)) {
                return Err(SimError::invalid(format!("{key} must be {label}")));
            }
        }
    }
    Ok(state)
}
/// A documented discriminant such as `mode` or `layout`; absent means the first listed value.
pub fn variant(state: &Value, key: &str, allowed: &[&str]) -> Result<String> {
    match state.get(key).and_then(Value::as_str).unwrap_or("") {
        "" => Ok(allowed[0].into()),
        v if allowed.contains(&v) => Ok(v.into()),
        v => Err(SimError::invalid(format!(
            "unknown {key} {v}; expected one of {}",
            allowed.join(", ")
        ))),
    }
}
/// Palette carried by seed state; absent or partial `theme` simply leaves renderer defaults.
pub fn theme(state: &Value) -> Result<PageTheme> {
    match state.get("theme") {
        Some(v) => Ok(serde_json::from_value(v.clone())?),
        None => Ok(PageTheme::default()),
    }
}
/// Themed brand landing: a wordmark and a line of copy, with nothing that reads as a control.
pub fn brand_page(brand: &str, tagline: &str, theme: PageTheme) -> Result<HttpResponse> {
    let accent = theme.accent.clone().unwrap_or_else(|| "#1a73e8".into());
    let muted = theme.muted.clone().unwrap_or_else(|| "#5f6368".into());
    themed_page(
        brand,
        theme,
        vec![
            spacer("brand-lead", 48),
            styled(
                "brand",
                brand,
                style().size(40).bold().color(accent).align("center"),
            ),
            styled(
                "tagline",
                tagline,
                style().size(16).color(muted).align("center"),
            ),
        ],
    )
}
/// Explicit textual HTTP links become native navigation controls; no fetch occurs during rendering.
pub fn links(prefix: &str, text: &str) -> Vec<PageElement> {
    text.split_whitespace()
        .enumerate()
        .filter_map(|(i, word)| {
            let url = word.trim_end_matches(['.', ',', ';', ')', ']']);
            let parsed = url::Url::parse(url).ok()?;
            matches!(parsed.scheme(), "http" | "https")
                .then(|| link(&format!("{prefix}-link-{i}"), url, url))
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_multipart_body_gives_its_fields_and_files_with_their_exact_bytes() {
        let mut r = HttpRequest::get("http://x.test/up");
        r.method = "POST".into();
        r.headers.insert(
            "content-type".into(),
            "multipart/form-data; boundary=XyZ".into(),
        );
        let mut body = b"--XyZ\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\nPlan A\r\n--XyZ\r\nContent-Disposition: form-data; name=\"plan\"; filename=\"plan.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n".to_vec();
        body.extend_from_slice(&[0, 13, 10, 45, 45, 255]);
        body.extend_from_slice(b"\r\n--XyZ\r\nContent-Disposition: form-data; name=\"empty\"; filename=\"\"\r\nContent-Type: application/octet-stream\r\n\r\n\r\n--XyZ--\r\n");
        r.body = body;
        let up = uploads(&r);
        assert_eq!(up.len(), 1, "an empty file input sends no file");
        assert_eq!(
            (up[0].field.as_str(), up[0].filename.as_str()),
            ("plan", "plan.bin")
        );
        assert_eq!(up[0].bytes, [0, 13, 10, 45, 45, 255]);
        let v = super::body(&r).unwrap();
        assert_eq!(v["title"], "Plan A");
        assert_eq!(v["plan"]["size"], 6);
        assert!(desk::submitted(&r));
    }
    #[test]
    fn chips_avatars_and_pills_are_well_formed() {
        let PageElement::Thumbnail {
            label, style: s, ..
        } = avatar("a", "Grace Hopper", 40)
        else {
            panic!()
        };
        assert_eq!(label, "GH");
        assert_eq!((s.width, s.radius), (Some(40), Some(20)));
        assert_eq!(avatar_tint("Grace Hopper"), avatar_tint("Grace Hopper"));
        let PageElement::Badge { style: s, .. } = chip("c", "rust", "#dea584", "#111111", style())
        else {
            panic!()
        };
        assert_eq!(s.background.as_deref(), Some("#dea584"));
        let PageElement::Row { children, .. } = pills("p", 8, vec![link("l", "Home", "/")]) else {
            panic!()
        };
        assert_eq!(children.len(), 2);
        assert!(
            matches!(&children[1], PageElement::Styled { style, .. } if style.flex == Some(64))
        );
        let PageElement::Link { style: Some(s), .. } =
            inline_link("i", "@ada", "/u/ada", 13, "#1264a3")
        else {
            panic!()
        };
        assert_eq!(s.one_line, Some(true));
        let mut page = Page::new("t");
        page.elements = vec![
            avatar("a", "Grace Hopper", 40),
            chip("c", "rust", "#dea584", "#111111", style()),
            pills(
                "p",
                8,
                vec![inline_link("i", "@ada", "/u/ada", 13, "#1264a3")],
            ),
        ];
        page.validate().unwrap();
    }
    #[test]
    fn forms_have_scoped_controls_and_wire_names() {
        let first = form("one", "/save", &[("title", "Title", "")]);
        let second = form("two", "/save", &[("title", "Title", "")]);
        let PageElement::Form {
            action, children, ..
        } = first
        else {
            panic!()
        };
        assert_eq!(action.fields["title"], "$one-title");
        let PageElement::Input { id, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(id, "one-title");
        let PageElement::Form { children, .. } = second else {
            panic!()
        };
        let PageElement::Input { id, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(id, "two-title");
    }
}
