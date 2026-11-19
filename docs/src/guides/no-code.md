# Set up a no-code site

This guide sets up Virling for a site built on Carrd, Framer, Webflow, Squarespace, Wix or Bubble that sells through checkout links, so every sale shows the channel that brought the buyer: add the script, link checkout, connect your payment provider and make a test purchase. No code is needed beyond pasting one tag. To start, sign in at [app.virling.com](https://app.virling.com/login) for the hosted plan, or [self-host it](../quickstart.md).

## What you need

- A site plan that allows custom code in the `<head>`. Most builders keep it on paid plans.
- Checkout links from Stripe Payment Links, Polar, Dodo Payments or LemonSqueezy.
- Permission to create a key in that payment provider.

## 1. Add the script

1. Click **+ Add a site** in the sidebar, or open `/app/setup`, and enter your domain, such as `yourdomain.com`, and its timezone.
2. Open `/app/yourdomain.com/setup/script` for the exact tag:

```html
<script defer src="https://app.virling.com/v.js"
  data-site="yourdomain.com"></script>
```

3. Paste it where your builder adds code to the `<head>` of every page, then publish:

- [Carrd](../track/frameworks.md#carrd)
- [Framer](../track/frameworks.md#framer)
- [Webflow](../track/frameworks.md#webflow)
- [Squarespace](../track/frameworks.md#squarespace)
- [Wix](../track/frameworks.md#wix)
- [Bubble](../track/frameworks.md#bubble)

Custom code runs only on the published site, not in the editor preview. Framer and Wix move between pages without a full reload, and the script counts each page change on its own.

Leave [cookieless mode](../track/cookieless.md) off: without the `virling_vid` cookie there is no visitor ID for checkout to carry.

## 2. Link checkout

Put your checkout links on your pages as normal buttons or links. The script adds the visitor ID to every link on these hosts when the page loads, and again on click:

| Link host | Provider |
| --- | --- |
| `buy.stripe.com` | Stripe Payment Links |
| `buy.polar.sh` | Polar checkout links |
| `checkout.dodopayments.com` | Dodo Payments static payment links |
| `*.lemonsqueezy.com` | LemonSqueezy checkout links |

The link must be a normal `<a href>` link on a page that runs the script. A parameter already on the link is never replaced, and links to other providers are left alone. See [Checkout links need no code](../payments/checkout.md#checkout-links-need-no-code).

## 3. Connect your payment provider

Open `/app/yourdomain.com/setup/payments`, pick your provider, paste the key and click **Connect and backfill**. Each page lists the key to create:

- [Stripe](../payments/stripe.md): a restricted key, `rk_live_`. Secret keys are refused.
- [Polar](../payments/polar.md): an organization access token, `polar_oat_`.
- [Dodo Payments](../payments/dodo.md): an API key.
- [LemonSqueezy](../payments/lemonsqueezy.md): an API key. Virling also adds a webhook, which is how LemonSqueezy returns the visitor ID.

Virling imports the last 90 days, then checks for new payments every minute. It never creates, changes or refunds anything. See [Which key to create](../payments/index.md#which-key-to-create).

## 4. Make a test purchase

1. Open your site in a private window from a tagged link, such as `https://yourdomain.com/?utm_source=test&utm_medium=test`.
2. Click your checkout link and pay. Stripe test Payment Links on `buy.stripe.com` and LemonSqueezy checkout links in test mode get the visitor ID like live ones. Polar sandbox and Dodo Payments test links do not, so test those with one small live payment and refund it.
3. Wait a minute for the next sync, then open `/app/yourdomain.com/payments`. The payment should show the `test` source instead of Unattributed. Open it to see its journey.

Payments imported with a test key stay in your reports after you switch to a live key. The full checklist and each provider's test mode are in [Test your setup](test-your-setup.md).

## What does not link

- **The builder's own store checkout.** Virling reads payments only from the [supported providers](../payments/index.md#supported-providers), so a sale through the builder's native store or checkout is not imported. Sell through one of the checkout links above instead.
- **Links opened outside your site.** A checkout link opened from an email, a QR code or another site carries no visitor ID, and its payment shows as [Unattributed](../attribution/unattributed.md). Send those people to a page on your site with a [tagged link](../track/links.md), and let them click checkout there.
- **Another browser.** The visitor ID is a cookie in one browser. A buyer who visits in one browser and pays in another is not linked to the visits in the first one.
- **Other checkout hosts.** Links to other providers, or checkout opened by a script rather than an `<a href>` link, get no visitor ID. See [Link checkout to visits](../payments/checkout.md) for passing it yourself.
- **Visits the script did not see.** An ad blocker, cookieless mode, or a page without the tag leaves no visit to credit. See [Why a payment is Unattributed](../attribution/unattributed.md#why-a-payment-is-unattributed).

Payments that do not link still count toward revenue, as Unattributed.
