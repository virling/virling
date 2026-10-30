---
name: virling
description: Answer revenue attribution questions from a Virling server with its MCP tools. Use when the user asks which channels, referrers, campaigns, pages or countries make money, how revenue or conversion changed, which payments came from where, or what a paying visitor did before paying.
---

# Virling

Virling links website visits to payments (Stripe, Polar, Dodo Payments, LemonSqueezy, Creem, RevenueCat). Each payment is credited to one visit by the site's attribution model, so every report can show revenue, not just traffic. The bundled `virling` MCP server reads it, read-only.

## Setup

The user connects Virling once, in Claude or Paperclip; see [setup.md](setup.md). Then call `list_sites`. With one site, `site` can be left out of the other tools. With several, pass `site` with the domain from `list_sites`.

## Tools

| Question | Tool |
| --- | --- |
| Which sites can I read? | `list_sites` |
| How is the site doing? | `get_overview` |
| Which channel, referrer, campaign, page or country makes money? | `get_breakdown` with `dimension` set to `channel`, `referrer`, `campaign`, `keyword`, `landing_page`, `page`, `country`, `device`, `browser`, `os` or `language` |
| Which channel brings customers who stay? | `get_retention` |
| Do AI assistants use and name the site? | `get_ai_answers` |
| Recent payments and their source | `list_payments` with `filter` (`all`, `attributed`, `unattributed`, `refunded`), `limit` (max 200) and `offset` |
| What did one payer do? | `get_visitor_timeline` with `visitor_id` from a payment's `visitor` |

Every tool except `list_sites` and `get_visitor_timeline` takes a range: `range` (`today`, `7d`, `30d`, `90d`, `12m`, `all`) or exact local dates `from` and `to` (`YYYY-MM-DD`, both inclusive). For "last month" use the calendar month with `from` and `to`, not `30d`. `list_payments` pages with `limit` and `offset`; `total` says how many there are.

## Reading the results

- Money is USD net of refunds (`revenue_usd`, `net_usd`). `amount` is in the payment currency's minor unit (cents).
- `overview.change` is relative to the previous period of the same length: `0.25` means up 25%. `null` means nothing to compare.
- `conversion` and `revenue_per_visitor` only count payments matched to a visitor tracked in the range; `null` until one is matched.
- Breakdown rows are sorted by revenue. `visitors` counts visits that started in the range, while payments follow their credited visit. The `Unattributed` row (`"unattributed": true`) holds payments with no credited visit, such as payments made before tracking or from a checkout not linked to visits. Mention it when it is large.
- In payments, `touch` is the credited visit (`source` like "ChatGPT", `channel` like "AI", `campaign`, `landing_page`); `null` means unattributed. `first_touch` is the payer's first ever visit. `visitor` is the ID to pass to `get_visitor_timeline`.
- A visitor timeline is `entries` of `visit` (with `steps`: `page` with `secs` on page, `goal`, `exit`, `payment`) or a standalone `payment` such as a renewal.
- Retention groups customers by their first payment's channel, referrer and campaign; renewals stay with it. `still_paying` comes from the provider's subscription status, else a recent payment. `churn_30d` covers customers who first paid 30 to 60 days ago. Under 60 days of payments (`day_one: true`) talk about refunds and churn, not LTV.
- Channels include Search, Communities (Hacker News, Reddit, Product Hunt and similar), Social, AI (ChatGPT, Perplexity, Claude and similar), Email, Paid, Referral and Direct.

## Weekly channel review

When asked for a weekly review, or run on a schedule:

1. `get_overview` with `range` `7d` for revenue, payments and the change against the week before.
2. `get_breakdown` with `dimension` `channel` and `range` `7d`, and again with `30d` for context.
3. `get_retention` for which channels bring customers who keep paying.
4. Report the channel that made the most money, the biggest drop, the share of revenue that is Unattributed, and one concrete next step, such as doing more of what paid or fixing checkout linking when Unattributed is high.

Report numbers with their range and site, and say when the data is thin (a handful of payments) instead of drawing strong conclusions.

## More

The CLI and HTTP API are documented at https://docs.virling.com/reference/api.html.

## Examples

See [examples.md](examples.md) for worked questions, such as "which channel made the most money last month" and "what did visitors from ChatGPT do before paying".
