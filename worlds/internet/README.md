# The built-in internet

The public web every world joins when it boots, unless the world sets `internet: false`:
94 services (sites, and institutions such as the card network and the ACH operator), the
backbone they hang off, and the public DNS. A world declares only what is its
own; `World::new` adds the rest through `cw_internet::join`, and anything the world
declares itself — a service, a node, a DNS name — wins over the internet's.

A world uplinks through `edge-router`. One that names no link to it gets each of its
machines linked there. A world that runs public sites of its own hangs them off `pop-west`,
`pop-east` or `pop-eu` and resolves them through `dns-public`; those five nodes are
`cw_protocol::INTERNET_ATTACHMENTS`, the only internet names a world may use before the
internet has joined it. Whether a machine can actually reach the internet is still its
gateway's `allow_internet`.

`world.yml` is the blueprint and `world.json` beside it is generated and embedded in the
`cw-internet` crate:

    cargo run -p cw-blueprint --bin cw-world -- build worlds/internet/world.yml

It is behind `computerworld`'s default `internet` feature. A build without it is smaller by
the whole of `world.json`, and boots only worlds that set `internet: false`.

**The content is neutral.** Nobody from any particular world is on these sites: the public
is fictional people with no story, and private per-person data — mailboxes, calendars,
drives, orders — starts empty. A world adds its own people and content through
`internet_overlays`, one overlay per site — in a blueprint, `services/<id>/overlay.json` —
merged in as the internet joins like a JSON merge
patch: objects key by key, any other value (a list included) restated whole, and
`search_entries` and `domains` appended. The
reference company's are the `overlay.json` files under
[`company-2026/services`](../company-2026/services), which is
where Alice's inbox and Northstar's repositories now live. `cargo test -p cw-internet`
fails if a name from that story appears here, and `--test internet_alone` boots every site
in a world with no company at all.

## What is on it

A synthetic public internet on recognisable domains — search engines, webmail, a video site, social
feeds, encyclopedias, forums, shops, an AI assistant and so on. Every one is an
ordinary service reached through the same DNS, routes and HTTP an actor already uses;
none of them touches the host network.

**Addresses are fictional even where names are not.** Every node sits in a range IANA
reserves for documentation — `203.0.113.0/24`, `198.51.100.0/24`, `192.0.2.0/24` — so
no simulated host resembles a real one. Alternate spellings are DNS aliases pointing
at the same node, exactly as they are on the real internet.

Internet nodes are reached through an edge router and three points of presence, so
latency is realistically higher than a LAN's 10 µs: a few milliseconds to a nearby site,
more to a distant one.

## The sites

Every site is a seed file under `sites/`, grouped here by the service kind that renders it
(the variant in parentheses is its `mode`, `layout` or `skin`). Alternate spellings
(`www.`, `m.`, `youtu.be`, `twitter.com`, `youtubemusic.com`, …) are extra `domains` on the
same seed.

| Kind | Sites |
|---|---|
| agency | frs.gov.example, sos.arden.gov.example, revenue.arden.gov.example, permits.portlinden.gov.example ([government offices](../../docs/government.md)) |
| assistant | chatgpt.com, claude.ai |
| bank | northwind.example, pinecrest.example (each with a routing number and, for cards, a BIN), paypal.com. A customer pays another customer of the same bank on `/send`; a service with an agreement (`originators`) originates ACH entries and, where the agreement says so, reads its account, sends wires and pays from it: alibaba.com holds its client funds at Pinecrest and a wallet at PayPal that way. A cardholder adds a card to Apple Pay or Google Pay on `/cards` |
| bnpl (pay later) | klarna.com (Pay in 4, Pay in 30 days), afterpay.com (Pay in 4). A buyer links a bank account once and takes a plan at a merchant's checkout (`/checkout?merchant=&ref=&amount=&return=`); the provider pays the merchant in full by ACH from its settlement account at northwind.example, with the merchant's payment code as the entry's memo, and debits the buyer's account for each instalment on the world clock. It reads its own entries back from the bank: a returned debit is retried once three days later, then the plan is overdue with a late fee. A merchant's service asks about a plan and refunds it (`/api/merchant/…`); alibaba.com is the seeded merchant |
| clearing (the ACH and wire operator) | fedline.example |
| cardnet (the card network) | keystone-network.example. It also keeps the tokens of cards in device wallets and the sheet a buyer confirms a wallet payment on (`/wallet/pay`) |
| ledger (accounting) | qbo.intuit.com |
| calendar (gcal) | calendar.google.com |
| chat / slack / discord / messages | chat.internal, slack.com, discord.com, messages.internal |
| docs (gdocs, plain) | docs.google.com, notion.so |
| drive (gdrive, dropbox) | drive.google.com, dropbox.com |
| forum (subreddits) | reddit.com, yelp.com |
| forum (qa) | stackoverflow.com, quora.com |
| forum (linkfeed) | news.ycombinator.com, craigslist.org |
| geo (maps, weather) | maps.google.com, openstreetmap.org, weather.com |
| git (github, plain) | github.com, gitlab.com |
| issues (linear) | linear.app |
| mail (gmail, outlook, plain) | mail.google.com, outlook.com, mail.com (each lets a site send as itself from its own domain, `senders`: `alibaba` as `alibaba.com`) |
| media (video) | youtube.com, tiktok.com, twitch.tv, vimeo.com, netflix.com |
| media (audio, music) | spotify.com, soundcloud.com, music.youtube.com |
| postal | mailbox.post.gov.example (where the offices' letters arrive) |
| press (wire) | reuters.com, bbc.com, news.google.com |
| press (magazine) | theverge.com, arstechnica.com, nytimes.com, cnn.com |
| press (blog) | alicechen.dev, bmartinez.net, eng.northstar.example, medium.com, substack.com |
| search (google, bing, ddg) | google.com, bing.com, duckduckgo.com |
| shop (retail) | amazon.com, etsy.com, ebay.com, booking.com, airbnb.com, uber.com, doordash.com |
| shop (tickets) | ticketmaster.com |
| shop (wholesale) | alibaba.com, a marketplace between companies: 24 suppliers and 73 listings in 38 categories, each listing with a minimum order and a price by quantity. **Sourcing**: a buyer searches products or manufacturers (by words, by a picture, or in AI Mode), sends an inquiry, or posts a request for quotation that takes up to ten quotes; stores bid within a monthly quota; the buyer compares the quotes side by side and orders a sample. **Ordering**: either side drafts a Trade Assurance order (trade term EXW, FOB, CIF or DDP, shipping method, ship date, initial payment and balance) and the other confirms it. **Paying**: the site never marks an order paid. A payment is a wire to the site's client-funds account at pinecrest.example through fedline.example, a card authorized by its issuer over keystone-network.example (the site is an acquirer there), Apple Pay or Google Pay (a card the buyer added to the wallet at its bank, confirmed on the network's sheet with a token, so the site never holds the card), an online bank payment (an ACH debit that counts once it settled and was not sent back), a payment from a paypal.com wallet, or Pay Later; refunds go back the way the money came, with the method's fee only where the real site returns it, and a store is paid by ACH. A card payment its cardholder disputes at their bank is a chargeback with a page for each side (`/chargebacks`): the payout is frozen, the store answers with evidence, the network rules. **Fulfilment**: production updates, an inspection service, shipment with tracking on the world clock, a forwarder the buyer books and compares quotes for, customs with duties for whoever the trade term names, delivery, receipt and its automatic confirmation. **After-sale**: late-shipment and late-delivery guarantees, disputes with evidence, offers, escalation and a ruling, returns, reviews, repeat orders, dropshipping orders. **Messenger**: files, cards (a listing, an order, a quotation, a proforma invoice, a request), group chats, replies, feedback, an order drafted from a chat. **A store's back office** (`/seller`, for a supplier's `members`): listings with variants, a price ladder and a quality score; analytics by listing and day; the inquiry inbox with response rate and time; quotations; the request market; the store page; membership (Gold Supplier, Verified Supplier); policy notices and penalty points; funds; and a monitor of every change to its own listings and to the stores it watches, whoever made the change. **People and AI Mode**: each supplier has a sales person whom a language model decides for (`people.personas`: working hours in China, a delay before each answer or none, English or Chinese with a translation, quotations and files through the messenger, a price that may not hold, an order that may be late), and AI Mode is the same model with the site's listing search, its supplier lookup and the internet's search engine. The site calls no model: it leaves its questions in `asks`, and the world's owner answers them ([societies](../../docs/societies.md#services-that-ask)). **Mail**: the site mails the address in an account's settings as `no-reply@alibaba.com`: a sign-in code, messages, quotes, an order's steps, payments and notices. Also on `message.`, `biz.`, `i.`, `rfq.`, `fb.`, `marketing.`, `myfinance.`, `logistics.` and `terms.alibaba.com` |
| social (microblog) | x.com, mastodon.social, facebook.com, bsky.app |
| social (photos) | instagram.com, pinterest.com |
| social (professional) | linkedin.com |
| speaker | kitchen, livingroom and office-tv `.speaker.internal` (cast targets; `bedroom.speaker.internal` is declared in DNS but unplugged: nothing listens) |
| static-site | intranet.internal, northstar.example, status.northstar.example, guide.example, apple.com, microsoft.com, zoom.us, whatsapp.com, figma.com, cloudflare.com, aws.amazon.com, stripe.com, npmjs.com, pypi.org, crates.io, docs.rs |
| wiki | wikipedia.org, imdb.com, archive.org |

## Adding or editing a site

Seed data lives in one file per site under `worlds/internet/services/<service-id>/service.json`,
never directly in `world.json`. The directory name must equal the `id`.

```json
{
  "id": "theverge", "kind": "press", "node": "theverge",
  "domains": ["theverge.com", "www.theverge.com"], "port": 80,
  "initial_state": { "layout": "magazine", "brand": "The Verge" },
  "search_entries": [
    {"url": "http://theverge.com/2026/atlas-determinism", "title": "…",
     "snippet": "…", "vertical": "news", "authority": 6, "keywords": ["atlas"]}
  ]
}
```

`search_entries` is the site's sitemap: every search engine seeded without `documents`
indexes all of them when the world boots, filing each under the site's first domain and
applying its own `authority_overrides`. That is what makes a search result a link that
really resolves. Its `vertical` is one of the
engines' `all`, `news`, `videos` or `images`. `place` lets a site bring its own host on a
documentation-range address, and derives the node, the link and a DNS A record for every
domain the site declares, so no name depends on the runtime's auto-add — see
[world blueprints](../../the engine's design notes). In `world.json` a service that came from its
own file is written on one line — the seed file is the reviewable copy — while the
hand-written computers, network and `.internal` services stay pretty-printed.
`scripts/build-content.sh` rebuilds this world with the others, and is idempotent.

