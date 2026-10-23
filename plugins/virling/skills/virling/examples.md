# Worked examples

Today is the user's today; work out dates from it. All commands take `--site <domain>` when the account has more than one site.

## Which channel made the most money last month?

```sh
virling query channels --from 2026-09-01 --to 2026-09-30 --json
```

Take the first row that is not `unattributed`; rows are sorted by `revenue_usd`. Give its revenue, payments and `revenue_per_visitor`, and the runner-up. If the `Unattributed` row is bigger than the top channel, say so: much of the revenue cannot be traced to a visit.

## How did revenue change this month?

```sh
virling query overview --range 30d --json
```

Report `current.revenue_usd` with `change.revenue_usd` as a percentage, and the same for `visitors` and `conversion`. Name both ranges (`range`, `previous_range`).

## What did visitors from ChatGPT do before paying?

1. Find the payments credited to ChatGPT:
   ```sh
   virling query payments --range 90d --filter attributed --limit 200 --json
   ```
   Keep rows where `touch.source` is "ChatGPT" (or `touch.referrer_host` is `chatgpt.com`). For all AI assistants, use `touch.channel == "AI"`. Page with `--offset` while `offset + limit < total`.
2. For a few of them, read the visitor's timeline:
   ```sh
   virling query visitor <rows[i].visitor> --json
   ```
3. Summarise across visitors: the landing page, pages seen and time on them (`steps` of type `page`), goals completed, how many visits before the payment, and the time from first visit (`first_touch.ts`) to payment.

## Which pages lead to revenue?

```sh
virling query landing-pages --range 90d --json
virling query pages --range 90d --json
```

`landing-pages` credits each payment to the first page of its visit. `pages` counts a payment once for every page in its credited visit, so totals add up to more than revenue; use it to see which pages paying visits pass through.

## Is a campaign paying off?

```sh
virling query campaigns --range 30d --json
```

Compare `revenue_usd` with what the user says the campaign cost. `key` is the `utm_campaign` value; `(none)` is traffic without one.

## Where do refunds come from?

```sh
virling query payments --filter refunded --range 90d --json
```

Group the rows by `touch.source` and sum `refunded` (minor units) or the difference between `amount_usd` and `net_usd`.
