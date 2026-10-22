# LemonSqueezy

Virling imports LemonSqueezy orders and subscription invoices, so they count toward revenue, and links them to visits through a webhook.

## Create an API key

1. In LemonSqueezy, open Settings, API, and create a key. LemonSqueezy's guide: [getting started](https://docs.lemonsqueezy.com/guides/developer-guide/getting-started).
2. Paste it into Virling.

> [!WARNING]
> **The key has full access.** LemonSqueezy API keys cannot be made read-only and cover every store on your account. Virling reads orders, subscriptions with their prices and store currency, and subscription invoices, adds the webhook described below to each store, and stores the key encrypted. Delete the key in LemonSqueezy at any time to revoke access.

## What is imported

- **Orders** and **subscription invoices** from the last 90 days on the first run, then new ones every minute. A subscription's first charge is imported once, as its order.
- **Refunds.** Refunds do not change an order's creation time, so Virling rescans the last 60 days for refunds every 6 hours.
- **Renewals.** An invoice with billing reason `renewal` is marked as a renewal.

## Checkout attribution

LemonSqueezy only returns checkout custom data in webhooks. When you connect LemonSqueezy, Virling registers a webhook on each of your stores for `order_created` and `subscription_payment_success`, pointing at `/api/webhooks/lemonsqueezy/` on your Virling address, with a signing secret it stores encrypted. Each webhook is checked against its `X-Signature` before the visitor ID is attached to the order or invoice. Connecting again with a new key updates the same webhook.

The webhook needs `VIRLING_BASE_URL` to be a public `https://` address that LemonSqueezy can reach. Without one, no webhook is registered and LemonSqueezy payments show as Unattributed.

Pass the visitor ID into checkout as custom data. In the browser:

```js
const url = new URL("https://STORE.lemonsqueezy.com/checkout/buy/VARIANT_ID");
const vid = window.virling?.vid?.();
if (vid) url.searchParams.set("checkout[custom][virling_vid]", vid);
location.href = url.href;
```

Or on your server, when you create a checkout with `POST /v1/checkouts`, set `data.attributes.checkout_data.custom` to `{ "virling_vid": "<the visitor's virling_vid cookie>" }`.
