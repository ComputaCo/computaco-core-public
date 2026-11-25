//! What a service's tests share: a request against the service with a scratch copy for probes, the reference site
//! loaded and initialised, every host this world answers on, the forms a page lets be pressed with nothing typed,
//! how wide a page lays out on a phone, and (feature `stills`) a page rendered to a PNG for the record. The open
//! services' tests and the look-alike services' (in another repository) call these, so the checks have one home.
use crate::audit::Sweep;
use cw_protocol::HttpRequest;
use cw_sdk::{Service, ServiceContext};
use cw_web::css::{parse_stylesheet, MatchContext, Media, Origin};
use cw_web::dom::Document;
use cw_web::layout::fragment::{Fragment, FragmentKind};
use cw_web::{Strictness, Viewport};
use serde_json::Value;
use std::collections::BTreeSet;

/// The phone widths this world's device set uses, and a small tablet.
pub const PHONE_WIDTHS: [u32; 3] = [390, 412, 768];

/// The context a test acts in: `actor` on alice's Mac, at tick 12.
pub fn ctx(actor: &str, instance: &str) -> ServiceContext {
    ServiceContext {
        actor: actor.into(),
        source: "alice-mac".into(),
        tick: 12,
        seed: 1,
        instance: instance.into(),
    }
}

/// The reference world's site `name` (its overlay applied), as its file has it.
pub fn site(name: &str) -> Value {
    let path = crate::reference::reference_site_path(name);
    serde_json::from_str(&std::fs::read_to_string(&path).expect(&path)).unwrap()
}

/// The site's first host and its state, initialised by `service` as alice.
pub fn load<S: Service>(service: &S, instance: &str, name: &str) -> (String, Value) {
    let file = site(name);
    let host = file["domains"][0].as_str().unwrap().to_ascii_lowercase();
    let state = service
        .initialize(file["initial_state"].clone(), &ctx("alice", instance))
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    (host, state)
}

/// One request, with a urlencoded body when it is a `POST`.
#[allow(clippy::too_many_arguments)]
pub fn request<S: Service>(
    service: &S,
    instance: &str,
    state: &mut Value,
    actor: &str,
    host: &str,
    method: &str,
    path: &str,
    body: &str,
) -> (u16, String) {
    let mut r = HttpRequest::get(format!("http://{host}{path}"));
    if method == "POST" {
        r.method = "POST".into();
        r.headers.insert(
            "content-type".into(),
            "application/x-www-form-urlencoded".into(),
        );
        r.body = body.as_bytes().to_vec();
    }
    let reply = service
        .handle(state, &ctx(actor, instance), &r)
        .unwrap_or_else(|e| panic!("{method} {path}: {e}"));
    (
        reply.status,
        String::from_utf8(reply.body).unwrap_or_default(),
    )
}

/// A sweep's caller. A `POST` probe runs against a scratch copy, so a sweep that presses every button on the site
/// does not change what the next page it reads says.
pub fn caller<'a, S: Service>(
    service: &'a S,
    instance: &'a str,
    state: &'a mut Value,
    actor: &'a str,
    host: &'a str,
) -> impl FnMut(&str, &str) -> (u16, String) + 'a {
    move |method: &str, path: &str| {
        if method == "POST" {
            let mut scratch = state.clone();
            request(
                service,
                instance,
                &mut scratch,
                actor,
                host,
                method,
                path,
                "",
            )
        } else {
            request(service, instance, state, actor, host, method, path, "")
        }
    }
}

/// Sweeps every site in `sites` as every actor from `seeds` and returns each one's faults (`Sweep`).
pub fn sweep_sites<S: Service>(
    service: &S,
    instance: &str,
    sites: &[&str],
    actors: &[&str],
    seeds: &[&str],
    allow_self: &[&str],
) -> Vec<String> {
    let mut out = Vec::new();
    for name in sites {
        let (host, seed) = load(service, instance, name);
        for actor in actors {
            let mut state = seed.clone();
            let mut call = caller(service, instance, &mut state, actor, &host);
            let faults = Sweep::new(seeds, &mut call).allow_self(allow_self).run();
            if !faults.is_empty() {
                out.push(format!("{name} as {actor}:\n  {}", faults.join("\n  ")));
            }
        }
    }
    out
}

/// Every host this world answers on: the reference company's own sites and the internet's.
pub fn world_domains() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for dir in [
        crate::reference::company_services(),
        crate::reference::internet_services(),
    ] {
        for entry in std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .flatten()
        {
            let Ok(text) = std::fs::read_to_string(entry.path().join("service.json")) else {
                continue;
            };
            let Ok(file) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            for domain in file["domains"].as_array().into_iter().flatten() {
                if let Some(d) = domain.as_str() {
                    out.insert(d.to_ascii_lowercase());
                }
            }
        }
    }
    assert!(out.len() > 20, "the world's site files must be readable");
    out
}

/// The forms on a page that need nothing typed into them: every field is hidden, or a text field the page already
/// filled. `(id, method, action, encoded fields)`.
pub fn ready_forms(body: &str) -> Vec<(String, String, String, String)> {
    let doc = cw_web::html::parse(body);
    let mut out = Vec::new();
    for node in doc.descendants(Document::ROOT) {
        if !doc.is(node, "form") {
            continue;
        }
        let mut fields: Vec<(String, String)> = Vec::new();
        let mut waiting = false;
        for field in doc.descendants(node) {
            let tag = doc.tag(field).unwrap_or_default();
            if !matches!(tag, "input" | "textarea" | "select") {
                continue;
            }
            let value = match tag {
                "textarea" => doc.text_content(field),
                _ => doc.attr(field, "value").unwrap_or_default().to_owned(),
            };
            if doc.attr(field, "type") != Some("hidden") && value.trim().is_empty() {
                waiting = true;
            }
            if let Some(name) = doc.attr(field, "name") {
                fields.push((name.to_owned(), value));
            }
        }
        if waiting {
            continue;
        }
        let action = doc.attr(node, "action").unwrap_or_default();
        if action.is_empty() || !action.starts_with('/') {
            continue;
        }
        out.push((
            doc.attr(node, "id").unwrap_or_default().to_owned(),
            doc.attr(node, "method")
                .unwrap_or("get")
                .to_ascii_uppercase(),
            action.to_owned(),
            url::form_urlencoded::Serializer::new(String::new())
                .extend_pairs(fields.iter().map(|(k, v)| (k.as_str(), v.as_str())))
                .finish(),
        ));
    }
    out
}

fn laid_out(
    html: &str,
    viewport: Viewport,
) -> (
    Document,
    cw_web::style::StyleSet,
    cw_web::layout::fragment::FragmentTree,
) {
    let doc = cw_web::html::parse(html);
    let mut sheets = Vec::new();
    for node in doc.descendants(Document::ROOT) {
        if doc.is(node, "style") {
            sheets.push(
                parse_stylesheet(&doc.text_content(node), Origin::Author, Strictness::Strict)
                    .unwrap(),
            );
        }
    }
    let media = Media::with_size(viewport.width as i32, viewport.height as i32);
    let styles = cw_web::style::cascade(
        &doc,
        &sheets,
        &media,
        &MatchContext::new(),
        Strictness::Strict,
    )
    .unwrap();
    let tree = cw_web::layout::layout(&doc, &styles, viewport);
    (doc, styles, tree)
}

/// How far right the page's boxes reach when it is laid out `width` pixels wide, and which box reaches furthest.
/// Boxes, not text runs: a preserved trailing space hangs past the end of its line by design and is not what clips
/// a page.
pub fn widest_box(html: &str, width: u32) -> (f64, String) {
    let viewport = Viewport {
        width,
        height: 844,
        scale: 1,
        zoom: 100,
    };
    let (doc, _, tree) = laid_out(html, viewport);
    fn visit(doc: &Document, f: &Fragment, ox: f64, worst: &mut (f64, String)) {
        let x = ox + f.rect.origin.x.to_f64_px();
        // What sits inside a scroll box of the page's own — a `<pre>` of code, a rail that scrolls — is reachable by
        // scrolling it, so it is not what clips the page. The document's own scroll box is not one of those.
        if let FragmentKind::Box {
            scroll: Some(_),
            source,
            ..
        } = &f.kind
        {
            let tag = doc.tag(source.node()).unwrap_or("");
            if !matches!(tag, "" | "html" | "body") {
                return;
            }
        }
        if let FragmentKind::Box { source, .. } | FragmentKind::InlineBox { source, .. } = &f.kind {
            let right = x + f.rect.size.width.to_f64_px();
            if right > worst.0 {
                let node = source.node();
                *worst = (
                    right,
                    format!(
                        "<{}{}{}>",
                        doc.tag(node).unwrap_or("?"),
                        doc.attr(node, "id")
                            .map(|i| format!(" id={i:?}"))
                            .unwrap_or_default(),
                        doc.attr(node, "class")
                            .map(|c| format!(" class={c:?}"))
                            .unwrap_or_default()
                    ),
                );
            }
        }
        for child in &f.children {
            visit(doc, child, x, worst);
        }
    }
    let mut worst = (0.0, String::new());
    visit(&doc, &tree.root, 0.0, &mut worst);
    worst
}

/// Asserts that every page in `pages` (a URL and its HTML) fits each phone width.
pub fn assert_fits_phones(site: &str, pages: &[(String, String)]) {
    for (path, body) in pages {
        for width in PHONE_WIDTHS {
            let (right, which) = widest_box(body, width);
            assert!(
                right <= f64::from(width) + 0.5,
                "{site} {path} at {width}px wide: {which} reaches {right}px, past the edge of the screen — the page \
                 is clipped on a phone"
            );
        }
    }
}

/// `html` laid out, painted and rendered through the engine, as PNG bytes.
#[cfg(feature = "stills")]
pub fn still_png(html: &str, viewport: Viewport) -> Vec<u8> {
    let (doc, styles, tree) = laid_out(html, viewport);
    let scene = cw_web::paint::paint(
        &doc,
        &styles,
        &tree,
        viewport,
        &cw_web::paint::PaintContext::default(),
    );
    let frame = cw_render::Renderer::new().render(&scene);
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, frame.width, frame.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&frame.rgba).unwrap();
    }
    out
}

/// Renders `html` through the engine to the PNG `target`, for the record (a still, not a gate).
#[cfg(feature = "stills")]
pub fn render_still(html: &str, target: &std::path::Path, viewport: Viewport) {
    let out = still_png(html, viewport);
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(target, out).unwrap_or_else(|e| panic!("write {}: {e}", target.display()));
    println!("wrote {}", target.display());
}

/// Every link that leaves a site, on the pages `off_site_pages` names, points at a host some site in this world
/// answers on. Returns how many links it checked; none is a fault of the seeds.
pub fn assert_off_site_links_answer<S: Service>(
    service: &S,
    instance: &str,
    sites: &[&str],
    off_site_pages: impl Fn(&Value) -> Vec<String>,
) {
    let domains = world_domains();
    let mut checked = 0;
    for name in sites {
        let (host, seed) = load(service, instance, name);
        let mut state = seed.clone();
        let mut call = caller(service, instance, &mut state, "alice", &host);
        for path in off_site_pages(&seed) {
            let (status, body) = call("GET", &path);
            if status != 200 {
                continue;
            }
            for control in crate::audit::controls(&cw_web::html::parse(&body)) {
                let Some(rest) = control
                    .target
                    .strip_prefix("http://")
                    .or_else(|| control.target.strip_prefix("https://"))
                else {
                    continue;
                };
                let authority = rest.split('/').next().unwrap_or("").to_ascii_lowercase();
                assert!(
                    domains.contains(&authority),
                    "{name} {path}: #{} points at {}, and no site in this world answers on {authority}",
                    control.id,
                    control.target
                );
                checked += 1;
            }
        }
    }
    assert!(
        checked > 0,
        "no off-site link was checked: the seeds must carry some"
    );
}

/// A control with nothing to fill in — like, follow, vote, join, save, remove, checkout — must not merely avoid a
/// 404: pressing it has to work. Every form on the pages `pages` names whose fields are all hidden or already filled
/// is submitted with exactly what the page put in it, on a scratch copy of the state, and the answer has to be a page
/// rather than a refusal. At least `least` controls must be pressed.
pub fn assert_ready_controls_work<S: Service>(
    service: &S,
    instance: &str,
    sites: &[&str],
    actors: &[&str],
    pages: impl Fn(&Value) -> Vec<String>,
    least: usize,
) {
    let mut pressed = 0;
    for name in sites {
        let (host, seed) = load(service, instance, name);
        for actor in actors {
            let mut state = seed.clone();
            for path in pages(&seed) {
                let (status, body) = request(
                    service, instance, &mut state, actor, &host, "GET", &path, "",
                );
                if status != 200 || !body.starts_with("<!DOCTYPE html>") {
                    continue;
                }
                for (id, method, action, fields) in ready_forms(&body) {
                    let mut scratch = state.clone();
                    let target = if method == "GET" && !fields.is_empty() {
                        format!("{action}?{fields}")
                    } else {
                        action.clone()
                    };
                    let (status, answer) = request(
                        service,
                        instance,
                        &mut scratch,
                        actor,
                        &host,
                        &method,
                        &target,
                        &fields,
                    );
                    assert!(
                        status < 400,
                        "{name} as {actor}, {path}: pressing #{id} ({method} {action}) answers {status}: {}",
                        answer.chars().take(200).collect::<String>()
                    );
                    pressed += 1;
                }
            }
        }
    }
    assert!(
        pressed > least,
        "only {pressed} controls were pressed: the walk found too little"
    );
}
