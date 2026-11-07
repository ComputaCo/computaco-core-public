# World and topology schema

The engine's protocol types are the authoritative schema; this page describes them.
JSON uses `schema_version: 1`; unsupported versions are rejected rather than silently
interpreted. Booting a world parses and validates the definition (`POST /v1/validate`
does only that).

| Field | Meaning |
|---|---|
| `id` | Nonempty world identity |
| `profiles` | OS definitions: `id`, `name`, `family`, `home`, `case_sensitive`, `shell` |
| `computers` | Machine `id`, `profile`, `address`, `user`, optional `node`, initial files, installed applications and packages, and an optional `presentation` (`desktop`, `laptop`, `phone`, `server`) |
| `network` | Nodes, links, DNS records, routes, explicit `implicit_lan` and gateway policy |
| `services` | Instance `id`, registered `kind`, placement `node`, domains, `port` (80), `tls` (default `true`: a second listener on 443 answers `https://`), initial state, and `search_entries`, the pages it offers the world's search engines |
| `internet` | Default `true`: `World::new` joins the built-in internet — its public sites, backbone and DNS — to the world, and the world may name the internet's five `INTERNET_ATTACHMENTS` (`edge-router`, `pop-west`, `pop-east`, `pop-eu`, `dns-public`) before it has. What the world declares itself wins. `false` is a world exactly as declared, and the only kind a build without `computerworld`'s `internet` feature boots |
| `internet_overlays` | By internet site id, the world's own content on that shared site: `initial_state` merged into the site's as a JSON merge patch (objects key by key; any other value, a list included, restated whole), and `search_entries` and `domains` appended. An overlay for a site the world does not join is an error |
| `metadata` | Owner-side JSON; not an actor observation channel. Two keys are load-bearing: `desktop_themes` maps a computer to an OS shell, and `desktop_apps` declares its application catalog. Both change what an actor sees and can launch — see [desktop GUI](desktop-gui.md). `device_presentations` maps a computer to `desktop`, `laptop`, `phone` or `server` (a computer's own `presentation` wins): laptops and phones show a battery, desktop computers and servers none. A phone shell is always a phone; a computer nothing is said about is a desktop computer. `search_engine` is the template the browser's address bar sends searches to (`%s` is the urlencoded query, default `https://google.com/search?q=%s`) — see [action families](action-families.md#the-omnibox). |

A computer's node defaults to its machine ID. Explicit nodes have an address and
zone (`local`, `internet`, `host`). Links are required by default; `network.implicit_lan: true` explicitly enables
implicit local reachability. A link names `from`/`to`, direction, logical
`latency_us` and `loss_per_million`. DNS records name an IP address or CNAME target, TTL in logical
microseconds, and optional resolver node. Service domains resolve to their
placement; clients still need reachable routes and listeners.

Validation checks duplicate identities, profile and node references, address
syntax and collisions, listener collisions, DNS definitions and link parameters.
The runtime also validates registered service kinds. OS profiles describe
synthetic path/shell behavior; they do not boot the corresponding operating system.

Definitions should keep all fixtures and initial state explicit. The seed can
vary initialization where an implementation uses its deterministic context; it is
not permission to draw host randomness. Runtime state, portable snapshots and
world definitions are different artifacts: a definition constructs a world, while
a snapshot resumes one.

See [creating a world](custom-world.md) and the built-in `company-2026` world
(`GET /v1/templates/company-2026` over the hosted API).

The `company-2026` definition is **generated**, not hand-written: each service's node, link
and DNS records are derived from where it sits, and the search engines index the sites'
`search_entries` themselves when the world boots. This document describes the definition,
which is plain JSON; a world of your own is written in it.
