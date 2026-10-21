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
