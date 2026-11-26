# Find which channel brings paying customers

Visitor counts tell you which channel is loud. This guide sets Virling up so every payment shows the channel that brought it, then shows how to read the numbers and act on them each week. To start, sign in at [app.virling.com](https://app.virling.com/login) for the hosted plan, or [self-host it](../quickstart.md). With a server that already runs Docker, self-hosting takes about 15 minutes.

It works with Stripe, Dodo Payments, Polar, LemonSqueezy, Creem, RevenueCat, Paddle, WooCommerce and Shopify. LemonSqueezy payments link to visits through a webhook, which needs Virling on a public `https://` address. The hosted plan already is.

## What you need

- A Virling account on the [hosted plan](../hosted.md), or your own Linux or macOS server with Docker or one of the [release binaries](../install/binary.md), and a domain such as `stats.example.com` pointing at it.
- Your site, where you can add one tag to the `<head>`.
- A payment account on Stripe, Dodo Payments, Polar, LemonSqueezy, Creem, RevenueCat or Paddle, or a WooCommerce or Shopify store.

## 1. Start Virling and add your site

**Hosted.** Sign in at [app.virling.com](https://app.virling.com/login), click **+ Add a site** in the sidebar, or open `/app/setup`, and enter your domain and its timezone.

**Self-hosted.** Start the server and add the site from the command line:

```sh
docker run -d --name virling \
  -p 8080:8080 \
  -v virling-data:/data \
  -e VIRLING_BASE_URL=https://stats.example.com \
  ghcr.io/virling/virling:latest serve --app

docker exec virling virling add-site yourdomain.com --owner you@example.com
```

Put HTTPS in front of it with your usual [reverse proxy](../self-host/reverse-proxy.md), then sign in at `https://stats.example.com/login`. Without SMTP, the sign-in link is printed to `docker logs virling`.

## 2. Add the script

Paste this before `</head>` on every page:

```html
<script defer src="https://app.virling.com/v.js"
  data-site="yourdomain.com"></script>
```

Self-hosted, the `src` is your own server's address. The script is about 1.4 KB gzipped and sets one first-party cookie, `virling_vid`, which is the visitor ID that checkout will carry. Do not turn on [cookieless mode](../track/cookieless.md) for this site: without the visitor ID, payments cannot be linked to visits.

Tag the links you share with UTM parameters, or use [tracked links](../track/links.md), so each post and campaign shows up by name.

## 3. Connect payments with a read-only key

In the app, open your site, then Setup, then Connect payments, pick your provider and paste its key. [Which key to create](../payments/index.md#which-key-to-create) lists the key and access for each one. LemonSqueezy keys cannot be limited, and the Creem and RevenueCat keys also need access to add a webhook.

Virling imports the last 90 days (60 for Shopify, and one-time purchases only for RevenueCat), then checks for new payments every minute, or receives them by webhook for RevenueCat. It never creates, changes or refunds a payment.

## 4. Pass the visitor ID into checkout

This is the step that turns traffic numbers into revenue numbers.

- **Checkout links need nothing.** Links on `buy.stripe.com`, `buy.polar.sh`, `checkout.dodopayments.com` and `*.lemonsqueezy.com` get the visitor ID from the script, when they are normal `<a href>` links on a page that runs it.
- **Checkout you create on your server** needs one line. For Stripe Checkout:

```js
const session = await stripe.checkout.sessions.create({
  client_reference_id: req.cookies.virling_vid,
  line_items: [{ price: "price_...", quantity: 1 }],
  mode: "subscription",
  success_url: "https://yourdomain.com/thanks",
});
```

- **A store** needs one snippet, in the WooCommerce theme's `functions.php` or the Shopify theme's `theme.liquid`.

| Provider | What carries the visitor ID | Details |
| --- | --- | --- |
| Stripe | `client_reference_id` or `metadata.virling_vid` on the Checkout Session, or `metadata.virling_vid` on the PaymentIntent | [Stripe](../payments/stripe.md) |
| Dodo Payments | `metadata.virling_vid` on the payment | [Dodo Payments](../payments/dodo.md) |
| Polar | `metadata.virling_vid` on the checkout, or `reference_id` on a checkout link | [Polar](../payments/polar.md) |
| LemonSqueezy | `virling_vid` in the checkout custom data, returned by webhook | [LemonSqueezy](../payments/lemonsqueezy.md) |
| Creem | `metadata.virling_vid` on the checkout, returned by webhook | [Creem](../payments/creem.md) |
| RevenueCat | The customer attribute `virling_vid`, set before the purchase and returned by webhook | [RevenueCat](../payments/revenuecat.md) |
| Paddle | `virling_vid` in `customData` with Paddle.js, or in `custom_data` on your server | [Paddle](../payments/paddle.md) |
| WooCommerce | `virling_vid` in the order's meta data, saved by the snippet | [WooCommerce](../payments/woocommerce.md) |
| Shopify | `virling_vid` in the order's note attributes, saved by the snippet as a cart attribute | [Shopify](../payments/shopify.md) |

The LemonSqueezy, Creem and RevenueCat webhooks need Virling on a public `https://` address. Virling adds them when you connect. The code for every provider is in [Link checkout to visits](../payments/checkout.md#per-provider).

Check it: make a test payment from a page that runs the script, wait a minute, and open the payments list. The payment should show a channel instead of Unattributed.

## 5. Read which channel pays

Open the dashboard and look at the Channels card with revenue next to visitors.

- **Per visitor** is the number to compare. It is revenue divided by visitors for that channel, so a small channel that pays well stands out from a big one that doesn't.
- **Payments** lists every payment with its channel. Open one to see its journey: the sessions before it and which one got the credit.
- **Referrers and campaigns** break a channel down to the post, site or UTM campaign that brought the money.

By default a payment is credited to the latest visit in the last 90 days that was not Direct. Someone who found you on a forum and came back a week later by typing your URL is credited to the forum. To see which channels introduce buyers instead, switch to first touch. See [Model and lookback window](../attribution/model.md).

## 6. The weekly five-minute check

Once a week, with at least a few weeks of data:

1. Sort channels by revenue, then look at per visitor.
2. Find the channel with lots of visitors and little revenue. That is where effort goes without paying back.
3. Find the channel with few visitors and high per-visitor revenue. That is where more effort is most likely to pay.
4. Open the last few payments and read their journeys, to see what paying customers did before they paid.
5. Change one thing, and compare next week.

Renewals keep the channel of the customer's first payment, so a channel keeps its credit for the subscribers it brought. [Retention by channel](../attribution/retention.md) shows which channels bring customers who stay.

## When numbers look wrong

- **Most revenue is Unattributed.** Checkout is not carrying the visitor ID. See [Unattributed revenue](../attribution/unattributed.md).
- **A channel is missing.** Check how the visit was classified in [Channels](../attribution/channels.md), and add UTM tags to links you control.
- **Old payments have no channel.** Payments are backfilled 90 days (60 for Shopify, and one-time purchases only for RevenueCat), but visits only exist from when the script was added.
