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

The embedder makes the exchange before the step that asks for it. It resolves the name,
gets the gateway's check of every address (`World::authorize_external`), sends the
request, and passes the answer to `World::step_external`. The agent's own `http.v1
request` (or a command's `curl` through the same path) is answered from that exchange, and
the trajectory records `external.submitted` and `external.completed`. The journal keeps
each exchange with its step (`RecordedStep::external`). A replay or a fork answers the same
request the same way and fetches nothing. A request with no exchange supplied fails with
`outside_unavailable`. A refused address fails with `outside_refused`. A host that did not
answer, or answered too much, fails with `outside_failed`.

In the hosted API, every `http.v1 request` in an agent's step is made this way. The platform
adds rules that no definition can loosen:

- Only public addresses. A name that resolves to a private, loopback, link-local or shared
  address is refused.
- Never the platform's own hosts.
- A redirect is not followed. The 3xx is the answer, as it is for `curl`.
- At most 15 s and 4 MiB per answer. At most 8 requests a step and 120 a minute per
  simulation.
- An operator can stop all of it with `PUT /v1/admin/outside {"requests": false}`.

A command's `curl` cannot be fetched before its step runs, so in the hosted API it fails with
`outside_unavailable`.

