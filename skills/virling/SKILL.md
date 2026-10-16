---
name: virling
description: Answer revenue attribution questions from a Virling server with the `virling query` CLI. Use when the user asks which channels, referrers, campaigns, pages or countries make money, how revenue or conversion changed, which payments came from where, or what a paying visitor did before paying.
---

# Virling

Virling links website visits to payments (Stripe, Polar, Dodo Payments, LemonSqueezy). Each payment is credited to one visit by the site's attribution model, so every report can show revenue, not just traffic. `virling query` reads it through a read-only API.

## Setup

1. Check the CLI: `virling --version`. If it is missing, see [setup.md](setup.md).
2. It needs the server URL and an API key. Ask the user for them if these are not set:
   ```sh
   export VIRLING_URL=https://stats.example.com   # their Virling server
   export VIRLING_API_KEY=vk_...                  # app: Settings > API keys
   ```
   Never print the key back or write it into files the user did not ask for.
3. `virling query sites --json`. With one site, `--site` can be left out. With several, pass `--site <domain>` (or set `VIRLING_SITE`).

## Commands

Always add `--json` and read the JSON. Every command has `--help` with examples.

| Question | Command |
| --- | --- |
| How is the site doing? | `virling query overview --range 30d --json` |
| Which channel makes money? | `virling query channels --json` |
| Which referrer / campaign / page? | `virling query referrers --json`, `campaigns`, `landing-pages`, `pages` |
| By country or device? | `virling query countries --json`, `devices` |
| Recent payments and their source | `virling query payments --limit 50 --json` |
| What did one payer do? | `virling query visitor <visitor> --json` |

Ranges: `--range today|7d|30d|90d|12m|all`, or exact local dates `--from 2026-09-01 --to 2026-09-30` (both inclusive). For "last month" use the calendar month with `--from/--to`, not `30d`. Payments page with `--limit` (max 200) and `--offset`; `total` says how many there are.

## Reading the results

- Money is USD net of refunds (`revenue_usd`, `net_usd`). `amount` is in the payment currency's minor unit (cents).
- `overview.change` is relative to the previous period of the same length: `0.25` means up 25%. `null` means nothing to compare.
- `conversion` and `revenue_per_visitor` only count payments matched to a visitor tracked in the range; `null` until one is matched.
- Breakdown rows are sorted by revenue. `visitors` counts visits that started in the range, while payments follow their credited visit. The `Unattributed` row (`"unattributed": true`) holds payments with no credited visit, such as payments made before tracking or from a checkout not linked to visits. Mention it when it is large.
- In payments, `touch` is the credited visit (`source` like "ChatGPT", `channel` like "AI", `campaign`, `landing_page`); `null` means unattributed. `first_touch` is the payer's first ever visit. `visitor` is the ID to pass to `virling query visitor`.
- A visitor timeline is `entries` of `visit` (with `steps`: `page` with `secs` on page, `goal`, `exit`, `payment`) or a standalone `payment` such as a renewal.
- Channels include Search, Social, AI (ChatGPT, Perplexity, Claude and similar), Email, Paid, Referral and Direct.

Report numbers with their range and site, and say when the data is thin (a handful of payments) instead of drawing strong conclusions.

## Examples

See [examples.md](examples.md) for worked questions, such as "which channel made the most money last month" and "what did visitors from ChatGPT do before paying".
