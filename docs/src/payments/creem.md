# Creem

Virling reads Creem transactions and, when allowed, subscriptions with a scoped API key, and adds one webhook so checkouts link to visits.

## Create a key

1. In Creem, open **Developers**, then **API keys**, and create a key. Creem's guide: [API introduction](https://docs.creem.io/api-reference/introduction).
2. Give it the `transactions:read` scope, plus `subscriptions:read` so [Retention](../attribution/retention.md) can tell who still pays, and `webhooks:write` so Virling can add its webhook. No other scopes are needed.
3. Paste it into Virling.

Live and test mode keys both work. Virling tries `api.creem.io` first and `test-api.creem.io` second, or the other way round for a key that starts with `creem_test_`.

## The webhook

Creem returns checkout metadata only in webhooks, not on transactions. When you connect, Virling adds one webhook for `checkout.completed` that points at `https://<your Virling>/api/webhooks/creem/<site ID>`, and keeps its signing secret encrypted. Each delivery is checked against the `creem-signature` header, an HMAC-SHA256 of the body.

The webhook needs Virling to be reachable over https. A server on plain http still imports payments, but they stay Unattributed.

## What is imported

- **Transactions** from the last 90 days on the first run, then new ones every minute, newest first.
- **Refunds and chargebacks.** Transactions from the last 60 days are read again every 6 hours, so later refunds update them.
- **Renewals.** Every payment after a customer's first is a renewal and keeps the channel of the first payment. See [Renewals](../attribution/renewals.md).
- **Free trials.** A completed checkout with nothing to pay starts a trial, and the customer's first paid transaction is credited to it.
- **Subscription status**, once an hour, when the key has `subscriptions:read`. Without it, retention falls back to payment recency.

## How a payment finds its visitor

Virling reads `virling_vid` from, in order: the checkout's `metadata`, the subscription's `metadata`, then the customer's `metadata`.

Set it in `metadata` when you create the checkout on your server:

```js
const res = await fetch("https://api.creem.io/v1/checkouts", {
  method: "POST",
  headers: { "x-api-key": process.env.CREEM_API_KEY, "content-type": "application/json" },
  body: JSON.stringify({
    product_id: "prod_...",
    success_url: "https://yourdomain.com/thanks",
    metadata: { virling_vid: req.cookies.virling_vid },
  }),
});
// Send the customer to (await res.json()).checkout_url
```

Use `https://test-api.creem.io` with a test mode key.
