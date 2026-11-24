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

## Check that it works

1. Connect a key that starts with `creem_test_`. Virling must be reachable over https.
2. Open a private window and visit a page that runs the script through a tagged link, such as `https://yourdomain.com/?utm_source=test&utm_medium=email`.
3. In the same window, go to a test checkout your server creates with `virling_vid` in `metadata`, and pay with a test card.
4. Wait a minute for the next sync, then open the payments list at `/app/yourdomain.com/payments`. The payment shows the `test` source and the **Email** channel of your tagged visit instead of Unattributed.

Use a new email address for each test: a later payment by the same customer is a renewal and keeps the first payment's channel. If a step fails, see [Test your setup](../guides/test-your-setup.md).

## Common questions

### Does Creem test mode work?

Yes. With a test mode key, Virling reads test transactions and adds its webhook in test mode. A site has one Creem connection, and test payments stay in your reports after you replace the key with a live one.

### Why is a Creem payment Unattributed?

Usually because the checkout had no `virling_vid` in its `metadata`, or no webhook brought it: the payment is from before you connected, or Virling was not reachable over https. After moving to https, set `VIRLING_BASE_URL` and use **Replace key** to add the webhook. See [Unattributed revenue](../attribution/unattributed.md).

### How are Creem refunds and chargebacks counted?

A refund lowers the revenue of its transaction, on that transaction's date, and a chargeback counts as a full refund. Transactions from the last 60 days are read again every 6 hours, so a refund can take that long to show, and one on an older transaction is not picked up automatically.

### How are Creem renewals credited?

Every payment after a customer's first keeps the channel of the first one. The first payment after a free trial is credited to the visit before the trial when it has no visitor ID of its own. If the first payment is older than the first import, renewals are usually Unattributed. See [Renewals](../attribution/renewals.md).

### How far back does Creem history go?

The first import reads transactions from the last 90 days. They count as revenue, but those from before the webhook carry no visitor ID and are usually Unattributed.

### Can I remove the Creem key?

Yes. **Disconnect** under Payment connections in site settings deletes the stored key and stops syncing, and imported payments stay in your reports. Delete the key and Virling's webhook in Creem too, since disconnecting does not remove them there.

## Guides

- [Creem revenue attribution](https://virling.com/integrations/creem)
- [How to track Creem revenue by traffic source](https://virling.com/blog/track-creem-revenue-by-traffic-source)
- [Creem UTM tracking](https://virling.com/blog/creem-utm-tracking)
- [Creem for SaaS: subscriptions by traffic source](https://virling.com/integrations/creem-saas)
