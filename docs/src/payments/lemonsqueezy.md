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

Checkout links on `STORE.lemonsqueezy.com` need no code: the script adds `checkout[custom][virling_vid]` to them. For a checkout you build yourself, pass the visitor ID as custom data. In the browser:

```js
const url = new URL("https://STORE.lemonsqueezy.com/checkout/buy/VARIANT_ID");
const vid = window.virling?.vid?.();
if (vid) url.searchParams.set("checkout[custom][virling_vid]", vid);
location.href = url.href;
```

Or on your server, when you create a checkout with `POST /v1/checkouts`, set `data.attributes.checkout_data.custom` to `{ "virling_vid": "<the visitor's virling_vid cookie>" }`.

## Check that it works

1. Switch your LemonSqueezy store to test mode, create an API key there and connect it. Virling must be on a public `https://` address.
2. Open a private window and visit a page that runs the script through a tagged link, such as `https://yourdomain.com/?utm_source=test&utm_medium=email`.
3. In the same window, click a test mode checkout link on your page and pay with a test card.
4. Wait a minute for the next sync, then open the payments list at `/app/yourdomain.com/payments`. The payment shows the `test` source and the **Email** channel of your tagged visit instead of Unattributed.

Use a new email address for each test: a later payment by the same customer is a renewal and keeps the first payment's channel. If a step fails, see [Test your setup](../guides/test-your-setup.md).

## Common questions

### Does LemonSqueezy test mode work?

Yes. With an API key created in test mode, Virling reads test orders and creates its webhook in test mode. A site has one LemonSqueezy connection, and test orders stay in your reports after you replace the key with a live one.

### Why is a LemonSqueezy payment Unattributed?

Usually because no webhook brought its visitor ID: Virling was not on a public `https://` address when you connected, the order is from before the webhook, or a checkout you create has no `virling_vid` in its custom data. Set `VIRLING_BASE_URL`, then use **Replace key** to add a missing webhook. See [Unattributed revenue](../attribution/unattributed.md).

### How are LemonSqueezy refunds counted?

Full and partial refunds lower the revenue of the order or invoice they belong to, on its own date. Orders and invoices from the last 60 days are read again every 6 hours, so a refund can take that long to show, and one on an older order is not picked up automatically.

### How are LemonSqueezy renewals credited?

Invoices with billing reason `renewal` or `updated`, and any later order by the same customer, keep the channel of that customer's first order. If that order is older than the first import, its renewals are usually Unattributed. See [Renewals](../attribution/renewals.md).

### How far back does LemonSqueezy history go?

The first import reads orders and subscription invoices from the last 90 days. They count as revenue, but those from before the webhook are Unattributed.

### Can I remove the LemonSqueezy key?

Yes. **Disconnect** under Payment connections in site settings deletes the stored key and stops syncing, and imported payments stay in your reports. Delete the key and Virling's webhook in LemonSqueezy too, since disconnecting does not remove them there.

## Guides

- [LemonSqueezy revenue attribution](https://virling.com/integrations/lemonsqueezy)
- [How to track LemonSqueezy revenue by traffic source](https://virling.com/blog/track-lemonsqueezy-revenue-by-traffic-source)
- [LemonSqueezy UTM tracking](https://virling.com/blog/lemonsqueezy-utm-tracking)
- [LemonSqueezy for digital products: sales by traffic source](https://virling.com/integrations/lemonsqueezy-digital-products)
