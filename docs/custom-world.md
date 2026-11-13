# Create a world

## A desktop for one agent, ready to copy

If what you want is *one machine an agent can use* — a desktop, a browser, the
everyday applications and a couple of documents — copy the built-in `agent-desktop`
world (`GET /v1/templates/agent-desktop` returns its whole definition) instead
of writing a world from scratch. It is one Ubuntu workstation for the user `ada`, with
Terminal, Files, Text Editor, Firefox, Calculator, Calendar, Notes, Image Viewer,
LibreOffice Calc and Visual Studio Code installed, three seeded documents under
`~/Documents`, and an intranet wiki at `http://wiki.internal/` for the browser to
reach. The gateway is closed to the internet and to the host, as every world's is.

The grants an actor needs are in the definition, under `metadata.actor_session`, so the
session is a copy rather than a guess:

```python
import json
from computerworld import World
definition = json.load(open("agent-desktop.json"))       # the template's definition, saved
world = World(definition, seed=7)
env = world.environment(definition["metadata"]["actor_session"])   # hand only env to the agent
```

Over the hosted API, `POST /v1/simulations {"template": "agent-desktop"}` boots it and
`POST /v1/simulations/{id}/agents` with that `actor_session` as the body lets in an
external agent with the same session ([the hosted API](cloud-api.md)). To keep a world of
your own as a template, `POST /v1/templates {name, definition}`.

`metadata.desktop_themes` is what gives the machine a shell to draw; without it there
are windows but no desktop around them. `installed_apps` is what makes an application
launchable — an id that is not there is `not_found` with the reason
`application_not_installed`, and `application.v1 list` shows the whole catalogue with
the reason each unlaunchable entry carries.
The engine's own tests drive this world through an actor session only, so it cannot rot.

## Anything else

Start from the built-in `unrelated-lab` world, or copy the structure of the built-in
`company-2026` world and replace its fixtures (both are in `GET /v1/templates`). The kernel has no required company domain or machine.

The built-in worlds are written in a source form and resolved to ordinary world
definitions before they are served; what you send the API is always the plain JSON
definition.

1. Define OS profiles and each machine's address, user and initial files.
2. Define service nodes and explicit links from clients to those nodes.
3. Place service instances with registered kinds and state. Give them domains.
4. Declare DNS records and gateway restrictions as needed. The public web is not
   yours to declare: every world joins the built-in internet
   unless it sets `internet: false`, and a machine reaches it when its gateway's
   `allow_internet` says so. Name `edge-router` to choose the uplink yourself, or
   leave it and each machine gets one.
5. Boot it (`POST /v1/simulations {"definition": ...}` over the hosted API, after
   `POST /v1/validate` if you like, or `World(definition, seed)` in the 0.3.0 packages) and
   grant an agent only the intended machines and action families.
6. Test behavior from actor actions: resolving, navigating, mutating and reading
   from a second machine. Owner inspection is diagnostic evidence, not the actor's solution.

Keep secret evaluation answers outside the definition's actor-visible files and
service responses. Use deterministic context values for generated fixtures.
Store the definition, engine version, module versions, seed and action sequence
when sharing a reproducible episode.

The fields each of these steps fills in are listed in the
[world schema](world-schema.md). A machine's insides are
[computers](computers.md) and its screen is the [desktop](desktop-gui.md); the wiring
between machines is [networking](networking.md). Once the world runs, the
[agent API](agent-api.md) is how an actor is let into it.
