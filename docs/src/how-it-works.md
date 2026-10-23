# How Virling works

Virling is one binary that receives events from your site, pulls payments from your provider and joins the two.

## The pieces

1. **The script.** `v.js` runs on your site. On every page load and route change it sends a pageview to `/api/event` on your Virling server, with the page URL, the referrer, the screen width and a visitor ID from a first-party cookie. Goals you call from your code go the same way.
2. **Ingest.** The server drops bots and foreign hosts, and keeps automated browsers apart, including Chrome claiming macOS on a screen under 1,024 device pixels, which no Mac has ([details](reference/event-api.md#responses)). Then it applies the per-site rate limit, sorts each visit into a [channel](attribution/channels.md), groups pageviews into sessions and writes the events to DuckDB.
3. **Payment sync.** With `--app`, a background job polls every connected provider once a minute with a read-only key. The first run imports the last 90 days. Amounts are converted to USD.
4. **Attribution.** After each sync round, attribution is rebuilt for every site: each payment carrying a visitor ID is matched to that visitor's sessions in the lookback window, and one session gets the credit. See [How attribution works](attribution/index.md).
5. **The app.** Sign-in, onboarding, the dashboard, the payments list and payment journeys, served at `/app` when you start with `--app`.

## Storage

Everything lives in one data folder, set with `--data-dir` or `VIRLING_DATA` (default `./virling-data`):

| File | Holds |
| --- | --- |
| `virling.sqlite` | Accounts, sites, sign-in sessions, provider connections and their sync cursors. Runs in WAL mode, so `virling.sqlite-wal` and `virling.sqlite-shm` sit next to it. |
| `events.duckdb` | Pageviews, goals, imported payments and the attribution table. |
| `secret.key` | The 32-byte key that encrypts provider API keys. Created on first start unless you set `VIRLING_SECRET_KEY`. |

SQLite migrations run on every start, so upgrading is replacing the binary. See [Data, backups, upgrades](self-host/data.md).

## What runs when

| Command | Serves |
| --- | --- |
| `virling serve` | `/v.js`, `/api/event`, `/health` and static assets. No sign-in, no sync. |
| `virling serve --app` | All of the above plus `/login`, `/app` and the payment sync job. `/` redirects to `/app`. |

`--marketing` serves the virling.com landing pages and waitlist. You do not need it when you self-host.

## What Virling never does

- It never stores visitor IP addresses. The IP is used only for the daily cookieless hash and is not written anywhere.
- It never writes to your payment provider. Keys are read-only where the provider allows it, and are encrypted at rest.
- It never calls a third party from your visitors' browsers. The script only talks to your Virling server.
