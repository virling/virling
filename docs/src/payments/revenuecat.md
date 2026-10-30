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
