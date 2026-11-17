# Set up Virling for a SaaS product

This guide sets up Virling for a B2B SaaS product, so you can see which channel brings paying customers, not only signups: add the script, connect your billing provider with a read-only key, pass the visitor ID into checkout, track signups and trials, and read revenue and retention by channel. To start, sign in at [app.virling.com](https://app.virling.com/login) for the hosted plan, or [self-host it](../quickstart.md).

## What you need

- Your marketing site and app, where you can add one tag to the `<head>`.
- Access to the code that creates your checkout, or hosted checkout links.
- Permission to create a read-only key in your billing provider.

## 1. Add the script

1. Click **+ Add a site** in the sidebar, or open `/app/setup`, and enter your domain, such as `yourproduct.com`, and its timezone.
2. Open `/app/yourproduct.com/setup/script` for the exact tag, and paste it before `</head>` on every page of your marketing site and your app:

```html
<script defer src="https://app.virling.com/v.js"
  data-site="yourproduct.com"></script>
```

The `virling_vid` cookie is set on your registrable domain, so a site on `yourproduct.com` and an app on `app.yourproduct.com` share the same visitor ID. See [Add the script](../track/script.md), the [framework guides](../track/frameworks.md) and, for apps that change pages without a reload, [Single-page apps](../track/spa.md).

Leave [cookieless mode](../track/cookieless.md) off: without the `virling_vid` cookie there is no visitor ID for checkout to carry.

## 2. Connect your billing provider

Open `/app/yourproduct.com/setup/payments`, pick your provider, paste the key and click **Connect and backfill**. Each page lists the key to create and what is imported:

- [Stripe](../payments/stripe.md)
- [Paddle](../payments/paddle.md)
- [Polar](../payments/polar.md)
- [LemonSqueezy](../payments/lemonsqueezy.md)
- [Dodo Payments](../payments/dodo.md)
- [Creem](../payments/creem.md)
- [RevenueCat](../payments/revenuecat.md), for in-app subscriptions

Virling imports the last 90 days, then checks for new payments every minute. It never creates, changes or refunds anything. See [Which key to create](../payments/index.md#which-key-to-create).

## 3. Pass the visitor ID to checkout

This is the step that links a payment to the visits before it.

- **Hosted checkout links** on `buy.stripe.com`, `buy.polar.sh`, `checkout.dodopayments.com` and `*.lemonsqueezy.com` need nothing: the script adds the visitor ID to them.
- **Checkout you create on your server** needs one field. For Stripe Checkout, set `client_reference_id` to the `virling_vid` cookie; other providers use metadata or custom data.

The code for every provider is in [Link checkout to visits](../payments/checkout.md#per-provider), and the [checkout snippet generator](https://virling.com/tools/checkout-snippet-generator) writes it for your provider. Payments without a visitor ID still count toward revenue, as [Unattributed](../attribution/unattributed.md).

## 4. Track signups and trials

Send a [goal](../track/goals.md) when someone signs up and when they start a trial:

```js
// After the signup form succeeds
virling("goal", "signup");

// When a trial starts
virling("goal", "trial");
```

Goals show up next to pageviews and in each payment's journey. The Funnel tab of the Goals card runs Visit, then a goal whose name contains `signup`, `sign_up`, `sign-up` or `register`, then one whose name contains `checkout`, then Paid. Every goal is listed with its count on the Goals tab. Add the stub from [Goals](../track/goals.md#calling-before-the-script-loads) if your code can run before the script loads.

## Trials and renewals

A free trial charges nothing, so there is no payment to carry the visitor ID. Stripe, Polar, Dodo Payments, Paddle, Creem and RevenueCat report the trial start, and Virling keeps it apart from payments: it is never counted as revenue, a payment or a customer. A later payment of that customer with no visitor ID of its own is credited to the visits before the trial started. Stripe only sees trials that started after it was connected.

Every renewal takes the channel of the customer's first payment, matched by the provider's customer ID, so a channel keeps the credit for the subscribers it brought, even years later. Upgrades and plan changes count as renewals. Renewals of subscriptions whose first payment is older than the 90-day backfill are Unattributed. See [Renewals](../attribution/renewals.md) for how each provider's renewals are detected.

## 5. Read which channel brings paying customers

- **Channels** card on the dashboard: visitors, revenue and revenue per visitor for each channel. Revenue per visitor is the number to compare. Use the Source and Campaign tabs to see the posts, sites and UTM campaigns behind a channel.
- **Retention** page: customers grouped by the channel of their first payment, with how many still pay, net LTV and 30-day churn. This answers which channel brings customers who stay. See [Retention by channel](../attribution/retention.md).
- **Payments**: every payment with the channel it was credited to. Open one to see its journey.
- **Time to pay**: the median time from first visit to first payment per channel, so you do not cut a channel before its customers have had time to pay.

By default a payment is credited to the latest visit in the last 90 days that was not Direct. B2B buyers often take weeks to decide, so try first touch, or a 365-day window, to see which channels introduce them. See [Model and lookback window](../attribution/model.md) and [Read your dashboard](dashboard.md).

## Common SaaS problems

### Payments are Unattributed

Usually checkout is not carrying the visitor ID, or the visitor's sessions are outside the lookback window. Every cause is in [Unattributed revenue](../attribution/unattributed.md). If checkout cannot carry the ID, you can [match Stripe payments by email](../attribution/unattributed.md#recover-payments-by-email) from signup.

### Direct is too high

- Tag every link you control, such as emails, ads and posts, with UTM parameters, or use [tracked links](../track/links.md).
- Check how visits are classified in [Channels](../attribution/channels.md), including [in-app browsers](../attribution/channels.md#in-app-browsers).
- Add any checkout or sign-in domain your buyers leave to and return from under **Same-visit domains**. See [How attribution works](../attribution/index.md#sessions).
- On the Channels card, Direct is split into typed or bookmarked, a shared link, and a return from checkout.

More checks are in [Troubleshooting](../troubleshooting.md).

## Check it works

1. Open your site in a private window from a tagged link, such as `https://yourproduct.com/?utm_source=test&utm_medium=test`.
2. Sign up and pay the way a customer would, from a page that runs the script. A trial start is not a payment, so test with a paid plan.
3. Wait a minute for the next sync, then open `/app/yourproduct.com/payments`. The payment should show the `test` source instead of Unattributed. Open it to see its journey, with the `signup` goal in it.

Refund the test payment in your provider afterwards.
