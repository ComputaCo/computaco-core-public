//! A phone is a supported viewport in this world, so every page of every skin has to be
//! readable on one. Each page is laid out at the two widths a phone actually has and the
//! laid-out content may not be wider than the screen: a page that overflows sideways is
//! clipped on both edges, and the text in the middle column loses its first words on every
//! line.
use cw_service_common::testkit::{assert_fits_phones, load, request};
use cw_service_forum::ForumService;

const SITES: [&str; 6] = [
    "hackernews",
    "reddit",
    "stackoverflow",
    "quora",
    "yelp",
    "craigslist",
];

#[test]
#[cfg_attr(
    not(feature = "reference-world"),
    ignore = "reads the reference company world, which is not public"
)]
fn every_page_of_every_skin_fits_a_phone() {
    for site in SITES {
        let (domain, mut state) = load(&ForumService, "forum", site);
        let seed = state.clone();
        let mut paths = vec![
            "/".to_owned(),
            "/newest".to_owned(),
            "/submit".to_owned(),
            "/search?q=a".to_owned(),
            "/r".to_owned(),
        ];
        for (id, _) in seed["threads"].as_object().into_iter().flatten() {
            paths.push(format!("/questions/{id}"));
        }
        for (handle, _) in seed["members"].as_object().into_iter().flatten() {
            paths.push(format!("/u/{handle}"));
        }
        for (board, _) in seed["boards"]
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(b, _)| !b.is_empty())
        {
            paths.push(format!("/r/{board}"));
        }
        let mut pages = Vec::new();
        for path in &paths {
            let url = format!("http://{domain}{path}");
            let (status, html) = request(
                &ForumService,
                "forum",
                &mut state,
                "alice",
                &domain,
                "GET",
                path,
                "",
            );
            assert_eq!(status, 200, "{url}");
            pages.push((path.clone(), html));
        }
        assert_fits_phones(site, &pages);
    }
}
