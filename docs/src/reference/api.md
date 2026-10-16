# Read API

A read-only JSON API over the same numbers as the dashboard: totals, breakdowns with revenue, payments and visitor timelines. The [CLI](cli.md) and AI agents use it. It is served when the app is on (`--app`).

## API keys

Create a key in **Settings > API keys** in the app. The key is shown once, so copy it then. Virling stores only a hash of it. A key reads every site of the account that made it and cannot change anything. Revoke it in the same place; requests with it fail right away.

Send the key as a bearer token:

```sh
curl -H "Authorization: Bearer $VIRLING_API_KEY" https://stats.example.com/api/v1/sites
```

## Errors

Errors are JSON with an `error` message.

| Status | When |
| --- | --- |
| `400` | A parameter is not valid, such as an unknown range, dimension or filter. |
| `401` | The `Authorization` header is missing, or the key is wrong or revoked. |
| `404` | The site does not exist or belongs to another account, or the visitor has no events or payments. |
| `503` | The data could not be read. Retry later. |

## Date ranges

Endpoints that take a range read it like the dashboard does. Days are local dates in the site's timezone.

| Parameter | What it does |
| --- | --- |
| `range` | `today`, `7d`, `30d`, `90d`, `12m` or `all`. `all` starts on the day tracking started. Default: `30d`, or `all` for a site tracked for 30 days or less. |
| `from`, `to` | Explicit dates such as `from=2026-09-01&to=2026-09-30`, both inclusive. They win over `range`. Up to 10 years. |

Responses echo the range they used, as `{"from": "2026-09-17", "to": "2026-10-16", "timezone": "UTC"}`.

Money is net of refunds, in USD (`*_usd` fields) or in the payment's currency minor unit (`amount`, `refunded`). Times are RFC 3339 in UTC.

## GET /api/v1/sites

The account's sites.

```json
{"sites": [{"domain": "example.com", "timezone": "Europe/Berlin"}]}
```

## GET /api/v1/sites/{domain}/overview

Totals for the range and for the period of the same length just before it.

```json
{
  "site": "example.com",
  "range": {"from": "2026-09-17", "to": "2026-10-16", "timezone": "UTC"},
  "previous_range": {"from": "2026-08-18", "to": "2026-09-16", "timezone": "UTC"},
  "current": {
    "visitors": 1840, "sessions": 2410, "payments": 31, "customers": 29,
    "revenue_usd": 1519.0, "converted": 22, "matched_payments": 24,
    "matched_revenue_usd": 1176.0, "revenue_per_visitor": 0.64, "conversion": 0.012
  },
  "previous": {"visitors": 1502, "revenue_usd": 1210.0, "...": "..."},
  "change": {"visitors": 0.225, "revenue_usd": 0.255, "payments": 0.148, "conversion": 0.09, "revenue_per_visitor": 0.04}
}
```

| Field | Meaning |
| --- | --- |
| `visitors`, `sessions` | Distinct visitors and visits in the range. |
| `payments`, `customers`, `revenue_usd` | Payments that did not fail, their distinct payers and their net USD. |
| `converted`, `matched_payments`, `matched_revenue_usd` | The same, counting only payments whose visitor was tracked in the range. |
| `conversion` | `converted / visitors`. `null` until a payment is matched to a visitor. |
| `revenue_per_visitor` | `matched_revenue_usd / visitors`. `null` until a payment is matched. |
| `change` | Relative change from `previous`, `0.25` meaning 25% more. `null` when the previous value is zero or missing. |

## GET /api/v1/sites/{domain}/breakdown/{dimension}

Visitors and credited revenue per value of a dimension, most revenue first, then most visitors.

| Dimension | Rows |
| --- | --- |
| `channel` | Search, Social, AI, Email, Paid, Referral, Direct and so on. |
| `referrer` | Referring domain, such as `chatgpt.com`. |
| `campaign` | `utm_campaign`. |
| `keyword` | `utm_term`. |
| `landing_page` | First page of the visit. |
| `page` | Every page viewed. A payment counts for each page of the visit credited for it. |
| `country` | Two-letter country code. |
| `device` | Desktop, Mobile, Tablet or Unknown. |
| `browser`, `os` | Browser and operating system. |

It takes the range parameters and `limit` (default 100, up to 1000).

```json
{
  "site": "example.com",
  "range": {"from": "2026-09-17", "to": "2026-10-16", "timezone": "UTC"},
  "dimension": "channel",
  "total": 7,
  "rows": [
    {"key": "AI", "label": "AI", "unattributed": false, "visitors": 210, "payments": 9,
     "customers": 9, "revenue_usd": 441.0, "revenue_per_visitor": 2.1},
    {"key": null, "label": "Unattributed", "unattributed": true, "visitors": 0, "payments": 7,
     "customers": 7, "revenue_usd": 343.0, "revenue_per_visitor": null}
  ]
}
```

Visitors count visits that started in the range. A payment is credited to one visit by the site's [attribution model](../attribution/model.md). Payments with no credited visit are in the `Unattributed` row. `key` is `null` for that row and for visits without a value, labelled `(none)`. `total` is the number of rows before `limit`.

## GET /api/v1/sites/{domain}/payments

Payments in the range, newest first, with the visit credited for each.

| Parameter | Default | What it does |
| --- | --- | --- |
| range parameters | | See above. |
| `filter` | `all` | `all`, `attributed`, `unattributed` or `refunded`. |
| `limit` | `20` | Up to 200. |
| `offset` | `0` | Rows to skip, for paging. |

```json
{
  "site": "example.com", "range": {"...": "..."}, "filter": "all",
  "total": 31, "limit": 20, "offset": 0,
  "rows": [{
    "provider": "stripe", "provider_payment_id": "pi_3P...", "ts": "2026-10-15T09:12:44Z",
    "amount": 4900, "currency": "usd", "amount_usd": 49.0, "refunded": 0, "net_usd": 49.0,
    "status": "paid", "visitor_id": "vlx3k2a9b7f1c0d4e8", "customer_id": "cus_Q1...",
    "is_renewal": false, "visitor": "vlx3k2a9b7f1c0d4e8",
    "touch": {
      "session_id": "...", "ts": "2026-10-15T08:58:02Z", "channel": "AI",
      "referrer_host": "chatgpt.com", "source": "ChatGPT", "campaign": null,
      "landing_page": "/pricing", "country": "DE", "device": "Desktop"
    },
    "first_touch": {"ts": "2026-10-02T17:40:10Z", "channel": "Search", "referrer_host": "google.com", "source": "Google"}
  }]
}
```

`touch` is the credited visit, `null` when the payment is unattributed. `first_touch` is the payer's first visit. `visitor` is the visitor whose timeline shows the payment; pass it to the visitor endpoint.

## GET /api/v1/sites/{domain}/visitors/{visitor_id}

Everything one visitor did, oldest first: each visit with its pages, goals, how it ended and payments made during it, plus payments outside a visit such as renewals.

```json
{
  "site": "example.com", "visitor_id": "vlx3k2a9b7f1c0d4e8",
  "country": "DE", "browser": "Firefox", "device": "Desktop",
  "entries": [{
    "type": "visit", "start": "2026-10-15T08:58:02Z", "end": "2026-10-15T09:12:44Z",
    "channel": "AI", "referrer_host": "chatgpt.com", "source": "ChatGPT", "campaign": null,
    "pageviews": 3,
    "steps": [
      {"type": "page", "ts": "2026-10-15T08:58:02Z", "path": "/pricing", "secs": 95},
      {"type": "goal", "ts": "2026-10-15T09:01:10Z", "name": "signup"},
      {"type": "exit", "ts": "2026-10-15T09:10:30Z", "page": "/checkout", "link": "checkout.stripe.com/c/pay"},
      {"type": "payment", "provider": "stripe", "ts": "2026-10-15T09:12:44Z", "net_usd": 49.0, "...": "..."}
    ]
  }],
  "payments": [{"provider": "stripe", "net_usd": 49.0, "...": "..."}]
}
```

An entry is `{"type": "visit", ...}` or `{"type": "payment", ...}`. A step is `page` (with seconds on the page, `null` when unknown), `goal`, `exit` (the last page and the link clicked out, if any) or `payment`. Payment entries and steps have the same fields as payment rows.

## Not available yet

- Writing through the API, such as adding sites or goals <span class="na">Not available yet</span>
- Keys limited to one site <span class="na">Not available yet</span>
