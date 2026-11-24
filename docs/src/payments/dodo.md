# Dodo Payments

Virling reads Dodo Payments payments, refunds and subscriptions with an API key.

## Create an API key

1. Create an API key in the Dodo Payments dashboard. Dodo's guide: [API reference introduction](https://docs.dodopayments.com/api-reference/introduction).
2. Paste it into Virling.

Live mode and test mode keys both work. Virling tries the key against `live.dodopayments.com` first and `test.dodopayments.com` second, and remembers which one accepted it.

## What is imported

- **Payments** from the last 90 days on the first run, then new ones every minute. Listing walks one day at a time and re-reads the last 30 minutes on each round, so payments that succeed late are not missed.
- **Refunds and disputes.** Refunds update the payment they belong to. Lost or accepted disputes count as refunded, since the money went back to the customer.
- **Renewals.** For a subscription payment, Virling looks up the subscription's paid payments. Every payment after the first paid one is marked as a renewal, so the first charge after a free trial counts as the first payment.

To stay inside Dodo's limit of 240 requests per minute, Virling makes at most a couple of extra lookups per round and continues on the next round. A large backfill can take a few minutes to finish.

## Only some products

When one Dodo account sells more than this site's products, list the site's product IDs in site settings, under Payment connections, in **Only these product IDs**, separated by commas. Virling then imports only payments, refunds and subscriptions for those products. A subscription payment counts for its subscription's product. Leave it empty to import every product.

Saving the list imports the last 90 days again with it. Payments already imported for other products stay; disconnect and connect again to start clean.

## How a payment finds its visitor

Virling reads `metadata.virling_vid` on the payment.

### Payment links

Static payment links on `checkout.dodopayments.com/buy/...` need no code. When a visitor clicks one, the script adds `metadata_virling_vid=<visitor ID>` to the link, unless it is already there, and Dodo stores it as the payment's `metadata.virling_vid`. The link must be a normal `<a href>` on a page that runs the script.

### Checkout sessions

For a checkout session you create on your server, set it in `metadata` from the `virling_vid` cookie:

```js
const session = await client.checkoutSessions.create({
  product_cart: [{ product_id: "pdt_...", quantity: 1 }],
  return_url: "https://yourdomain.com/thanks",
  metadata: { virling_vid: req.cookies.virling_vid },
});
// Send the customer to session.checkout_url
```

If you build a payment link URL in your own code instead of an `<a href>`, add the parameter yourself:

```js
const url = new URL("https://checkout.dodopayments.com/buy/pdt_...");
const vid = window.virling?.vid?.();
if (vid) url.searchParams.set("metadata_virling_vid", vid);
location.href = url.href;
```

## Check that it works

1. Connect a test mode key.
2. Open a private window and visit a page that runs the script through a tagged link, such as `https://yourdomain.com/?utm_source=test&utm_medium=email`.
3. In the same window, go to a test checkout your server creates with `metadata.virling_vid`, or a test link with `metadata_virling_vid` added in your own code, and pay with a test card. The script does not add the visitor ID to test links.
4. Wait a minute for the next sync, then open the payments list at `/app/yourdomain.com/payments`. The payment shows the `test` source and the **Email** channel of your tagged visit instead of Unattributed.

Use a new email address for each test: a later payment by the same customer is a renewal and keeps the first payment's channel. If a step fails, see [Test your setup](../guides/test-your-setup.md).

## Common questions

### Does Dodo Payments test mode work?

Yes. With a test mode key, Virling reads test payments and refunds. A site has one Dodo Payments connection, and test payments stay in your reports after you replace the key with a live one.

### Why is a Dodo payment Unattributed?

Usually because the payment has no `metadata.virling_vid`: the checkout session was created without it, or a payment link was opened from an email or another site, or built in code without `metadata_virling_vid`. See [Unattributed revenue](../attribution/unattributed.md).

### How are Dodo refunds and disputes counted?

A succeeded refund lowers the revenue of its payment, on that payment's date, and one made after you connect counts even when the payment is older than 90 days. A lost or accepted dispute counts as a full refund when Virling reads the payment, so for a later one use **Replace key** to read the last 90 days again.

### How are Dodo renewals credited?

Every payment after a customer's first keeps the channel of the first one. The first payment after a free trial is credited to the visit before the trial when it has no visitor ID of its own. If the first payment is older than the first import, renewals are usually Unattributed. See [Renewals](../attribution/renewals.md).

### How far back does Dodo history go?

The first import reads payments from the last 90 days. Payments from before the script was on your site count as revenue but are Unattributed.

### Can I remove the Dodo key?

Yes. **Disconnect** under Payment connections in site settings deletes the stored key and stops syncing, and imported payments stay in your reports. Delete the key in Dodo Payments too to revoke access completely.

## Guides

- [Dodo Payments revenue attribution](https://virling.com/integrations/dodo-payments)
- [How to track Dodo Payments revenue by traffic source](https://virling.com/blog/track-dodo-payments-revenue-by-traffic-source)
- [Dodo Payments UTM tracking](https://virling.com/blog/dodo-payments-utm-tracking)
- [Dodo Payments for SaaS: subscriptions by traffic source](https://virling.com/integrations/dodo-payments-saas)
