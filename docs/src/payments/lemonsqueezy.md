# LemonSqueezy

Virling imports LemonSqueezy orders and subscription invoices, so they count toward revenue. Linking them to visits is <span class="na">Not available yet</span>.

## Create an API key

1. In LemonSqueezy, open Settings, API, and create a key. LemonSqueezy's guide: [getting started](https://docs.lemonsqueezy.com/guides/developer-guide/getting-started).
2. Paste it into Virling.

> [!WARNING]
> **The key has full access.** LemonSqueezy API keys cannot be made read-only and cover every store on your account. Virling only reads orders and subscription invoices, and stores the key encrypted. Delete the key in LemonSqueezy at any time to revoke access.

## What is imported

- **Orders** and **subscription invoices** from the last 90 days on the first run, then new ones every minute. A subscription's first charge is imported once, as its order.
- **Refunds.** Refunds do not change an order's creation time, so Virling rescans the last 60 days for refunds every 6 hours.
- **Renewals.** An invoice with billing reason `renewal` is marked as a renewal.

## Checkout attribution

LemonSqueezy only returns checkout custom data in webhooks, and Virling does not receive webhooks yet ([issue #15](https://github.com/virling/virling/issues/15)). Until then, LemonSqueezy payments show as Unattributed.

You can tag checkouts now, and they will match once webhook support ships. In the browser:

```js
const url = new URL("https://STORE.lemonsqueezy.com/checkout/buy/VARIANT_ID");
const vid = window.virling?.vid?.();
if (vid) url.searchParams.set("checkout[custom][virling_vid]", vid);
location.href = url.href;
```

Or on your server, when you create a checkout with `POST /v1/checkouts`, set `data.attributes.checkout_data.custom` to `{ "virling_vid": "<the visitor's virling_vid cookie>" }`.
