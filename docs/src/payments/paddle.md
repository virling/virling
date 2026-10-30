# Paddle

Virling reads Paddle Billing transactions, their refunds and, when allowed, subscriptions with a read-only API key.

## Create a key

1. In Paddle, open **Developer tools**, then **Authentication**, and create an API key. Paddle's guide: [API keys](https://developer.paddle.com/api-reference/about/api-keys).
2. Give it the `transaction.read` and `adjustment.read` permissions, plus `subscription.read` so [Retention](../attribution/retention.md) can tell who still pays. No other permissions are needed.
3. Paste it into Virling. It starts with `pdl_live_` or, in sandbox, `pdl_sdbx_`.

Virling tries `api.paddle.com` first and `sandbox-api.paddle.com` second, or the other way round for a sandbox key.

## What is imported

- **Completed transactions** from the last 90 days on the first run, then new ones every minute, newest first.
- **Refunds and chargebacks.** Approved refund and chargeback adjustments lower the transaction's revenue. Transactions from the last 60 days are read again every 6 hours, so later refunds show up. Pending refunds count once Paddle approves them.
- **Renewals.** A transaction from a subscription renewal or plan change is marked as a renewal and keeps the channel of the first payment. See [Renewals](../attribution/renewals.md).
- **Free trials.** A subscription's first transaction with nothing to pay starts a trial, and the customer's later payments are credited to it.
- **Subscription status**, once an hour, when the key has `subscription.read`. Without it, retention falls back to payment recency.

## How a payment finds its visitor

Virling reads `virling_vid` from the transaction's `custom_data`. With Paddle.js, pass it as `customData` when you open checkout:

```js
Paddle.Checkout.open({
  items: [{ priceId: "pri_...", quantity: 1 }],
  customData: { virling_vid: window.virling?.vid?.() },
});
```

For transactions you create on your server, set `custom_data: { virling_vid: req.cookies.virling_vid }`.

Renewals carry no custom data of their own; they keep the channel of the subscription's first payment.
