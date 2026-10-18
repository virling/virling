<p align="center">
  <img src="https://virling.com/favicon.svg" width="72" height="72" alt="Virling logo">
</p>

<h1 align="center">Virling</h1>

<p align="center">
  Open-source revenue analytics. See which channels make you money, not just traffic.<br>
  <a href="https://virling.com">virling.com</a> · <a href="https://docs.virling.com">Docs</a> · <a href="https://virling.com/agency">For agencies</a>
</p>

---

Virling joins every visit to the payment that followed. It answers the questions a founder asks every week:

- Which channel brought the most revenue this month?
- Did that launch, tweet or Reddit post make money, or just traffic?
- What is a visitor from each source worth?
- What path did my last paying customers take before they paid?

It runs as one Rust binary with embedded storage. No Postgres, no ClickHouse, no Redis.

> **Status:** early development. The waitlist is open at [virling.com](https://virling.com). Expect breaking changes until the first release.

## What works today

- **Tracking script** (`/v.js`, about 1.2 KB gzipped, source in `assets/v.src.js`): pageviews, UTM tags, referrers, single-page-app route changes, custom goals, and a first-party visitor ID. Stripe Payment Links, Polar checkout links and Dodo payment links get the visitor ID added automatically.
- **Cookieless mode**: a daily-rotating salted hash instead of a cookie, for sites that want no consent banner.
- **Event ingest** into embedded DuckDB, with bot filtering, per-site rate limits and channel rules (Paid, Email, AI, Organic search, Communities, Social, Referral, Direct). Visitor IPs are never stored.
- **Magic-link sign-in and onboarding** (behind `--app` while the waitlist runs).

## Coming next

Stripe, Dodo Payments, Polar and LemonSqueezy sync from a read-only key, the attribution job, the revenue dashboard and payment journeys.

## Run it

The full guide, with Docker, release binaries and payment setup, is at [docs.virling.com](https://docs.virling.com). To run from source, you need Rust 1.99 or newer.

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
