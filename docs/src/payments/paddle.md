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

For transactions you create on your server, set `custom_data: { virling_vid: req.cookies.virling_vid }`. For the Next.js App Router, see [Next.js Paddle UTM tracking](https://virling.com/blog/nextjs-paddle-utm-tracking).

Renewals carry no custom data of their own; they keep the channel of the subscription's first payment.

## Check that it works

1. Connect a sandbox key that starts with `pdl_sdbx_`.
2. Open a private window and visit a page that runs the script through a tagged link, such as `https://yourdomain.com/?utm_source=test&utm_medium=email`.
3. In the same window, open a checkout with Paddle.js set to the sandbox environment and `virling_vid` in `customData`, and pay with one of Paddle's test cards.
4. Wait a minute for the next sync, then open the payments list at `/app/yourdomain.com/payments`. The payment shows the `test` source and the **Email** channel of your tagged visit instead of Unattributed.

Use a new email address for each test: a later payment by the same customer is a renewal and keeps the first payment's channel. If a step fails, see [Test your setup](../guides/test-your-setup.md).

## Common questions

### Does the Paddle sandbox work?

Yes. Paste a sandbox key and Virling reads sandbox transactions and refunds. A site has one Paddle connection, and sandbox payments stay in your reports after you replace the key with a live one.

### Does Virling work with Paddle.js overlay checkout?

Yes, overlay and inline checkout both work when you pass `customData` to `Paddle.Checkout.open`. The script does not add the visitor ID to Paddle checkouts by itself.

### Why is a Paddle payment Unattributed?

Usually because the transaction's `custom_data` has no `virling_vid`: checkout was opened without `customData`, or your server created the transaction without it. Custom data on a customer, product or price is not read. See [Unattributed revenue](../attribution/unattributed.md).

### How are Paddle refunds and chargebacks counted?

An approved refund or chargeback lowers the revenue of its transaction, on that transaction's date. Transactions from the last 60 days are read again every 6 hours, so it can take that long to show, and one on an older transaction is not picked up automatically.

### How are Paddle renewals credited?

Subscription renewals, plan changes and any later transaction of the same customer keep the channel of that customer's first payment. The first payment after a free trial is credited to the visit before the trial when it has no visitor ID of its own. If the first payment is older than the first import, renewals are usually Unattributed. See [Renewals](../attribution/renewals.md).

### How far back does Paddle history go?

The first import reads transactions created in the last 90 days. After that, one created more than about 10 minutes before it is paid, such as an invoice, can take up to 6 hours to show, and is not picked up automatically if it was created more than 60 days earlier. Transactions from before the script was on your site count as revenue but are Unattributed.

## Guides

- [Paddle revenue attribution](https://virling.com/integrations/paddle)
- [How to track Paddle revenue by traffic source](https://virling.com/blog/track-paddle-revenue-by-traffic-source)
- [Paddle UTM tracking: pass UTM and custom data to checkout](https://virling.com/blog/paddle-checkout-utm-parameters)
- [Paddle for SaaS: subscriptions by traffic source](https://virling.com/integrations/paddle-saas)
