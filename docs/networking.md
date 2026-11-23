# Synthetic networking

The path is application → URL → source-aware DNS → route and gateway checks →
listener/service → HTTP response → browser page. A browser cannot bypass this path
by consulting backing service state. A second machine sees a mutation after its
own request; cached pages remain snapshots of received content.

Each computer has its own node identity and address. Network definitions contain
links, routes, DNS records, placement and gateway policy. Service instances bind
at their configured node/port and own domain names; unless a service sets
`tls: false` it also listens on 443, so `https://` reaches the same handler as
`http://` and the browser shows whichever scheme was asked for (the first
service placed on a node owns that node's 443). The network distinguishes
local, synthetic internet and host zones. DNS cache expiry and link latency use
logical microseconds; configured loss uses seeded simulation state. Links are
required unless `network.implicit_lan` is explicitly true (default false). DNS
records can carry IP literals or CNAME targets; an explicit resolver must be
reachable from the source.

`cw-network` supplies DNS resolution, reachability/policy decisions, listeners,
HTTP preparation/response traces, stream connection buffers and datagrams.
Timed stream writes use `write_at`/`advance`/`read`; datagrams use explicit
send/receive operations. Transport is semantic rather than packet-level TCP emulation. Listener ownership
allows process cleanup. Network traces expose request/resolution/response and
failure information without host packet capture.

The kernel drives requests and service dispatch in the deterministic scheduler.
Synchronous convenience HTTP uses that same synthetic path. Use queued APIs when
an episode needs an in-flight checkpoint or separately advanced communication.

Default host egress is denied. Unknown names never fall through to host DNS. A world
reaches a real host only through its gateway policy and an embedder that makes the
exchange ([outside hosts](#outside-hosts)).
Explicit host authorization checks policy and resolved addresses before producing
adapter authorization; the default runtime does not install a real network
adapter. An allow flag alone cannot open a socket. Any future adapter must keep
redirect and address checks inside the capability boundary.

See [world schema](world-schema.md) and [host isolation](security.md). A synthetic public-looking domain does not imply
traffic reaches the real public site with that name.

The native owner's `begin_external` creates an authorized external-effect record
from an explicit address set. A separately implemented adapter performs the I/O
and returns it through `complete_external`. Generation, deadline and response-size
checks reject stale/oversized completions. These APIs do not perform DNS or fetch
on the host by themselves. Live external effects cannot be resumed as deterministic
synthetic work; complete them before creating a portable resumable checkpoint.

The optional `cw-host-adapters` crate supplies two implementations. `RecordedAdapter`
consumes exact request/response exchanges in order, rejects mismatches without
consuming them, and never falls back to live traffic. `NativeHttpAdapter` is
available only with feature `native-http` on native targets. Its reqwest client
pins authorized addresses, disables proxy discovery and automatic redirects,
rejects caller Host overrides and enforces timeout/response-byte limits. The
owner must resolve candidate addresses and authorize **every** address before
calling it; the adapter does not bypass `begin_external`/`complete_external`.
Reauthorize redirects as new effects instead of following them automatically. The adapter
is built and tested separately from the simulation.

Live HTTP results are external input, so reproducible replay must supply recorded
exchanges. `Runtime::try_snapshot` rejects unresolved live effects. The default
simulation and browser Wasm artifacts do not depend on reqwest.

## Outside hosts

A world reaches a real host only when its definition's gateway names it. Example:

```yaml
network:
  gateway:
    allow_host: true
    host_allowlist: [api.github.com, example.com]   # or ["*"]: any name the world itself does not define
    sources: [workstation]                          # the nodes whose machines may reach out
    schemes: [https]
    ports: [443]
    allowed_cidrs: ["0.0.0.0/0", "::/0"]
    max_response_bytes: 1048576
```

A request leaves the world when the gateway admits its source, host, port and scheme, and
the world's own DNS has no record for the name (exact or wildcard). A name the world
defines always stays inside, even under `"*"`. An IP literal or `localhost` never leaves.
`World::is_external` gives this decision.

The embedder makes each exchange before the step that consumes it. It resolves the name,
gets the gateway's check of every address (`World::authorize_external`), sends the
request, and passes the answer to `World::step_external`. A request is answered from a
supplied exchange with the same machine, method, URL and body; the trajectory records
`external.submitted` and `external.completed`. The journal keeps the exchanges the step
consumed, in the order it consumed them (`RecordedStep::external`). A replay or a fork
answers the same requests the same way and fetches nothing. A request with no exchange
supplied fails with `outside_unavailable`. A refused address fails with `outside_refused`.
A host that did not answer, or answered too much, fails with `outside_failed`.

Most requests are known only while the step runs: a page the world's browser opens asks
for its stylesheets, scripts and pictures, its scripts make `fetch` and `XMLHttpRequest`
calls, and a command's `curl` runs inside the step. So the embedder finds them by running
the step:

1. Take a snapshot (`World::snapshot`; the state is shared, not copied).
2. Run the step with the exchanges made so far.
3. `World::take_wanted` lists each outside request the step made with no exchange
   supplied (`{machine, request}`, each once). If it is empty, the step stands.
4. Otherwise restore the snapshot, make those exchanges, add them to the set, and go to 1.

A page needs one run for itself, one for what it links, and one more for each level of
what its scripts load (the page in `crates/computerworld/tests/outside_pages.rs` takes four
runs). Only the last run counts: its journal entry holds every answer it used, and a replay
does not run the rounds again. `World::opens_outside` says whether a world's gateway names
any host at all; a world whose gateway names none never needs the snapshot.

The world's browser follows a redirect itself, and each hop is its own exchange. Cookies a
real site sets live in the browser's state like any other cookie, so a snapshot, a fork and
a replay keep them.

A page script's requests follow a browser's rules (`crates/web/browser/src/scripted.rs`).
A request to the page's own origin carries the origin's cookies. A request to another
origin is a CORS request and carries `Origin`. A simple one (GET, HEAD or POST with only
safelisted headers) goes out, and its answer reaches the page only when
`Access-Control-Allow-Origin` names the page's origin or is `*`. Any other is first asked
about with an `OPTIONS` preflight, whose answer must allow its method
(`Access-Control-Allow-Methods`) and its headers (`Access-Control-Allow-Headers`).
`credentials: 'include'` (an XHR's `withCredentials`) sends that origin's cookies and keeps
its `Set-Cookie`; the answer must then name the origin exactly and say
`Access-Control-Allow-Credentials: true`. A CORS answer shows the page only the safelisted
headers and those in `Access-Control-Expose-Headers`. `mode: 'same-origin'` refuses another
origin, and `mode: 'no-cors'` gets an opaque answer (status 0, no body). No answer shows a
script its `Set-Cookie`, and a script cannot set the Fetch standard's forbidden request
headers (`Cookie`, `Origin`, `Host`, `Referer` and the rest). A refusal is a network error
to the page (`TypeError: Failed to fetch`, an XHR's `error` event) and a line on the tab's
console, worded as Chrome words it. There is no preflight cache: each request that needs a
preflight sends one. The page's own loads (`<script src>`, stylesheets, `@import`, modules,
pictures) go to any origin the transport allows, with cookies only to the page's own. These
rules hold inside the world's own internet too.

In the hosted API, every step of a simulation whose gateway names a host is run this way.
The platform adds rules that no definition can loosen:

- Only public addresses. A name that resolves to a private, loopback, link-local or shared
  address is refused.
- Never the platform's own hosts.
- The platform follows no redirect. The browser does, hop by hop; `http.v1` and `curl` get
  the 3xx as their answer.
- At most 15 s and 4 MiB per answer. A step makes at most 96 requests, receives about
  24 MiB, spends at most 30 s reaching out and runs at most 8 times. A simulation makes at
  most 600 requests a minute. Six requests go at once. A request past a limit fails with
  `outside_failed` and says which limit, and is not asked for again in that step.
- An operator can stop all of it with `PUT /v1/admin/outside {"requests": false}`.

The step's answer says what reaching out took: `outside: {rounds, requests, bytes,
elapsed_ms}`. Measured on 2026-11-22 through a local API on a developer machine
(`cloud/scripts/ingress/real-page.mjs`, one `browser.v1 navigate` each):

| Page | Runs | Requests | Bytes | Step |
|---|---|---|---|---|
| `https://example.com/` | 3 | 2 | 2,540 | 2.9 s |
| `https://news.ycombinator.com/` | 4 | 6 | 47,207 | 2.7 s |
| `https://en.wikipedia.org/wiki/Web_browser` | 6 | 79 | 1,912,925 | 17.1 s |

Requests made outside a step are not answered: a page's timer that fetches while the world
clock advances with no step fails with `outside_unavailable`, and the page sees a network
error.
