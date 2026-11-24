# RevenueCat

Virling reads RevenueCat purchases, renewals and refunds through a webhook it adds with your key, and imports earlier one-time purchases once when you connect.

## Create a key

1. In RevenueCat, open your project's settings, then **API keys**, and create a **V2 secret key**. RevenueCat's guide: [authentication](https://www.revenuecat.com/docs/projects/authentication).
2. Give it read access to **Project configuration** and **Customer information**, and read and write access to **Integrations**, so Virling can add its webhook. Leave everything else without access.
3. Paste it into Virling.

The key covers one project. Connect each project to its own site.

## The webhook

When you connect, Virling adds one webhook to the project for initial purchases, renewals, non-renewing purchases and cancellations. It points at `https://<your Virling>/api/webhooks/revenuecat/<site ID>` and sends a random secret as its `Authorization` header, which Virling checks on every delivery and keeps encrypted.

The webhook needs Virling to be reachable over https. Without it, only the one-time purchases from the first import arrive.

## What is imported

- **Purchases and renewals** as they happen, in the currency the customer paid. A renewal keeps the channel of the first payment. See [Renewals](../attribution/renewals.md).
- **Free trials.** A trial start is kept, and the first paid renewal is credited to the visit that started the trial.
- **Refunds.** A cancellation by customer support, or with a negative price, marks the payment refunded.
- **One-time purchases from the last 90 days**, once, when you connect. Virling reads about 40 customers a minute, to stay well under RevenueCat's rate limit. Subscription payments from before you connected are not imported, since RevenueCat lists them per subscription only for some stores.
- **Subscription status** is not read, so [Retention](../attribution/retention.md) uses payment recency for RevenueCat customers.

## What gets credited

A RevenueCat purchase is credited to a visit only when it carries a visitor ID. That covers purchases from a web paywall on a page that runs the script, and app users whose `virling_vid` attribute was set from a web visit. In-app purchases from people who never came through your site have no visit to credit, so they count toward revenue as Unattributed.

## How a payment finds its visitor

Virling reads the customer attribute `virling_vid`. Set it before the purchase, from your app with the RevenueCat SDK's `setAttributes`, or from your server:

```js
await fetch(`https://api.revenuecat.com/v2/projects/${projectId}/customers/${appUserId}/attributes`, {
  method: "POST",
  headers: { authorization: `Bearer ${process.env.REVENUECAT_SECRET_KEY}`, "content-type": "application/json" },
  body: JSON.stringify({ attributes: [{ name: "virling_vid", value: req.cookies.virling_vid }] }),
});
```

That call needs a key with write access to Customer information, separate from the one you give Virling.

## Check that it works

1. Make sure Virling is reachable over https and that its webhook is listed in your RevenueCat project.
2. Open a private window and visit a page that runs the script through a tagged link, such as `https://yourdomain.com/?utm_source=test&utm_medium=email`.
3. In the same window, run `window.virling.vid()` in the browser console, and set that value as the `virling_vid` attribute of a new test customer, through your web paywall, your app or the API call above.
4. Make a sandbox purchase as that customer.
5. Open the payments list at `/app/yourdomain.com/payments`. The payment arrives by webhook and shows the `test` source and the **Email** channel of your tagged visit instead of Unattributed.

Use a new customer for each test: a later purchase by the same customer is a renewal and keeps the first payment's channel. If a step fails, see [Test your setup](../guides/test-your-setup.md#revenuecat).

## Common questions

### Does the RevenueCat sandbox work?

Yes. Virling does not tell sandbox events from production ones, so sandbox purchases and their renewals are imported like real payments and stay in your reports.

### Do anonymous RevenueCat customers link to a visit?

Yes, when `virling_vid` is set on that customer before the purchase. Virling groups payments by the original app user ID. RevenueCat payments are never matched by email.

### How are RevenueCat refunds counted?

A refund event marks the whole payment refunded, on the purchase's date; partial refunds are not handled separately. For a subscription, RevenueCat sends one only when the latest period is refunded, and a reversed refund is not read. Turning off auto-renewal changes nothing.

### How are RevenueCat renewals and trials credited?

Renewals, and any later purchase by the same customer, keep the channel of that customer's first payment. The first paid renewal after a free trial is credited through its own `virling_vid` when the event carries one, and otherwise to the visit before the trial. Product change events are not read, so a plan change counts only when RevenueCat also sends a purchase or [renewal](../attribution/renewals.md).

### How far back does RevenueCat history go?

The first import reads one-time purchases from the last 90 days, about 40 customers a minute and at most 100 purchases per customer. Earlier subscription payments are not imported, but their later renewals arrive by webhook. A renewal whose first payment is not in Virling is credited only through its own `virling_vid`.

### Why is a RevenueCat payment Unattributed or missing?

It is Unattributed when the customer had no `virling_vid`, or no visit of that visitor falls inside the lookback window. It is missing when the webhook did not reach Virling, since subscription payments are not polled. See [Unattributed revenue](../attribution/unattributed.md).

## Guides

- [RevenueCat revenue attribution](https://virling.com/integrations/revenuecat)
- [How to track RevenueCat revenue by traffic source](https://virling.com/blog/track-revenuecat-revenue-by-traffic-source)
- [RevenueCat for AI apps: subscribers by channel](https://virling.com/integrations/revenuecat-ai-apps)
