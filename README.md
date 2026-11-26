<p align="center">
  <img src="https://virling.com/favicon.svg" width="72" height="72" alt="Virling logo">
</p>

<h1 align="center">Virling</h1>

<p align="center">
  Open-source revenue attribution. See which traffic channel brings paying customers, not just visitors.<br>
  <a href="https://virling.com">virling.com</a> · <a href="https://virling.com/demo/">Live demo</a> · <a href="https://docs.virling.com">Docs</a> · <a href="https://virling.com/agency">For agencies</a>
</p>

<p align="center">
  <a href="https://www.opensourcealternatives.to"><img src="https://www.opensourcealternatives.to/badge-osa.svg" alt="As seen on Open Source Alternatives"></a>
</p>

<p align="center">
  <a href="https://virling.com/demo/"><img src=".github/dashboard.png" alt="Virling dashboard: revenue, revenue per visitor and the channels that bring paying customers" width="900"></a>
</p>

---

Virling joins every visit to the payment that followed. It answers the questions a founder asks every week:

- Which channel brought the most revenue this month?
- Did that launch, tweet or Reddit post make money, or just traffic?
- What is a visitor from each source worth?
- What path did my last paying customers take before they paid?

It runs as one Rust binary with embedded storage. No Postgres, no ClickHouse, no Redis.

## Who it is for

- **[SaaS](https://virling.com/integrations/saas):** which channel brings customers who convert from trial and keep paying.
- **[Online stores](https://virling.com/integrations/ecommerce):** which channel brings orders and revenue to a Shopify or WooCommerce store.
- **[Agencies](https://virling.com/agency):** one account for every client, with white-label weekly reports on which channels brought paying customers.
- **[AI apps](https://virling.com/integrations/ai-apps):** which posts, communities and AI assistants bring users who pay.

## Get started

| [Start a 14-day free trial](https://app.virling.com/login) | [Self-host](#run-it) |
| --- | --- |
| Hosted at app.virling.com, with no server to run. Add a card, cancel before day 15 and nothing is charged. See [Hosted plan](https://docs.virling.com/hosted). | Free, with every feature, on your own server. Run the Docker image or a Linux or macOS binary. See [Install](https://docs.virling.com/install/). |

> **Status:** early development. Expect breaking changes until a stable release. The list below describes the main branch; the [latest release](https://github.com/virling/virling/releases) can be behind it.

## What works today

- **Tracking script** (`/v.js`, source in `assets/v.src.js`): pageviews, UTM tags, referrers, single-page-app route changes, custom goals, and a first-party visitor ID. Stripe Payment Links, Polar checkout links, Dodo payment links and LemonSqueezy checkout links get the visitor ID added automatically.
- **Cookieless mode**: a daily-rotating salted hash instead of a cookie, for sites that want no consent banner, at the cost of payment attribution. It can also start cookieless and switch to the cookie on consent.
- **Event ingest** into embedded DuckDB, with bot filtering, per-site rate limits and channel rules (Paid, Email, AI, Organic search, Communities, Social, Referral, Direct). Visitor IPs are never stored.
- **Payment sync** from [nine providers](https://docs.virling.com/payments/): Stripe, Dodo Payments, Polar, LemonSqueezy, Creem, RevenueCat, Paddle, WooCommerce and Shopify. [Which key to create](https://docs.virling.com/payments/#which-key-to-create) lists the access each one needs; LemonSqueezy keys cannot be limited. Virling backfills 90 days (60 for Shopify, one-time purchases only for RevenueCat), then picks up new payments every minute, or by webhook for RevenueCat. It never creates, changes or refunds a payment. [LemonSqueezy payments](https://docs.virling.com/payments/lemonsqueezy) link to visits through a webhook, which needs a public `https` address.
- **Imports** of daily traffic history from [Plausible](https://docs.virling.com/track/import-plausible) and [Google Analytics 4](https://docs.virling.com/track/import-google-analytics) CSV exports, from the command line. Payments are only credited to tracked visits.
- **Attribution**: last non-direct, first or last touch, with a 30, 60, 90 or 365 day lookback. Renewals keep the channel of the customer's first payment. Payments with no visitor ID and no email match count as Unattributed.
- **Revenue dashboard**: revenue, revenue per visitor and conversion by channel, referrer, campaign, page and country, payment journeys, a Visitors page, retention by first channel, tracked short links, and AI assistant visits next to AI crawler hits.
- **Reports and sharing**: weekly email reports with agency branding, client report links, and an optional public dashboard.
- **Team members** on Agency accounts, up to 3 users when self-hosting.
- **Read API, MCP server and CLI**: API keys that read your numbers and can only add chart notes, and read-only MCP with OAuth sign-in, so `virling query` and AI assistants such as Claude and ChatGPT can answer revenue questions. Hosted Virling is in the [Claude directory](https://claude.ai/directory/virling). An agent skill is in `plugins/virling/skills/virling`, and `claude plugin marketplace add virling/virling` installs it with the MCP server as a Claude Code plugin.
- **WordPress plugin** in `integrations/wordpress`.
- **Magic-link sign-in and onboarding** (behind `--app`).

Not available yet: other attribution models such as linear or position-based, custom channel rules, a reporting currency other than USD, uploading an import in site settings, and Windows builds.

## Run it

With Docker:

```sh
docker run -d --name virling -p 8080:8080 -v virling-data:/data \
  -e VIRLING_BASE_URL=https://stats.example.com \
  ghcr.io/virling/virling:latest serve --app
docker exec virling virling add-site yourdomain.com --owner you@example.com
```

The full guide, with release binaries, a reverse proxy and payment setup, is at [docs.virling.com](https://docs.virling.com). To run from source, you need Rust 1.99 or newer:

```sh
cargo run --release -- add-site yourdomain.com --owner you@example.com
cargo run --release -- serve --app
```

Then add the script to your site:

```html
<script defer src="https://YOUR-VIRLING-HOST/v.js" data-site="yourdomain.com"></script>
```

Track a goal from your code:

```js
virling("goal", "signup");
```

Pass the visitor ID into checkout so payments can be matched back to visits:

```js
virling.vid();
```

## Configuration

| Variable | Default | What it does |
| --- | --- | --- |
| `VIRLING_DATA` | `./virling-data` | Folder for the SQLite and DuckDB files |
| `VIRLING_ADDR` | `0.0.0.0:8080` | Address to listen on |
| `VIRLING_BASE_URL` | `http://localhost:8080` | Public URL, used in emails and snippets |
| `VIRLING_APP` | off | Serve sign-in and the app |
| `VIRLING_SMTP_URL` | unset | SMTP for emails, e.g. `smtps://user:pass@smtp.example.com:465`. Unset: emails are logged |
| `VIRLING_MAIL_FROM` | `Virling <login@virling.com>` | Sender for sign-in emails |
| `VIRLING_TRUST_PROXY` | off | Read the client IP from `X-Forwarded-For` |
| `VIRLING_EVENTS_PER_SECOND` | `200` | Ingest limit per site |

## Development

```sh
cargo test
```

`worker/` holds the Cloudflare Worker that serves virling.com. It reuses the landing pages in `assets/` and builds with `wrangler deploy`.

## License

[AGPL-3.0](LICENSE). Self-host it freely. If you run a modified version as a service, share your changes.

Built by [Stacklance Infotech LLP](https://stacklance.in).
