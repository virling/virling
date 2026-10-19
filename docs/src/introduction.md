# Introduction

Virling is open-source revenue analytics. It joins every visit to the payment that followed, so you can see which channels make you money, not just traffic.

## What Virling answers

- **Revenue by channel.** Which channel brought the most revenue this month, after refunds.
- **Value per visitor.** What a visitor from each source is worth, so a launch or a Reddit post can be judged by money instead of pageviews.
- **The path before payment.** Which sessions a paying customer had in the lookback window, and which one got the credit.

## What works today

- A tracking script about 1.2 KB gzipped that records pageviews, referrers, UTM tags, single-page-app route changes and custom goals, with a first-party visitor ID. See [Add the script](track/script.md).
- An optional [cookieless mode](track/cookieless.md) that counts visitors with a daily salted hash.
- Event ingest into embedded DuckDB with bot filtering, a per-site rate limit and [channel rules](attribution/channels.md). Visitor IPs are never stored.
- Payment sync from [Stripe](payments/stripe.md), [Dodo Payments](payments/dodo.md), [Polar](payments/polar.md) and [LemonSqueezy](payments/lemonsqueezy.md) with a read-only key: a 90-day backfill, then a poll every minute.
- [Attribution](attribution/index.md) with two models and a configurable lookback window, renewals that inherit the first payment's channel, and payment journeys in the app.
- Magic-link sign-in and an onboarding flow, served when you start the server with `--app`.

Virling runs as one Rust binary with embedded storage. There is no Postgres, ClickHouse or Redis to run next to it. See [How Virling works](how-it-works.md).

## Project status

Virling is in early development. Releases are listed on [GitHub](https://github.com/virling/virling/releases). Expect breaking changes until a stable release, and read the [changelog](https://github.com/virling/virling/blob/main/CHANGELOG.md) before you upgrade.

The hosted plan at app.virling.com is <span class="na">Not available yet</span>. Today you run Virling on your own server. See [Hosted plan](hosted.md).

## License

Virling is licensed under [AGPL-3.0](https://github.com/virling/virling/blob/main/LICENSE). Self-host it freely. If you run a modified version as a service, share your changes. It is built by Stacklance Infotech LLP.

## Next

Follow the [Quickstart](quickstart.md) to go from an empty server to your first attributed payment.
