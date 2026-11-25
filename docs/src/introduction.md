# Introduction

Virling is open-source revenue analytics. It joins every visit to the payment that followed, so you can see which channels make you money, not just traffic.

## What Virling answers

- **Revenue by channel.** Which channel brought the most revenue this month, after refunds.
- **Value per visitor.** What a visitor from each source is worth, so a launch or a Reddit post can be judged by money instead of pageviews.
- **The path before payment.** Which sessions a paying customer had in the lookback window, and which one got the credit.

## What works today

- A tracking script about 2 KB gzipped that records pageviews, referrers, UTM tags, single-page-app route changes and custom goals, with a first-party visitor ID. See [Add the script](track/script.md).
- An optional [cookieless mode](track/cookieless.md) that counts visitors with a daily salted hash.
- Event ingest into embedded DuckDB with bot filtering, a per-site rate limit and [channel rules](attribution/channels.md). Visitor IPs are never stored.
- Payment sync from nine providers: [Stripe](payments/stripe.md), [Dodo Payments](payments/dodo.md), [Polar](payments/polar.md), [LemonSqueezy](payments/lemonsqueezy.md), [Creem](payments/creem.md), [RevenueCat](payments/revenuecat.md), [Paddle](payments/paddle.md), [WooCommerce](payments/woocommerce.md) and [Shopify](payments/shopify.md), with a read-only key where the provider allows one. Virling backfills 90 days, then checks for new payments every minute. Shopify backfills 60 days. RevenueCat imports one-time purchases from the last 90 days, once, then sends purchases, renewals and refunds by webhook; subscription payments from before you connected are not imported.
- Imports of daily traffic history from [Plausible](track/import-plausible.md) and [Google Analytics 4](track/import-google-analytics.md) CSV exports, from the command line. Payments are only credited to tracked visits.
- [Attribution](attribution/index.md) with [three models](attribution/model.md), last non-direct, first touch or last touch, and a 30, 60, 90 or 365 day lookback window. Renewals keep the channel of the customer's first payment.
- A [revenue dashboard](guides/dashboard.md) with revenue, revenue per visitor and conversion by channel, referrer, campaign, page and country, payment journeys, a Visitors page, [retention by first channel](attribution/retention.md), [tracked short links](track/links.md), and AI assistant visits next to [AI crawler](track/ai-crawlers.md) hits.
- Weekly email reports with agency branding, client report links and an optional public dashboard. See [Run Virling for client sites](guides/agencies.md).
- Team members on Agency accounts, up to 3 users when self-hosting.
- A [read API](reference/api.md), a [CLI](reference/cli.md) and a read-only [MCP server](reference/mcp.md) with OAuth sign-in, so AI assistants can answer revenue questions. API keys read your numbers and can only add chart notes.
- A [WordPress plugin](track/frameworks.md#wordpress).
- Magic-link sign-in and an onboarding flow, served when you start the server with `--app`.

Virling runs as one Rust binary with embedded storage. There is no Postgres, ClickHouse or Redis to run next to it. See [How Virling works](how-it-works.md).

## Project status

Virling is in early development. Releases are listed on [GitHub](https://github.com/virling/virling/releases). Expect breaking changes until a stable release, and read the [changelog](https://github.com/virling/virling/blob/main/CHANGELOG.md) before you upgrade.

Hosted plans at app.virling.com start with a 14-day free trial, and you can also run Virling on your own server. See [Hosted plan](hosted.md).

## License

Virling is licensed under [AGPL-3.0](https://github.com/virling/virling/blob/main/LICENSE). Self-host it freely. If you run a modified version as a service, share your changes. It is built by Stacklance Infotech LLP.

## Next

Follow the [Quickstart](quickstart.md) to go from an empty server to your first attributed payment.
