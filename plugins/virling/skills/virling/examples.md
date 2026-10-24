# Worked examples

Today is the user's today; work out dates from it. Pass `site` to every tool when the account has more than one site.

## Which channel made the most money last month?

Call `get_breakdown` with `dimension: "channel"`, `from: "2026-09-01"`, `to: "2026-09-30"`.

Take the first row that is not `unattributed`; rows are sorted by `revenue_usd`. Give its revenue, payments and `revenue_per_visitor`, and the runner-up. If the `Unattributed` row is bigger than the top channel, say so: much of the revenue cannot be traced to a visit.

## How did revenue change this month?

Call `get_overview` with `range: "30d"`.

Report `current.revenue_usd` with `change.revenue_usd` as a percentage, and the same for `visitors` and `conversion`. Name both ranges (`range`, `previous_range`).

## What did visitors from ChatGPT do before paying?

1. Call `list_payments` with `range: "90d"`, `filter: "attributed"` and `limit: 200`. Keep rows where `touch.source` is "ChatGPT" (or `touch.referrer_host` is `chatgpt.com`). For all AI assistants, use `touch.channel == "AI"`. Page with `offset` while `offset + limit < total`.
2. For a few of them, call `get_visitor_timeline` with `visitor_id` set to `rows[i].visitor`.
3. Summarise across visitors: the landing page, pages seen and time on them (`steps` of type `page`), goals completed, how many visits before the payment, and the time from first visit (`first_touch.ts`) to payment.

## Which pages lead to revenue?

Call `get_breakdown` twice with `range: "90d"`: once with `dimension: "landing_page"`, once with `dimension: "page"`.

`landing_page` credits each payment to the first page of its visit. `page` counts a payment once for every page in its credited visit, so totals add up to more than revenue; use it to see which pages paying visits pass through.

## Which countries pay?

Call `get_breakdown` with `dimension: "country"` and `range: "90d"`. Country is a two-letter code.

## Is a campaign paying off?

Call `get_breakdown` with `dimension: "campaign"` and `range: "30d"`.

Compare `revenue_usd` with what the user says the campaign cost. `key` is the `utm_campaign` value; `(none)` is traffic without one.

## Where do refunds come from?

Call `list_payments` with `filter: "refunded"` and `range: "90d"`.

Group the rows by `touch.source` and sum `refunded` (minor units) or the difference between `amount_usd` and `net_usd`.
