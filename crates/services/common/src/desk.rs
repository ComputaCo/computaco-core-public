//! A back-office dashboard: the page shell, form fields, stat tiles and the
//! Post/Redirect/Get answer a browser form gets. The creator studios, the stock library,
//! the podcast host and the newsroom's CMS are all built from it; each brings its own
//! colour as `--accent` and its own skin as a class on `<body>`.
use crate::html::{div, el, label, page, span, text_input, Document, Html};
use cw_protocol::{HttpRequest, HttpResponse, PageTheme, Result};
use std::collections::BTreeMap;

pub const CSS: &str = include_str!("desk.css");

/// `123456` -> `$1,234.56`; a negative amount keeps its sign.
pub fn money(cents: i64) -> String {
    let n = cents.unsigned_abs();
    let dollars = (n / 100).to_string();
    let mut grouped = String::new();
    for (i, c) in dollars.chars().enumerate() {
        if i > 0 && (dollars.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(c);
    }
    format!(
        "{}${grouped}.{:02}",
        if cents < 0 { "-" } else { "" },
        n % 100
    )
}

/// What a person types for an amount: `12`, `12.5`, `$1,234.50`. Never a float.
pub fn parse_money(text: &str) -> std::result::Result<i64, String> {
    let t: String = text
        .trim()
        .trim_start_matches('$')
        .chars()
        .filter(|c| *c != ',')
        .collect();
    let bad = || format!("{text:?} is not an amount");
    let (whole, frac) = t.split_once('.').unwrap_or((&t, ""));
    if (whole.is_empty() && frac.is_empty())
        || !whole.chars().all(|c| c.is_ascii_digit())
        || !frac.chars().all(|c| c.is_ascii_digit())
        || frac.len() > 2
        || whole.len() > 9
    {
        return Err(bad());
    }
    let whole: i64 = if whole.is_empty() {
        0
    } else {
        whole.parse().map_err(|_| bad())?
    };
    let frac: i64 = match frac.len() {
        0 => 0,
        1 => frac.parse::<i64>().map_err(|_| bad())? * 10,
        _ => frac.parse().map_err(|_| bad())?,
    };
    Ok(whole * 100 + frac)
}

/// Thousands separators: `18342` -> `18,342`.
pub fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `Brightloop Coffee Co.` -> `brightloop-coffee-co`.
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

/// A comma list as typed: trimmed, empty items dropped, each once, in order.
pub fn list(text: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for item in text.split(',') {
        let item = item.trim().to_owned();
        if !item.is_empty() && !out.contains(&item) {
            out.push(item);
        }
    }
    out
}

/// A code drawn from what it names, never from a random source: FNV-1a over the parts,
/// spelled in an alphabet without look-alike characters.
pub fn code(parts: &[&str], len: usize) -> String {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut out = String::new();
    for round in 0..len.div_ceil(8) {
        for part in parts {
            for b in part.bytes().chain(*b"\x1f") {
                h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        h = (h ^ round as u64).wrapping_mul(0x0000_0100_0000_01b3);
        let mut x = h;
        for _ in 0..8 {
            out.push(ALPHABET[(x % ALPHABET.len() as u64) as usize] as char);
            x /= ALPHABET.len() as u64;
        }
    }
    out.truncate(len);
    out
}

/// FNV-1a 64 over bytes: a stable digest for a file the service does not keep.
pub fn digest(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h = (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// The request's host, lowercased, without a port or a trailing dot.
pub fn host(r: &HttpRequest) -> String {
    url::Url::parse(&r.url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_default()
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

/// A browser's form post, as opposed to an API client's JSON.
pub fn submitted(r: &HttpRequest) -> bool {
    r.header("content-type").is_some_and(|h| {
        h.starts_with("application/x-www-form-urlencoded") || h.starts_with("multipart/form-data")
    })
}

/// Post/Redirect/Get: the form lands on the page it changed, and a reload does not post again.
pub fn see_other(location: &str) -> HttpResponse {
    HttpResponse {
        status: 303,
        headers: BTreeMap::from([("location".into(), location.to_owned())]),
        body: vec![],
    }
}

/// A body served with its own type: a feed, a certificate, a file.
pub fn typed(status: u16, content_type: &str, body: impl Into<Vec<u8>>) -> HttpResponse {
    HttpResponse {
        status,
        headers: BTreeMap::from([("content-type".into(), content_type.into())]),
        body: body.into(),
    }
}

/// A labelled one-line field.
pub fn field(id: &str, name: &str, caption: &str, value: &str) -> Html {
    div("field")
        .child(label(id, caption))
        .child(text_input(id, name, value).attr("autocomplete", "off"))
}

/// A labelled field with a hint under it.
pub fn field_hint(id: &str, name: &str, caption: &str, value: &str, hint: &str) -> Html {
    field(id, name, caption, value).child(el("p").class("hint").text(hint))
}

/// A labelled multi-line field.
pub fn area(id: &str, name: &str, caption: &str, value: &str, rows: u32) -> Html {
    div("field").child(label(id, caption)).child(
        el("textarea")
            .id(id)
            .attr("name", name)
            .attr("rows", rows.to_string())
            .attr("spellcheck", "false")
            .text(value),
    )
}

/// One figure on a stats strip.
pub fn stat(id: &str, caption: &str, value: impl Into<String>) -> Html {
    div("stat")
        .id(id)
        .child(span("cap").text(caption))
        .child(span("num").text(value))
}

/// A status as a coloured pill: `good`, `warn`, `bad` or plain.
pub fn badge(id: &str, tone: &str, text: impl Into<String>) -> Html {
    span(&format!("badge {tone}")).id(id).text(text)
}

/// A heading with a line under it.
pub fn head(title: &str, sub: &str) -> Html {
    div("head").child(
        div("")
            .child(el("h1").id("title").text(title))
            .child(el("p").id("subtitle").class("sub").text(sub)),
    )
}

/// The look every dashboard shares; the brand's colour rides in as `--accent`.
pub struct Shell<'a> {
    pub brand: &'a str,
    pub mark: &'a str,
    pub skin: &'a str,
    pub theme: &'a PageTheme,
    pub actor: &'a str,
    /// `(id, href, label)`; the one whose id is `current` is marked as the page.
    pub nav: Vec<(&'a str, String, &'a str)>,
}
impl Shell<'_> {
    pub fn page(&self, title: &str, current: &str, main: Vec<Html>) -> Result<HttpResponse> {
        self.page_styled(CSS, title, current, main)
    }
    /// The page with another sheet than the desk's: a dashboard family that brings its own (the look-alike services'
    /// business dashboards do).
    pub fn page_styled(
        &self,
        css: &str,
        title: &str,
        current: &str,
        main: Vec<Html>,
    ) -> Result<HttpResponse> {
        let t = self.theme;
        let or =
            |v: &Option<String>, fallback: &str| v.clone().unwrap_or_else(|| fallback.to_owned());
        let root = format!(
            "--accent: {}; --ink: {}; --muted: {}; --surface: {}; --paper: {}",
            or(&t.accent, "#2563eb"),
            or(&t.ink, "#1f2933"),
            or(&t.muted, "#616e7c"),
            or(&t.surface, "#f5f7fa"),
            or(&t.background, "#ffffff"),
        );
        let nav = el("nav")
            .id("nav")
            .class("nav")
            .each(&self.nav, |(id, href, text)| {
                let on = *id == current;
                el("a")
                    .id(*id)
                    .attr("href", href.as_str())
                    .class(if on { "tab on" } else { "tab" })
                    .when(on, |n| n.attr("aria-current", "page"))
                    .text(*text)
            });
        let who = if self.actor.is_empty() {
            "Signed out".to_owned()
        } else {
            format!("Signed in as {}", self.actor)
        };
        let header = el("header").id("top").class("top").child(
            div("top-in")
                .child(
                    span("brand")
                        .id("brand")
                        .child(span("mark").attr("aria-hidden", "true").text(self.mark))
                        .child(span("name").text(self.brand)),
                )
                .child(nav)
                .child(span("who").id("who").text(who)),
        );
        let doc = Document::new(format!("{title} | {}", self.brand))
            .lang("en")
            .stylesheet(css)
            .root_style(&root)
            .body_class(&format!("skin-{}", self.skin))
            .body([header, el("main").id("main").class("main").children(main)]);
        page(&doc)
    }
    /// A refused form, said in words on a page rather than as a JSON error.
    pub fn refused(&self, message: &str, back: &str) -> Result<HttpResponse> {
        self.refused_styled(CSS, message, back)
    }
    /// [`Shell::refused`] with another sheet (see [`Shell::page_styled`]).
    pub fn refused_styled(&self, css: &str, message: &str, back: &str) -> Result<HttpResponse> {
        let mut r = self.page_styled(
            css,
            "Something went wrong",
            "",
            vec![div("card error")
                .id("error")
                .child(el("h1").id("error-title").text("That did not go through"))
                .child(el("p").id("error-message").text(message))
                .child(el("a").id("error-back").attr("href", back).text("Go back"))],
        )?;
        r.status = 400;
        Ok(r)
    }
}

/// A form error for a browser, or the JSON error for an API client.
pub fn refuse(shell: &Shell, r: &HttpRequest, message: &str, back: &str) -> Result<HttpResponse> {
    if submitted(r) {
        shell.refused(message, back)
    } else {
        crate::error(400, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn money_and_codes_are_stable() {
        assert_eq!(money(0), "$0.00");
        assert_eq!(money(-123_456), "-$1,234.56");
        assert_eq!(parse_money("$1,234.5"), Ok(123_450));
        assert!(parse_money("1.234").is_err());
        assert_eq!(code(&["a", "1"], 11), code(&["a", "1"], 11));
        assert_ne!(code(&["a", "1"], 11), code(&["a", "2"], 11));
        assert_eq!(slug(" Lantern Media! "), "lantern-media");
        assert_eq!(list(" a, b ,a,, "), vec!["a", "b"]);
        assert_eq!(grouped(1_234_567), "1,234,567");
        assert_eq!(digest(b""), "cbf29ce484222325");
    }
}
