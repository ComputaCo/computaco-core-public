//! A page script's requests to another origin follow the browser's rules: same-origin by
//! default for what the page may read, CORS for the rest (simple requests checked on the
//! way back, the others preflighted), credentials by mode, and opaque `no-cors` answers.
//! The transport here is two hosts: the page's own (`app.test`) and an API on another
//! origin (`api.test`) whose paths answer with or without CORS headers.
use cw_browser::BrowserState;
use cw_protocol::{HttpRequest, HttpResponse, Result};
use cw_scene::Scene;

const W: u32 = 800;
const H: u32 = 600;
const APP: &str = "https://app.test";

const PAGE: &str = r#"<!DOCTYPE html><title>Cross-origin</title><div id=out></div>
<script>
function say(t) { var p = document.createElement('p'); p.textContent = t; document.getElementById('out').appendChild(p); }
function run(name, p) { p.then(function (v) { say(name + ': ' + v); }, function (e) { say(name + ': blocked ' + e.name); }); }
var api = 'https://api.test';
run('open', fetch(api + '/open').then(function (r) { return r.text().then(function (t) { return r.type + ' ' + t + ' secret=' + r.headers.get('x-secret') + ' count=' + r.headers.get('x-count'); }); }));
run('closed', fetch(api + '/closed').then(function (r) { return r.text(); }));
run('put', fetch(api + '/put', { method: 'PUT', headers: { 'X-Token': 't1', 'Content-Type': 'application/json' }, body: '{}' }).then(function (r) { return r.text(); }));
run('delete', fetch(api + '/put', { method: 'DELETE' }).then(function (r) { return r.text(); }));
run('wildcard-cred', fetch(api + '/open', { credentials: 'include' }).then(function (r) { return r.text(); }));
run('cred', fetch(api + '/cred', { credentials: 'include' }).then(function (r) { return r.text(); }));
run('opaque', fetch(api + '/closed', { mode: 'no-cors' }).then(function (r) { return r.type + ' ' + r.status; }));
run('same-origin-mode', fetch(api + '/open', { mode: 'same-origin' }).then(function (r) { return r.text(); }));
run('own', fetch('/own').then(function (r) { return r.text().then(function (t) { return t + ' set-cookie=' + r.headers.get('set-cookie'); }); }));
var x = new XMLHttpRequest();
x.open('GET', api + '/closed');
x.onload = function () { say('xhr: ' + x.responseText); };
x.onerror = function () { say('xhr: blocked'); };
x.send();
var y = new XMLHttpRequest();
y.open('GET', api + '/open');
y.onload = function () { say('xhr-open: ' + y.responseText); };
y.send();
</script>"#;

#[derive(Default)]
struct Hosts {
    seen: Vec<HttpRequest>,
}

impl Hosts {
    fn serve(&mut self, r: HttpRequest) -> Result<HttpResponse> {
        self.seen.push(r.clone());
        let url = url::Url::parse(&r.url).unwrap();
        let mut resp = HttpResponse::text(200, "");
        let mut set = |k: &str, v: &str| {
            resp.headers.insert(k.into(), v.into());
        };
        match (url.host_str().unwrap(), url.path(), r.method.as_str()) {
            ("app.test", "/", _) => {
                set("content-type", "text/html");
                set("set-cookie", "sid=own; Path=/");
                resp.body = PAGE.as_bytes().to_vec();
            }
            ("app.test", "/own", _) => {
                set("set-cookie", "seen=1; Path=/");
                resp.body = b"own".to_vec();
            }
            ("api.test", "/open", _) => {
                set("access-control-allow-origin", "*");
                set("access-control-expose-headers", "X-Count");
                set("x-count", "3");
                set("x-secret", "s");
                resp.body = b"open body".to_vec();
            }
            ("api.test", "/closed", _) => resp.body = b"closed body".to_vec(),
            ("api.test", "/put", "OPTIONS") => {
                set("access-control-allow-origin", APP);
                set("access-control-allow-methods", "GET, PUT");
                set("access-control-allow-headers", "x-token, content-type");
            }
            ("api.test", "/put", _) => {
                set("access-control-allow-origin", APP);
                resp.body = format!("{} done", r.method).into_bytes();
            }
            ("api.test", "/cred", _) => {
                set("access-control-allow-origin", APP);
                set("access-control-allow-credentials", "true");
                set("set-cookie", "api=1; Path=/");
                let cookie = r.headers.get("cookie").cloned().unwrap_or_default();
                resp.body = format!("cookie=[{cookie}]").into_bytes();
            }
            _ => resp.status = 404,
        }
        Ok(resp)
    }
}

fn texts(scene: &Scene) -> Vec<String> {
    scene
        .nodes
        .iter()
        .filter_map(|n| serde_json::to_value(&n.primitive).ok())
        .filter_map(|v| v.get("text")?.as_str().map(str::to_owned))
        .collect()
}

#[test]
fn a_scripts_cross_origin_requests_follow_cors() {
    let mut hosts = Hosts::default();
    let mut b = BrowserState::default();
    b.set_entropy(42, "computer/test/browser");
    b.set_viewport(W, H);
    b.navigate(&format!("{APP}/"), &mut |r| hosts.serve(r))
        .unwrap();
    let shown = texts(&b.scene(W, H)).join("\n");
    let line = |name: &str| {
        shown
            .lines()
            .find(|l| l.starts_with(&format!("{name}: ")))
            .unwrap_or_else(|| panic!("no line {name} in:\n{shown}"))
            .to_owned()
    };
    // Allowed by `*`: the body, and only the headers the other origin exposes.
    assert_eq!(line("open"), "open: cors open body secret=null count=3");
    // A simple request goes out, but its answer is not the page's without the header.
    assert_eq!(line("closed"), "closed: blocked TypeError");
    // Preflighted, allowed: the method and both headers were asked about.
    assert_eq!(line("put"), "put: PUT done");
    // Preflighted, refused: DELETE is not among the allowed methods.
    assert_eq!(line("delete"), "delete: blocked TypeError");
    // Credentials never go to an origin that answers `*`.
    assert_eq!(line("wildcard-cred"), "wildcard-cred: blocked TypeError");
    assert_eq!(line("cred"), "cred: cookie=[]");
    assert_eq!(line("opaque"), "opaque: opaque 0");
    assert_eq!(
        line("same-origin-mode"),
        "same-origin-mode: blocked TypeError"
    );
    // The page's own origin: readable, but `Set-Cookie` is never shown to a script.
    assert_eq!(line("own"), "own: own set-cookie=null");
    assert_eq!(line("xhr"), "xhr: blocked");
    assert_eq!(line("xhr-open"), "xhr-open: open body");

    let preflight = hosts
        .seen
        .iter()
        .find(|r| r.method == "OPTIONS" && r.url.ends_with("/put"))
        .expect("a preflight for PUT");
    assert_eq!(preflight.headers["origin"], APP);
    assert_eq!(preflight.headers["access-control-request-method"], "PUT");
    assert_eq!(
        preflight.headers["access-control-request-headers"],
        "content-type,x-token"
    );
    assert!(!preflight.headers.contains_key("cookie"));
    // The refused DELETE was asked about and never sent.
    assert!(hosts
        .seen
        .iter()
        .any(|r| r.method == "OPTIONS" && r.headers["access-control-request-method"] == "DELETE"));
    assert!(!hosts.seen.iter().any(|r| r.method == "DELETE"));
    // Cookies of the page's origin never go to another origin; its requests carry `Origin`.
    let to_api: Vec<&HttpRequest> = hosts
        .seen
        .iter()
        .filter(|r| r.url.starts_with("https://api.test"))
        .collect();
    assert!(to_api
        .iter()
        .all(|r| r.headers.get("origin").map(String::as_str) == Some(APP)));
    assert!(to_api
        .iter()
        .all(|r| !r.headers.get("cookie").is_some_and(|c| c.contains("sid"))));
    // Same origin: the page's cookie goes, and no `Origin` header is added.
    let own = hosts.seen.iter().find(|r| r.url.ends_with("/own")).unwrap();
    assert_eq!(
        own.headers.get("cookie").map(String::as_str),
        Some("sid=own")
    );
    assert!(!own.headers.contains_key("origin"));
    // Each refusal is on the console, as Chrome says it.
    let blocked = b
        .console()
        .iter()
        .filter(|e| e.level == "error" && e.text.contains("blocked by CORS policy"))
        .count();
    assert_eq!(blocked, 5, "{:?}", b.console());
}

#[test]
fn credentials_go_to_another_origin_only_when_asked_and_allowed() {
    let mut hosts = Hosts::default();
    let mut b = BrowserState::default();
    b.set_entropy(42, "computer/test/browser");
    b.set_viewport(W, H);
    b.navigate(&format!("{APP}/"), &mut |r| hosts.serve(r))
        .unwrap();
    // The first credentialed request was given a cookie by api.test; a second one, made
    // by the page later, sends it back.
    b.navigate(&format!("{APP}/"), &mut |r| hosts.serve(r))
        .unwrap();
    let creds: Vec<&HttpRequest> = hosts
        .seen
        .iter()
        .filter(|r| r.url == "https://api.test/cred")
        .collect();
    assert_eq!(creds.len(), 2);
    assert!(!creds[0].headers.contains_key("cookie"));
    assert_eq!(
        creds[1].headers.get("cookie").map(String::as_str),
        Some("api=1")
    );
    let shown = texts(&b.scene(W, H)).join("\n");
    assert!(shown.contains("cred: cookie=[api=1]"), "{shown}");
    // A request without `include` never carried it.
    assert!(hosts
        .seen
        .iter()
        .filter(|r| r.url == "https://api.test/closed")
        .all(|r| !r.headers.contains_key("cookie")));
}
