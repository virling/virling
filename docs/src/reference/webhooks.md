# Webhooks

Virling can post three kinds of events to a URL of yours, within a minute of them happening:

| Event | When |
| --- | --- |
| `payment.attributed` | A new paid payment is credited to a visit. Payments that stay unattributed for 10 minutes are skipped. |
| `goal.completed` | A visitor completes a [goal](../track/goals.md). |
| `alert.traffic_spike` | Visitors today reach at least 20 and three times the daily average of the week before, once a day at most. |

Set it up in **Site settings > Webhooks**: paste a public `https://` URL and click **Send a test and turn on**. Virling sends a `ping` event first and only saves the URL when it answers with a 2xx status. Only the account owner can set it, and the URL and secret are stored encrypted. Webhooks stay off while a site holds sample data.

## Slack

Paste a Slack incoming webhook URL (`https://hooks.slack.com/...`) to get each event as a short message in a channel instead of JSON, such as "New payment on example.com: $49 from ChatGPT".

## Payload

Every other URL gets a `POST` with a JSON body:

```json
{
  "type": "payment.attributed",
  "site": "example.com",
  "created": "2026-11-01T09:30:00Z",
  "data": {
    "provider": "stripe",
    "payment_id": "pi_123",
    "paid_at": "2026-11-01T09:29:12Z",
    "amount": 4900,
    "currency": "usd",
    "amount_usd": 49.0,
    "renewal": false,
    "visitor_id": "v1abc",
    "customer_id": "cus_123",
    "source": "ChatGPT",
    "channel": "AI",
    "referrer": "chatgpt.com",
    "campaign": null,
    "landing_page": "/pricing"
  }
}
```

`amount` is in the currency's minor unit. A `goal.completed` event has `goal`, `path`, `visitor_id` and `completed_at` in `data`, and an `alert.traffic_spike` event has `visitors_today` and `usual_per_day`.

To keep a copy in a warehouse, point the webhook at a small endpoint or pipeline of yours that inserts each body as a row.

## Verify the signature

Each request carries two headers:

- `Virling-Event`: the event type.
- `Virling-Signature`: `t=<unix seconds>,v1=<hex>`, where the hex is the HMAC-SHA256 of `<t>.<raw body>` keyed with the signing secret shown in Site settings.

Compute the HMAC over the raw body before parsing it, compare it in constant time, and reject requests whose `t` is more than five minutes old. In Node.js:

```js
import crypto from "node:crypto";

function verify(secret, header, rawBody) {
  const { t, v1 } = Object.fromEntries(header.split(",").map((p) => p.split("=")));
  const mac = crypto.createHmac("sha256", secret).update(`${t}.${rawBody}`).digest("hex");
  const fresh = Math.abs(Date.now() / 1000 - Number(t)) < 300;
  return fresh && crypto.timingSafeEqual(Buffer.from(mac), Buffer.from(v1));
}
```

## Delivery

Virling checks for new events once a minute and sends them oldest first. A request that fails or answers with a non-2xx status stops that round; the same event is tried again the next minute, and the error shows next to the webhook in Site settings until a send succeeds. Answer quickly: requests time out after 5 seconds. Saving a new URL rotates the signing secret.
