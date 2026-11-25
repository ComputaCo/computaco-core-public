//! No dead controls. Every page of every skin is swept: every link is followed, every form
//! and `formaction` is probed against the service's own router, and every page is audited on
//! its own for the subtler lies — a decorative `<button>`, a field outside a form, a fake
//! `role`, an inert element drawn with a pointer cursor or lighting up under one.
//!
//! The sweep only sees this site. A link that leaves it is checked in the second test: it
//! has to name a host some site in this world actually answers on.
use cw_service_common::testkit::{
    assert_off_site_links_answer, assert_ready_controls_work, sweep_sites,
};
use cw_service_forum::ForumService;
use serde_json::Value;

const SERVICE: ForumService = ForumService;
const INSTANCE: &str = "forum";
const ACTORS: [&str; 2] = ["alice", "bob"];
const SITES: [&str; 6] = [
    "hackernews",
    "reddit",
    "stackoverflow",
    "quora",
    "yelp",
    "craigslist",
];
/// Where a sweep starts. Everything else is reached by following links.
const SEEDS: &[&str] = &["/", "/newest", "/submit", "/search", "/r"];
/// Links that may land on the page they sit on, because the real product's do too.
const ALLOW_SELF: &[&str] = &["nav-home", "brand"];

#[test]
#[cfg_attr(
    not(feature = "reference-world"),
    ignore = "reads the reference company world, which is not public"
)]
fn no_page_of_any_seeded_site_draws_a_control_that_cannot_act() {
    let faults = sweep_sites(&SERVICE, INSTANCE, &SITES, &ACTORS, SEEDS, ALLOW_SELF);
    assert!(faults.is_empty(), "{}", faults.join("\n"));
}

/// The pages that carry a link off the site: a submission's URL and a member's site.
fn off_site_pages(seed: &Value) -> Vec<String> {
    let mut out: Vec<String> = vec!["/".into(), "/newest".into()];
    out.extend(
        seed["members"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(h, _)| format!("/u/{h}")),
    );
    out.extend(
        seed["threads"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(id, _)| format!("/questions/{id}")),
    );
    out
}

#[test]
#[cfg_attr(
    not(feature = "reference-world"),
    ignore = "reads the reference company world, which is not public"
)]
fn a_link_that_leaves_the_site_names_a_host_this_world_answers_on() {
    assert_off_site_links_answer(&SERVICE, INSTANCE, &SITES, off_site_pages);
}

#[test]
#[cfg_attr(
    not(feature = "reference-world"),
    ignore = "reads the reference company world, which is not public"
)]
fn every_control_that_needs_no_typing_works_when_it_is_pressed() {
    assert_ready_controls_work(&SERVICE, INSTANCE, &SITES, &ACTORS, off_site_pages, 20);
}
