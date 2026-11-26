# Set up Virling for a mobile app

This guide sets up Virling for a mobile app that sells subscriptions through RevenueCat, so you can see which marketing channel brings paying subscribers: add the script to your marketing site and web paywall, pass the visitor ID to RevenueCat or your web checkout, connect RevenueCat, and read revenue by channel. To start, sign in at [app.virling.com](https://app.virling.com/login) for the hosted plan, or [self-host it](../quickstart.md).

## What you need

- A marketing site, and a web paywall if you have one, where you can add one tag to the `<head>`.
- Access to your app's RevenueCat SDK code, or to the server that knows each user's RevenueCat app user ID.
- Permission to create a V2 secret key in your RevenueCat project.
- A Virling server reachable over https, for the RevenueCat webhook. The hosted plan already is.

## 1. Add the script

1. Click **+ Add a site** in the sidebar, or open `/app/setup`, and enter your domain, such as `yourapp.com`, and its timezone.
2. Open `/app/yourapp.com/setup/script` for the exact tag, and paste it before `</head>` on every page of your marketing site and your web paywall:

```html
<script defer src="https://app.virling.com/v.js"
  data-site="yourapp.com"></script>
```

The `virling_vid` cookie is set on your registrable domain, so a site on `yourapp.com` and a paywall on `pay.yourapp.com` share the same visitor ID. See [Add the script](../track/script.md).

Leave [cookieless mode](../track/cookieless.md) off: without the `virling_vid` cookie there is no visitor ID to pass on.

## 2. Pass the visitor ID

This is the step that links a purchase to the visits before it. Virling reads the RevenueCat customer attribute `virling_vid`, so set it before the purchase.

- **Web paywall.** On your server, read the `virling_vid` cookie from the request and set it on the customer:

```js
await fetch(`https://api.revenuecat.com/v2/projects/${projectId}/customers/${appUserId}/attributes`, {
  method: "POST",
  headers: { authorization: `Bearer ${process.env.REVENUECAT_SECRET_KEY}`, "content-type": "application/json" },
  body: JSON.stringify({ attributes: [{ name: "virling_vid", value: req.cookies.virling_vid }] }),
});
```

That call needs a key with write access to Customer information, separate from the one you give Virling.
- **In the app.** If your app receives the visitor ID from a web visit, set it with the RevenueCat SDK's `setAttributes` as `virling_vid` before the purchase.
- **Web checkout with another provider.** If you sell on the web through Stripe, Paddle or another provider instead, pass the visitor ID into that checkout as shown in [Link checkout to visits](../payments/checkout.md#per-provider), and connect that provider too.

Purchases without a visitor ID still count toward revenue, as [Unattributed](../attribution/unattributed.md).

## 3. Connect RevenueCat

1. In RevenueCat, open your project's settings, then **API keys**, and create a **V2 secret key**.
2. Give it read access to **Project configuration** and **Customer information**, and read and write access to **Integrations**. Leave everything else without access.
3. Open `/app/yourapp.com/setup/payments`, pick RevenueCat, paste the key and click **Connect and backfill**.

Virling adds one webhook to the project for initial purchases, renewals, non-renewing purchases and cancellations. One key covers one project, so connect each project to its own site. See [RevenueCat](../payments/revenuecat.md).

## What is tracked

- **Purchases and renewals** as they happen, in the currency the customer paid.
- **Free trials.** A trial start is kept apart from payments and never counted as revenue. The first paid renewal is credited to the visit that started the trial.
- **Renewals** keep the channel of the customer's first payment, so a channel keeps the credit for the subscribers it brought. See [Renewals](../attribution/renewals.md).
- **Refunds.** A cancellation by customer support, or with a negative price, marks the payment refunded.
- **One-time purchases from the last 90 days**, imported once when you connect.

## What is not tracked

- **Installs and purchases that never touch the web.** Someone who finds your app in the store and buys in the app has no visit to credit. The purchase counts toward revenue as Unattributed.
- **Store installs.** Virling reads payments from RevenueCat, not installs from the App Store or Google Play.
- **Subscription payments from before you connected.** RevenueCat lists them per subscription only for some stores, so they are not imported.
- **Subscription status.** It is not read, so [Retention](../attribution/retention.md) uses payment recency for RevenueCat customers.
- **Purchases without the webhook.** If Virling is not reachable over https, only the one-time purchases from the first import arrive.

## 4. Read which channel brings subscribers

- **Channels** card on the dashboard: visitors, revenue and revenue per visitor for each channel. Use the Source and Campaign tabs to see the posts, sites and UTM campaigns behind a channel.
- **Payments**: every purchase with the channel it was credited to. Open one to see its journey.
- **Retention** page: customers grouped by the channel of their first payment. See [Retention by channel](../attribution/retention.md).

See [Read your dashboard](dashboard.md) for the rest.

## Check it works

1. Open your marketing site in a private window from a tagged link, such as `https://yourapp.com/?utm_source=test&utm_medium=test`.
2. Go on to your web paywall, or set the `virling_vid` attribute for your test user, then make a sandbox purchase. RevenueCat sends sandbox events to the same webhook.
3. Open `/app/yourapp.com/payments`. The purchase should show the `test` source instead of Unattributed.

See [Test your setup](test-your-setup.md) for the full checklist.
