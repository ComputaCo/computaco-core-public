//! A document with script: the `cw_web::script::Realm` that owns its DOM, styles and
//! layout, the host the realm calls back into, and the bookkeeping that lets a realm
//! (a VM heap, which can be neither cloned nor serialised) live inside browser state
//! that is cloned for every snapshot and serialised for every checkpoint.
//!
//! Ownership. The realm owns the document; `WebDocument` keeps no second DOM for a
//! scripted page and reads the realm's `document()`, `styles()` and
//! `fragment_tree()` to paint, hit-test and project. A `Scripted` is a handle:
//!
//! * the live realm sits in a cell shared (by `Arc`) between every clone of the
//!   document, stamped with an *epoch* that every mutating entry bumps;
//! * each handle remembers the epoch it last saw. A handle whose epoch is the cell's
//!   owns the live realm. A handle left behind (the copy a snapshot holds, once the
//!   live world moved on) carries the `RealmState` it was cloned at, and rebuilds a
//!   realm of its own from it the first time it is used again: from the heap image
//!   the state carries (`RealmState::image`) and a replay of the inputs after it,
//!   or by replaying the whole journal when it carries none;
//! * cloning captures the `RealmState` once per epoch and shares it, so taking a
//!   snapshot costs one copy of the journal (and, once the realm has run
//!   `IMAGE_EVERY_STEPS` VM steps since its last image, writing a new one), and
//!   restoring is paid only if the restored copy is actually used.
//!
//! Serialisation writes the `RealmState`; deserialisation yields a handle with no
//! live realm, restored lazily. A new document is a new realm, so a journal never
//! outlives a navigation.
//!
//! The host. A realm's host must be `'static`, but what it needs (the transport
//! closure, the cookie jar, storage, the world clock, the tab's entropy stream) lives
//! in the `BrowserState` for the length of one call. So the host holds a shared
//! `HostEnv` slot: the browser moves those things into it before entering the realm
//! and takes them back afterwards. Navigations and submissions the page asks for are
//! queued there and performed by the browser after the entry returns, never
//! re-entrantly.
//!
//! Network policy for page script, as a browser's: `fetch`/XHR to another origin is a
//! CORS request (a preflight for any that is not simple, and the answer handed over only
//! when the other origin allows the document's), `mode: 'same-origin'` stays home, and
//! `mode: 'no-cors'` gets an opaque answer. Cookies go, and `Set-Cookie` is honoured, by
//! the request's credentials mode (`same-origin` by default; `include`, or an XHR's
//! `withCredentials`, for another origin, which must then allow credentials). No answer
//! shows a script its `Set-Cookie`, and `document.cookie` never shows or overwrites an
//! `HttpOnly` cookie. The page's own loads (scripts, stylesheets, imports) go to any
//! origin the transport allows, with cookies only to the document's own.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use cw_determinism::Determinism;
use cw_protocol::{HttpRequest, HttpResponse, Result};
use cw_ui::{UiApp, UiState};
use cw_web::confined::Confined;
use cw_web::script::{
    Credentials, FetchRequest, FetchResponse, LogLevel, Realm, RealmState, RequestMode,
    ScriptHostDocument, StorageArea,
};
use cw_web::Viewport;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::page_script::{PageScript, ScriptState};
use crate::{cookie_path_matches, parse_cookie, Cookie, MAX_TEXT_RESOURCE_BYTES};

/// VM steps a document's load (each `<script>`) and each later entry may spend.
/// A real app's first render is one task and can be large: react-admin's post list
/// (MUI, react-query) commits in one scheduler task of 60-100 million steps, so the
/// budget is what that needs with room to spare (about 2.5 s of virtual time).
pub const STEP_BUDGET: u64 = 250_000_000;
/// Virtual milliseconds the event loop is given to settle after every event: zero
/// and near-zero timers (`$(fn)`, a framework's scheduler, a resolved `fetch`) and
/// one animation frame run before the next paint, as they would in a browser, even
/// though the world clock has not moved. The realm's clock may therefore run up to
/// this far ahead of the world's; it never goes back.
pub const SETTLE_MS: u32 = 20;
/// Requests one entry into the realm may make.
pub const MAX_FETCHES_PER_ENTRY: u32 = 64;
/// Lines a tab's console keeps.
pub const MAX_CONSOLE_LINES: usize = 500;

/// One line of a tab's console: `console.*` output, uncaught errors, budget
/// interruptions.
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize, cw_snapshot::Encode, cw_snapshot::Decode,
)]
pub struct ConsoleEntry {
    /// `log`, `info`, `warn`, `error` or `debug`.
    pub level: String,
    pub text: String,
}

/// What a page asked the browser to do once the current event is over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PendingNav {
    Navigate {
        url: String,
        new_tab: bool,
    },
    Submit {
        action: String,
        method: String,
        enctype: String,
        data: Vec<(String, String)>,
        /// The form's chosen files, by field name, sent after `data`.
        files: Vec<(String, cw_web::upload::FormFile)>,
    },
}

pub(crate) struct TransportPtr(*mut (dyn FnMut(HttpRequest) -> Result<HttpResponse> + 'static));
// SAFETY: the pointer is only dereferenced on the thread that installed it, inside
// the call that installed it (`HostEnv::with_transport`), while that call still
// holds the `&mut` it was made from.
unsafe impl Send for TransportPtr {}

/// Everything a realm's host needs from the browser for one entry.
#[derive(Default)]
pub struct HostEnv {
    pub(crate) transport: Option<TransportPtr>,
    pub cookies: BTreeMap<String, Vec<Cookie>>,
    /// localStorage, by origin.
    pub storage: BTreeMap<String, BTreeMap<String, String>>,
    /// The tab's sessionStorage, by origin.
    pub session: BTreeMap<String, BTreeMap<String, String>>,
    /// World clock, microseconds.
    pub now: u64,
    pub entropy: Option<Determinism>,
    pub stream: String,
    pub viewport: Viewport,
    pub console: Vec<ConsoleEntry>,
    pub navs: Vec<PendingNav>,
    /// The document's URL: its origin scopes storage and cookies.
    pub url: String,
    pub(crate) fetches: u32,
    /// Replaying a journal: console lines were already reported.
    pub(crate) muted: bool,
}

impl HostEnv {
    /// Installs `transport` for the length of `f`.
    pub fn with_transport<T>(
        &mut self,
        transport: Option<&mut (dyn FnMut(HttpRequest) -> Result<HttpResponse> + '_)>,
        f: impl FnOnce(&mut HostEnv) -> T,
    ) -> T {
        self.transport = transport.map(|t| {
            let p: *mut (dyn FnMut(HttpRequest) -> Result<HttpResponse> + '_) = t;
            // SAFETY: only the lifetime bound is erased; the pointer is cleared below,
            // before the borrow it came from ends.
            TransportPtr(unsafe {
                std::mem::transmute::<
                    *mut (dyn FnMut(HttpRequest) -> Result<HttpResponse> + '_),
                    *mut (dyn FnMut(HttpRequest) -> Result<HttpResponse> + 'static),
                >(p)
            })
        });
        let out = f(self);
        self.transport = None;
        out
    }

    fn origin(&self) -> String {
        Url::parse(&self.url)
            .map(|u| u.origin().ascii_serialization())
            .unwrap_or_default()
    }

    fn cookie_header(&self, url: &Url, for_script: bool) -> String {
        self.cookies
            .get(&url.origin().ascii_serialization())
            .map(|cookies| {
                cookies
                    .iter()
                    .filter(|c| {
                        cookie_path_matches(url.path(), &c.path)
                            && (!c.secure || url.scheme() == "https")
                            && !(for_script && c.http_only)
                    })
                    .map(|c| format!("{}={}", c.name, c.value))
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .unwrap_or_default()
    }

    fn store_cookie(&mut self, url: &Url, header: &str, from_script: bool) {
        let Some(cookie) = parse_cookie(header, url.path()) else {
            return;
        };
        if from_script && cookie.http_only {
            return;
        }
        let jar = self
            .cookies
            .entry(url.origin().ascii_serialization())
            .or_default();
        if from_script
            && jar
                .iter()
                .any(|c| c.name == cookie.name && c.path == cookie.path && c.http_only)
        {
            return;
        }
        jar.retain(|c| c.name != cookie.name || c.path != cookie.path);
        let lower = header.to_ascii_lowercase();
        if !lower.contains("max-age=0")
            && !lower.contains("max-age=-")
            && !lower.contains("expires=thu, 01 jan 1970")
        {
            jar.push(cookie);
            jar.sort_by(|a, b| (&a.name, &a.path).cmp(&(&b.name, &b.path)));
        }
    }

    /// One request of the page's: its own loads (`RequestMode::Resource`) and its
    /// scripts' `fetch` and `XMLHttpRequest`, with the browser's rules for both. A
    /// script's request to another origin is a CORS request: a simple one (GET, HEAD or
    /// POST with only safelisted headers) is sent and its answer handed over only when
    /// the other origin allows the document's; any other is first asked about with an
    /// `OPTIONS` preflight that must allow its method and headers. Cookies go with a
    /// request, and are kept from its answer, by its credentials mode; an answer never
    /// shows a script its `Set-Cookie`.
    fn fetch(&mut self, request: &FetchRequest) -> std::result::Result<FetchResponse, String> {
        let Some(TransportPtr(transport)) = self.transport.as_ref().map(|t| TransportPtr(t.0))
        else {
            return Err("network unavailable".into());
        };
        self.fetches += 1;
        if self.fetches > MAX_FETCHES_PER_ENTRY {
            return Err("too many requests".into());
        }
        let mode = request.mode;
        let from_script = mode != RequestMode::Resource;
        let mut url = Url::parse(&request.url).map_err(|e| e.to_string())?;
        let document_origin = Url::parse(&self.url).ok().map(|u| u.origin());
        let mut method = request.method.to_ascii_uppercase();
        let mut body = request.body.clone().unwrap_or_default();
        let mut headers: BTreeMap<String, String> = request
            .headers
            .iter()
            .map(|(k, v)| (k.to_ascii_lowercase(), v.clone()))
            .filter(|(k, _)| k != "cookie" && !(from_script && forbidden_request_header(k)))
            .collect();
        // Once a request has left the document's origin its answer is the other
        // origin's to share (CORS "tainting"), whatever origin a redirect ends at; and a
        // redirect from one other origin to a third sends `Origin: null`.
        let mut crossed = false;
        let mut origin_header = document_origin
            .as_ref()
            .map(|o| o.ascii_serialization())
            .unwrap_or_else(|| "null".into());
        let mut preflighted = false;
        let include = request.credentials == Credentials::Include;
        for _ in 0..=8 {
            if !matches!(url.scheme(), "http" | "https")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err("browser supports credential-free http/https URLs only".into());
            }
            url.set_fragment(None);
            let same_origin = !crossed && document_origin.as_ref() == Some(&url.origin());
            if !same_origin {
                crossed = true;
            }
            let cors = !same_origin && mode == RequestMode::Cors;
            match mode {
                RequestMode::SameOrigin if !same_origin => {
                    return Err(self.blocked(
                        &url,
                        "the request's mode is 'same-origin' and the URL is of another origin.",
                    ));
                }
                RequestMode::NoCors
                    if !same_origin && !(simple_method(&method) && simple_headers(&headers)) =>
                {
                    return Err(self.blocked(
                        &url,
                        "a 'no-cors' request may use only GET, HEAD or POST and safelisted headers.",
                    ));
                }
                _ => {}
            }
            if cors && !preflighted && !(simple_method(&method) && simple_headers(&headers)) {
                self.preflight(transport, &url, &origin_header, &method, &headers, include)?;
                preflighted = true;
            }
            let mut out = HttpRequest::get(url.as_str());
            out.method = method.clone();
            out.headers = headers.clone();
            out.body = body.clone();
            if !same_origin && from_script {
                out.headers.insert("origin".into(), origin_header.clone());
            }
            let credentialed = match request.credentials {
                Credentials::Omit => false,
                Credentials::SameOrigin => same_origin,
                Credentials::Include => true,
            };
            if credentialed {
                let cookies = self.cookie_header(&url, false);
                if !cookies.is_empty() {
                    out.headers.insert("cookie".into(), cookies);
                }
            }
            // SAFETY: see `TransportPtr`; `with_transport` is on the stack above us.
            let response = unsafe { (*transport)(out) }.map_err(|e| e.message)?;
            if credentialed {
                if let Some(header) = response.header("set-cookie") {
                    let header = header.to_owned();
                    self.store_cookie(&url, &header, false);
                }
            }
            if cors {
                if let Err(why) = cors_check(&response, &origin_header, include) {
                    return Err(self.blocked(&url, &why));
                }
            }
            if matches!(response.status, 301 | 302 | 303 | 307 | 308) {
                if let Some(location) = response.header("location") {
                    let next = url.join(location).map_err(|e| e.to_string())?;
                    if crossed && next.origin() != url.origin() {
                        origin_header = "null".into();
                    }
                    url = next;
                    if response.status == 303
                        || (matches!(response.status, 301 | 302) && method == "POST")
                    {
                        method = "GET".into();
                        body.clear();
                        headers.remove("content-type");
                    }
                    continue;
                }
            }
            if response.body.len() > MAX_TEXT_RESOURCE_BYTES {
                return Err("response exceeds the subresource budget".into());
            }
            if mode == RequestMode::NoCors && crossed {
                // The page may know only that something answered.
                return Ok(FetchResponse {
                    status: 0,
                    status_text: String::new(),
                    headers: vec![],
                    body: vec![],
                    url: String::new(),
                    kind: "opaque".into(),
                });
            }
            let all: Vec<(String, String)> = response
                .headers
                .iter()
                .map(|(k, v)| (k.to_ascii_lowercase(), v.clone()))
                .filter(|(k, _)| !(from_script && (k == "set-cookie" || k == "set-cookie2")))
                .collect();
            let (headers, kind) = if cors {
                (exposed_headers(&response, all, include), "cors")
            } else {
                (all, "")
            };
            return Ok(FetchResponse {
                status: response.status,
                status_text: status_text(response.status).into(),
                headers,
                body: response.body,
                url: url.to_string(),
                kind: kind.into(),
            });
        }
        Err("redirect limit".into())
    }

    /// A CORS preflight: `OPTIONS` with the method and the headers the request will
    /// carry, sent without cookies; the other origin's answer must allow both.
    fn preflight(
        &mut self,
        transport: *mut (dyn FnMut(HttpRequest) -> Result<HttpResponse> + 'static),
        url: &Url,
        origin: &str,
        method: &str,
        headers: &BTreeMap<String, String>,
        include: bool,
    ) -> std::result::Result<(), String> {
        let unsafe_headers: Vec<&str> = headers
            .iter()
            .filter(|(k, v)| !safelisted_header(k, v))
            .map(|(k, _)| k.as_str())
            .collect();
        let mut out = HttpRequest::get(url.as_str());
        out.method = "OPTIONS".into();
        out.headers.insert("accept".into(), "*/*".into());
        out.headers.insert("origin".into(), origin.into());
        out.headers
            .insert("access-control-request-method".into(), method.into());
        if !unsafe_headers.is_empty() {
            out.headers.insert(
                "access-control-request-headers".into(),
                unsafe_headers.join(","),
            );
        }
        // SAFETY: see `TransportPtr`; `with_transport` is on the stack above us.
        let response = unsafe { (*transport)(out) }.map_err(|e| e.message)?;
        let refused = |why: &str| {
            format!("Response to preflight request doesn't pass access control check: {why}")
        };
        if !(200..300).contains(&response.status) {
            return Err(self.blocked(url, &refused("It does not have HTTP ok status.")));
        }
        if let Err(why) = cors_check(&response, origin, include) {
            return Err(self.blocked(url, &refused(&why)));
        }
        let list = |name: &str| -> Vec<String> {
            response
                .header(name)
                .unwrap_or("")
                .split(',')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect()
        };
        let methods = list("access-control-allow-methods");
        if !simple_method(method)
            && !methods.iter().any(|m| m == method)
            && !(!include && methods.iter().any(|m| m == "*"))
        {
            return Err(self.blocked(
                url,
                &format!("Method {method} is not allowed by Access-Control-Allow-Methods in preflight response."),
            ));
        }
        let allowed = list("access-control-allow-headers");
        for name in unsafe_headers {
            let named = allowed.iter().any(|h| h.eq_ignore_ascii_case(name));
            let wildcard = !include && name != "authorization" && allowed.iter().any(|h| h == "*");
            if !named && !wildcard {
                return Err(self.blocked(
                    url,
                    &format!("Request header field {name} is not allowed by Access-Control-Allow-Headers in preflight response."),
                ));
            }
        }
        Ok(())
    }

    /// A request the browser refused the page, said on its console as Chrome says it.
    fn blocked(&mut self, url: &Url, why: &str) -> String {
        let text = format!(
            "Access to fetch at '{url}' from origin '{}' has been blocked by CORS policy: {why}",
            self.origin()
        );
        if !self.muted {
            self.console.push(ConsoleEntry {
                level: "error".into(),
                text: text.clone(),
            });
        }
        text
    }
}

/// GET, HEAD and POST need no preflight.
fn simple_method(method: &str) -> bool {
    matches!(method, "GET" | "HEAD" | "POST")
}

/// A CORS-safelisted request header (the Fetch standard's list, without `range`).
fn safelisted_header(name: &str, value: &str) -> bool {
    if value.len() > 128 {
        return false;
    }
    let unsafe_byte = |b: u8| {
        (b < 0x20 && b != b'\t')
            || matches!(
                b,
                b'"' | b'('
                    | b')'
                    | b':'
                    | b'<'
                    | b'>'
                    | b'?'
                    | b'@'
                    | b'['
                    | b'\\'
                    | b']'
                    | b'{'
                    | b'}'
                    | 0x7f
            )
    };
    match name {
        "accept" => !value.bytes().any(unsafe_byte),
        "accept-language" | "content-language" => value.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b' ' | b'*' | b',' | b'-' | b'.' | b';' | b'=')
        }),
        "content-type" => {
            !value.bytes().any(unsafe_byte) && {
                let essence = value
                    .split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase();
                matches!(
                    essence.as_str(),
                    "application/x-www-form-urlencoded" | "multipart/form-data" | "text/plain"
                )
            }
        }
        _ => false,
    }
}

fn simple_headers(headers: &BTreeMap<String, String>) -> bool {
    headers.iter().all(|(k, v)| safelisted_header(k, v))
}

/// Headers a page script may not set (the Fetch standard's forbidden request headers):
/// the browser sets them, or they belong to the connection.
fn forbidden_request_header(name: &str) -> bool {
    matches!(
        name,
        "accept-charset"
            | "accept-encoding"
            | "access-control-request-headers"
            | "access-control-request-method"
            | "connection"
            | "content-length"
            | "cookie"
            | "cookie2"
            | "date"
            | "dnt"
            | "expect"
            | "host"
            | "keep-alive"
            | "origin"
            | "referer"
            | "set-cookie"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "via"
    ) || name.starts_with("proxy-")
        || name.starts_with("sec-")
}

/// The CORS check of an answer from another origin: it names the document's origin
/// (or `*`, for a request without credentials), and allows credentials when they went.
fn cors_check(
    response: &HttpResponse,
    origin: &str,
    include: bool,
) -> std::result::Result<(), String> {
    let Some(allowed) = response
        .header("access-control-allow-origin")
        .map(str::trim)
    else {
        return Err(
            "No 'Access-Control-Allow-Origin' header is present on the requested resource.".into(),
        );
    };
    if allowed == "*" && !include {
        return Ok(());
    }
    if allowed == "*" {
        return Err("The value of the 'Access-Control-Allow-Origin' header in the response must not be the wildcard '*' when the request's credentials mode is 'include'.".into());
    }
    if allowed != origin {
        return Err(format!(
            "The 'Access-Control-Allow-Origin' header has a value '{allowed}' that is not equal to the supplied origin."
        ));
    }
    if include
        && response
            .header("access-control-allow-credentials")
            .map(str::trim)
            != Some("true")
    {
        return Err("The value of the 'Access-Control-Allow-Credentials' header in the response must be 'true' when the request's credentials mode is 'include'.".into());
    }
    Ok(())
}

/// What a CORS answer shows the page: the safelisted response headers and those the
/// other origin exposes (`*` exposes all, for a request without credentials).
fn exposed_headers(
    response: &HttpResponse,
    all: Vec<(String, String)>,
    include: bool,
) -> Vec<(String, String)> {
    let exposed: Vec<String> = response
        .header("access-control-expose-headers")
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    let every = !include && exposed.iter().any(|h| h == "*");
    all.into_iter()
        .filter(|(k, _)| {
            every
                || matches!(
                    k.as_str(),
                    "cache-control"
                        | "content-language"
                        | "content-length"
                        | "content-type"
                        | "expires"
                        | "last-modified"
                        | "pragma"
                )
                || exposed.iter().any(|h| h == k)
        })
        .collect()
}

fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        422 => "Unprocessable Entity",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "",
    }
}

/// The realm's host: a handle on the slot the browser fills for each entry.
struct BrowserHost {
    env: Arc<Mutex<HostEnv>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

impl ScriptHostDocument for BrowserHost {
    fn fetch(&mut self, request: &FetchRequest) -> std::result::Result<FetchResponse, String> {
        lock(&self.env).fetch(request)
    }
    fn navigate(&mut self, url: &str) {
        let (url, new_tab) = match url.strip_prefix("cw-new-tab:") {
            Some(rest) => (rest, true),
            None => (url, false),
        };
        lock(&self.env).navs.push(PendingNav::Navigate {
            url: url.to_owned(),
            new_tab,
        });
    }
    fn submit_form(
        &mut self,
        action: &str,
        method: &str,
        enctype: &str,
        data: &[(String, String)],
    ) {
        lock(&self.env).navs.push(PendingNav::Submit {
            action: action.to_owned(),
            method: method.to_owned(),
            enctype: enctype.to_owned(),
            data: data.to_vec(),
            files: vec![],
        });
    }
    fn submit_form_with_files(
        &mut self,
        action: &str,
        method: &str,
        enctype: &str,
        data: &[(String, String)],
        files: &[(String, cw_web::upload::FormFile)],
    ) {
        lock(&self.env).navs.push(PendingNav::Submit {
            action: action.to_owned(),
            method: method.to_owned(),
            enctype: enctype.to_owned(),
            data: data.to_vec(),
            files: files.to_vec(),
        });
    }
    fn now_micros(&self) -> i64 {
        lock(&self.env).now.min(i64::MAX as u64) as i64
    }
    fn random_u64(&mut self) -> u64 {
        let mut env = lock(&self.env);
        let stream = env.stream.clone();
        env.entropy
            .get_or_insert_with(|| Determinism::new(0))
            .next_u64(&stream)
    }
    fn viewport(&self) -> Viewport {
        lock(&self.env).viewport
    }
    fn storage_get(&self, area: StorageArea, key: &str) -> Option<String> {
        let env = lock(&self.env);
        let origin = env.origin();
        match area {
            StorageArea::Local => env.storage.get(&origin),
            StorageArea::Session => env.session.get(&origin),
        }
        .and_then(|m| m.get(key).cloned())
    }
    fn storage_set(&mut self, area: StorageArea, key: &str, value: &str) {
        let mut env = lock(&self.env);
        let origin = env.origin();
        let map = match area {
            StorageArea::Local => &mut env.storage,
            StorageArea::Session => &mut env.session,
        };
        map.entry(origin)
            .or_default()
            .insert(key.to_owned(), value.to_owned());
    }
    fn storage_remove(&mut self, area: StorageArea, key: &str) {
        let mut env = lock(&self.env);
        let origin = env.origin();
        let map = match area {
            StorageArea::Local => &mut env.storage,
            StorageArea::Session => &mut env.session,
        };
        if let Some(m) = map.get_mut(&origin) {
            m.remove(key);
        }
    }
    fn storage_keys(&self, area: StorageArea) -> Vec<String> {
        let env = lock(&self.env);
        let origin = env.origin();
        match area {
            StorageArea::Local => env.storage.get(&origin),
            StorageArea::Session => env.session.get(&origin),
        }
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
    }
    fn cookie_get(&self) -> String {
        let env = lock(&self.env);
        match Url::parse(&env.url) {
            Ok(u) => env.cookie_header(&u, true),
            Err(_) => String::new(),
        }
    }
    fn cookie_set(&mut self, cookie: &str) {
        let mut env = lock(&self.env);
        if cookie
            .split(';')
            .skip(1)
            .any(|p| p.trim().eq_ignore_ascii_case("httponly"))
        {
            return;
        }
        if let Ok(u) = Url::parse(&env.url) {
            env.store_cookie(&u, cookie, true);
        }
    }
    fn log(&mut self, level: LogLevel, text: &str) {
        let mut env = lock(&self.env);
        if env.muted {
            return;
        }
        let level = match level {
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            _ => "log",
        };
        env.console.push(ConsoleEntry {
            level: level.into(),
            text: text.to_owned(),
        });
    }
}

/// The live realm and the slot its host reads.
///
/// A realm is full of `Rc`s into its thread's VM tables, so it is confined to the
/// thread that built it (`cw_web::confined`): a handle used on another thread
/// rebuilds a realm of its own there from its state, exactly as a handle the live
/// realm moved on from does, and leaves this one to its thread.
struct RealmCell {
    realm: Option<Confined<PageScript>>,
    epoch: u64,
    env: Arc<Mutex<HostEnv>>,
}

impl RealmCell {
    /// The live realm, when this thread may use it.
    fn live(&mut self) -> Option<&mut PageScript> {
        self.realm.as_mut().and_then(Confined::get_mut)
    }
}

/// What the browser mirrors out of the realm after every entry, so the chrome, the
/// observations and `refresh_pending` never have to enter the VM.
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    cw_snapshot::Encode,
    cw_snapshot::Decode,
)]
pub struct Mirror {
    /// World-clock microseconds of the earliest pending timer.
    pub next_timer: Option<i64>,
    pub wants_frame: bool,
    /// The realm's same-document history: `(index, length)`.
    pub history: (usize, usize),
    /// World tick timers last ran at (background tabs are throttled against it).
    pub last_run: u64,
    /// `visibilitychange` state last dispatched.
    pub hidden: bool,
}

struct Local {
    cell: Arc<Mutex<RealmCell>>,
    epoch: u64,
    /// The realm's state at `epoch`; always present when the cell has moved on or
    /// holds no realm.
    saved: Option<Arc<ScriptState>>,
    mirror: Mirror,
}

/// A scripted document's handle on its realm. See the module documentation.
pub struct Scripted {
    local: Mutex<Local>,
}

/// A realm's state is written as `state`, as it always was; a compiled app's as
/// `ui`.
#[derive(Serialize)]
struct ScriptedRef<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<&'a RealmState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ui: Option<&'a UiState>,
    mirror: &'a Mirror,
}
#[derive(Deserialize)]
struct ScriptedOwned {
    #[serde(default)]
    state: Option<RealmState>,
    #[serde(default)]
    ui: Option<Box<UiState>>,
    #[serde(default)]
    mirror: Mirror,
}

impl Serialize for Scripted {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let state = self.state();
        let (state, ui) = match &*state {
            ScriptState::Js(r) => (Some(r), None),
            ScriptState::Ui(u) => (None, Some(&**u)),
        };
        ScriptedRef {
            state,
            ui,
            mirror: &self.mirror(),
        }
        .serialize(s)
    }
}
impl<'de> Deserialize<'de> for Scripted {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let o = ScriptedOwned::deserialize(d)?;
        let state = match (o.state, o.ui) {
            (_, Some(ui)) => ScriptState::Ui(ui),
            (Some(r), None) => ScriptState::Js(r),
            (None, None) => return Err(serde::de::Error::missing_field("state")),
        };
        Ok(Scripted::from_state(state, o.mirror))
    }
}
/// A realm's state, or a compiled app's, then the mirror: what serde writes, in a
/// tag instead of two optional fields.
impl cw_snapshot::Encode for Scripted {
    fn encode(&self, w: &mut cw_snapshot::Writer) {
        match &*self.state() {
            ScriptState::Js(realm) => {
                w.byte(0);
                cw_snapshot::Encode::encode(realm, w);
            }
            ScriptState::Ui(ui) => {
                w.byte(1);
                cw_snapshot::Encode::encode(ui, w);
            }
        }
        cw_snapshot::Encode::encode(&self.mirror(), w);
    }
}
impl cw_snapshot::Decode for Scripted {
    fn decode(r: &mut cw_snapshot::Reader<'_>) -> std::result::Result<Self, cw_snapshot::Error> {
        let state = match r.byte()? {
            0 => ScriptState::Js(r.nested(cw_snapshot::Decode::decode)?),
            1 => ScriptState::Ui(cw_snapshot::Decode::decode(r)?),
            b => return Err(r.error(format!("{b} is not a script state"))),
        };
        Ok(Scripted::from_state(state, cw_snapshot::Decode::decode(r)?))
    }
}
impl Clone for Scripted {
    fn clone(&self) -> Self {
        let state = self.state();
        let local = lock(&self.local);
        Scripted {
            local: Mutex::new(Local {
                cell: local.cell.clone(),
                epoch: local.epoch,
                saved: Some(state),
                mirror: local.mirror.clone(),
            }),
        }
    }
}
impl PartialEq for Scripted {
    fn eq(&self, o: &Self) -> bool {
        self.mirror() == o.mirror() && *self.state() == *o.state()
    }
}
impl Eq for Scripted {}

impl Scripted {
    /// A new realm for `html` at `url`; nothing has run yet.
    pub fn new(html: &str, url: &str, viewport: Viewport, now: u64) -> Scripted {
        let env = Arc::new(Mutex::new(HostEnv {
            viewport,
            now,
            url: url.to_owned(),
            ..HostEnv::default()
        }));
        let mut realm = Realm::new(html, url, Box::new(BrowserHost { env: env.clone() }));
        realm.set_step_budget(STEP_BUDGET);
        Self::with_script(PageScript::Js(Box::new(realm)), env)
    }

    /// A compiled TSX app (`module`, its IR) for the page `html` at `url`; nothing
    /// has rendered yet. `Err` when the IR does not fit the page (no container).
    pub fn new_compiled(
        module: cw_ui::ir::Module,
        html: &str,
        url: &str,
        viewport: Viewport,
        now: u64,
    ) -> std::result::Result<Scripted, cw_ui::UiError> {
        let env = Arc::new(Mutex::new(HostEnv {
            viewport,
            now,
            url: url.to_owned(),
            ..HostEnv::default()
        }));
        let app = UiApp::new(
            module,
            html,
            url,
            Box::new(BrowserHost { env: env.clone() }),
        )?;
        Ok(Self::with_script(PageScript::Ui(Box::new(app)), env))
    }

    fn with_script(script: PageScript, env: Arc<Mutex<HostEnv>>) -> Scripted {
        let saved = Self::kept(&script);
        let cell = RealmCell {
            realm: Some(Confined::new(script)),
            epoch: 1,
            env,
        };
        Scripted {
            local: Mutex::new(Local {
                cell: Arc::new(Mutex::new(cell)),
                epoch: 1,
                saved,
                mirror: Mirror::default(),
            }),
        }
    }

    /// The state a handle keeps beside its live script after every change: none for
    /// a realm, whose state can be read from any thread (`snapshot_without_heap`);
    /// a compiled app's snapshot, since its state is its heap and the handle may
    /// next be used on another thread, where that heap cannot be read.
    fn kept(script: &PageScript) -> Option<Arc<ScriptState>> {
        script.is_compiled().then(|| Arc::new(script.snapshot()))
    }

    fn from_state(state: ScriptState, mirror: Mirror) -> Scripted {
        let cell = RealmCell {
            realm: None,
            epoch: 0,
            env: Arc::new(Mutex::new(HostEnv::default())),
        };
        Scripted {
            local: Mutex::new(Local {
                cell: Arc::new(Mutex::new(cell)),
                epoch: 0,
                saved: Some(Arc::new(state)),
                mirror,
            }),
        }
    }

    pub fn mirror(&self) -> Mirror {
        lock(&self.local).mirror.clone()
    }

    pub fn update_mirror(&self, f: impl FnOnce(&mut Mirror)) {
        f(&mut lock(&self.local).mirror)
    }

    /// The realm's serialisable state as of this handle's epoch.
    pub fn state(&self) -> Arc<ScriptState> {
        let mut local = lock(&self.local);
        if let Some(s) = &local.saved {
            return s.clone();
        }
        let state = {
            let cell = lock(&local.cell);
            let realm = cell
                .realm
                .as_ref()
                .expect("an unsaved handle owns the live realm");
            Arc::new(match realm.get() {
                Some(script) => script.snapshot(),
                None => Self::state_off_thread(realm),
            })
        };
        local.saved = Some(state.clone());
        state
    }

    /// The live script's state, read on a thread other than its own.
    fn state_off_thread(realm: &Confined<PageScript>) -> ScriptState {
        // SAFETY: `snapshot_without_heap` reads the realm's journal and plain
        // state, never its heap; a compiled app (whose state is its heap) is never
        // here, since its handle keeps its state beside it (`kept`).
        unsafe { realm.get_unchecked() }
            .snapshot_without_heap()
            .expect("a compiled app's handle keeps its state")
    }

    /// How many inputs the realm's journal holds (the replay cost of a restore; a
    /// compiled app restores without replay).
    pub fn journal_len(&self) -> usize {
        PageScript::journal_len(&self.state())
    }

    /// Whether a compiled app (not a JS realm) runs the page.
    pub fn is_compiled(&self) -> bool {
        let saved = lock(&self.local).saved.clone();
        match saved {
            Some(s) => matches!(*s, ScriptState::Ui(_)),
            None => self.read(|r| r.is_compiled()),
        }
    }

    /// Drops the live realm, keeping its state: what a document left behind in the
    /// history costs is its journal, not a VM heap.
    pub fn suspend(&self) {
        let _ = self.state();
        let local = lock(&self.local);
        let mut cell = lock(&local.cell);
        if cell.epoch == local.epoch {
            // Dropped here, or handed back to its own thread (`Confined`).
            cell.realm = None;
        }
    }

    /// Makes sure this handle's cell holds the realm of this handle's epoch, live on
    /// this thread, rebuilding it by replay when the shared one moved on (or was
    /// never built, or belongs to another thread).
    fn ready(local: &mut Local) {
        let (in_sync, foreign) = {
            let cell = lock(&local.cell);
            let current = cell.epoch == local.epoch;
            match &cell.realm {
                Some(realm) if current && realm.is_home() => (true, None),
                // This handle's state is the live realm, on another thread: read
                // what can be read of it from here, and rebuild.
                Some(realm) if current && local.saved.is_none() => {
                    (false, Some(Arc::new(Self::state_off_thread(realm))))
                }
                _ => (false, None),
            }
        };
        if in_sync {
            return;
        }
        if foreign.is_some() {
            local.saved = foreign;
        }
        let state = local
            .saved
            .clone()
            .expect("a handle out of step with its cell carries its state");
        let env = Arc::new(Mutex::new(HostEnv {
            muted: true,
            ..HostEnv::default()
        }));
        let realm = PageScript::restore(&state, Box::new(BrowserHost { env: env.clone() }));
        lock(&env).muted = false;
        let fresh = RealmCell {
            realm: Some(Confined::new(realm)),
            epoch: local.epoch,
            env,
        };
        if Arc::strong_count(&local.cell) == 1 {
            *lock(&local.cell) = fresh;
        } else {
            local.cell = Arc::new(Mutex::new(fresh));
        }
    }

    /// Enters the realm to change it. `env` is moved into the host's slot for the
    /// length of `f` and handed back with what the page did to it.
    pub fn enter<T>(&self, env: &mut HostEnv, f: impl FnOnce(&mut PageScript) -> T) -> T {
        let mut local = lock(&self.local);
        Self::ready(&mut local);
        let cell_arc = local.cell.clone();
        let mut cell = lock(&cell_arc);
        let slot = cell.env.clone();
        env.fetches = 0;
        std::mem::swap(&mut *lock(&slot), env);
        let realm = cell.live().expect("ready");
        let out = f(realm);
        let (next_timer, wants_frame, history) = (
            realm.next_timer_micros(),
            realm.wants_animation_frame(),
            realm.history_position(),
        );
        let kept = Self::kept(realm);
        std::mem::swap(&mut *lock(&slot), env);
        cell.epoch += 1;
        local.epoch = cell.epoch;
        local.saved = kept;
        local.mirror.next_timer = next_timer;
        local.mirror.wants_frame = wants_frame;
        local.mirror.history = history;
        out
    }

    /// The handle's epoch: bumped by every `enter`, so it keys render caches.
    pub fn epoch(&self) -> u64 {
        lock(&self.local).epoch
    }

    /// Enters the realm to read it (paint, hit testing, projection). Layout may be
    /// flushed; nothing the page can observe changes.
    pub fn read<T>(&self, f: impl FnOnce(&mut PageScript) -> T) -> T {
        let mut local = lock(&self.local);
        Self::ready(&mut local);
        let cell_arc = local.cell.clone();
        let mut cell = lock(&cell_arc);
        f(cell.live().expect("ready"))
    }
}

impl std::fmt::Debug for Scripted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scripted")
            .field("mirror", &self.mirror())
            .finish()
    }
}
