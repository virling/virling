<p align="center">
  <img src="https://virling.com/favicon.svg" width="72" height="72" alt="Virling logo">
</p>

<h1 align="center">Virling</h1>

<p align="center">
  Open-source revenue attribution. See which traffic channel brings paying customers, not just visitors.<br>
  <a href="https://virling.com">virling.com</a> · <a href="https://virling.com/demo/">Live demo</a> · <a href="https://docs.virling.com">Docs</a> · <a href="https://virling.com/agency">For agencies</a>
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

> **Status:** early development. Releases are on the [releases page](https://github.com/virling/virling/releases) and run on your own server today. Expect breaking changes until a stable release. The hosted beta is open to everyone at [beta.virling.com](https://beta.virling.com/login).

## What works today

- **Tracking script** (`/v.js`, about 1.3 KB gzipped, source in `assets/v.src.js`): pageviews, UTM tags, referrers, single-page-app route changes, custom goals, and a first-party visitor ID. Stripe Payment Links, Polar checkout links and Dodo payment links get the visitor ID added automatically.
- **Cookieless mode**: a daily-rotating salted hash instead of a cookie, for sites that want no consent banner, at the cost of payment attribution.
- **Event ingest** into embedded DuckDB, with bot filtering, per-site rate limits and channel rules (Paid, Email, AI, Organic search, Communities, Social, Referral, Direct). Visitor IPs are never stored.
- **Payment sync** from Stripe, Dodo Payments, Polar and LemonSqueezy with a read-only key (LemonSqueezy keys cannot be limited): a 90-day backfill, then a poll every minute. Virling never creates, changes or refunds anything. LemonSqueezy payments count toward revenue, but linking them to visits waits on webhook support.
- **Attribution**: last non-direct or first touch, with a 30, 60, 90 or 365 day lookback. Renewals keep the channel of the customer's first payment. Payments without a visitor ID count as Unattributed.
- **Revenue dashboard**: revenue, revenue per visitor and conversion by channel, referrer, campaign, page and country, payment journeys, a Visitors page, retention by first channel, tracked short links, and AI assistant visits next to AI crawler hits.
- **Reports and sharing**: weekly email reports with agency branding, and an optional public dashboard.
- **Read API, MCP server and CLI**: read-only access with API keys or OAuth sign-in, so `virling query` and AI assistants such as Claude and ChatGPT can answer revenue questions. An agent skill is in `skills/virling`, and `claude plugin marketplace add virling/virling` installs it with the MCP server as a Claude Code plugin.
- **WordPress plugin** in `integrations/wordpress`.
- **Magic-link sign-in and onboarding** (behind `--app`).

Not available yet: the hosted plan, team members, imports from other tools, other attribution models and Windows builds.

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

`worker/` holds the Cloudflare Worker that serves virling.com during the waitlist phase. It reuses the landing pages in `assets/` and builds with `wrangler deploy`.

## License

[AGPL-3.0](LICENSE). Self-host it freely. If you run a modified version as a service, share your changes.

Built by [Stacklance Infotech LLP](https://stacklance.in).
