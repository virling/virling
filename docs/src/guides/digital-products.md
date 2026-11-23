# Sell digital products

This guide is for creators who sell ebooks, templates, courses or other digital products and want to see which post, link or launch sold them, not only which one brought visitors. Do the steps in order and finish with a test purchase. To start, sign in at [app.virling.com](https://app.virling.com/login) for the hosted plan, or [self-host it](../quickstart.md).

## 1. Add the script

Paste the tag from the app's setup screen before `</head>` on every page of your site or sales page. It sets one first-party cookie, `virling_vid`, which is the visitor ID that checkout will carry. Leave [cookieless mode](../track/cookieless.md) off: without the visitor ID, payments cannot be linked to visits. See [Add the script](../track/script.md), and [Set up a no-code site](no-code.md) for Framer, Webflow, Carrd and similar builders.

## 2. Connect your payment provider

Open your site, then Setup, then Connect payments, and paste the key your provider's page asks for. Virling imports the last 90 days, then checks for new payments every minute.

| Provider | Checkout links that need no code | Read more |
| --- | --- | --- |
| LemonSqueezy | `*.lemonsqueezy.com` | [LemonSqueezy](../payments/lemonsqueezy.md) |
| Polar | `buy.polar.sh` | [Polar](../payments/polar.md) |
| Dodo Payments | `checkout.dodopayments.com` | [Dodo Payments](../payments/dodo.md) |
| Stripe Payment Links | `buy.stripe.com` | [Stripe](../payments/stripe.md#payment-links) |
| Creem | None: set `virling_vid` in the checkout's `metadata` on your server | [Creem](../payments/creem.md) |

The script adds the visitor ID to checkout links on these hosts when they are normal `<a href>` links on a page that runs it. LemonSqueezy and Creem return the visitor ID through a webhook, so they need Virling on a public `https://` address. For an overlay or a checkout you create in code, see [Set up a merchant of record checkout](merchant-of-record.md) and [Link checkout to visits](../payments/checkout.md).

## 3. Tag launch and social links

Visits from a bio link, an app or a newsletter often arrive with no referrer and land in Direct. Make one [tracked link](../track/links.md) per place you post: in the app, open **Links** and fill in a name, the page to open, a source and a channel. Each link gets its own short URL and shows by name under the Campaign tab.

Point the link at your sales page, not straight at checkout. A checkout link opened from an email, a QR code or another site carries no visitor ID, and its payment shows as [Unattributed](../attribution/unattributed.md).

For creators, affiliates and sponsors who promote your product, open **Partners**, add a partner and the page it should open, and give them the short link Virling makes.

## 4. Make a test purchase

Open your site in a private window from a tagged link, such as `https://yourdomain.com/?utm_source=test&utm_medium=test`, click your checkout link and pay in your provider's test mode. Wait a minute, then open the payments list. The payment should show the `test` source instead of Unattributed, and its journey should start at your tagged visit. Each provider's test mode is in [Test your setup](test-your-setup.md).

## Read which post sold

- **Channels card.** The **Source** tab splits a channel by source, such as Reddit or a newsletter, and the **Campaign** tab shows each tracked link. Compare revenue per visitor to see which post brought buyers rather than only traffic. See [Find which channel pays](channel-revenue.md).
- **Launches.** Log a launch day under **Launches** to see the visitors, new customers and revenue of that day and the day after. See [Track a launch](launch.md).
- **Partners.** Each creator or affiliate link is ranked by revenue, with visitors, paying customers and refund rate. **Copy report link** gives the partner a read-only page with only their own numbers. Virling does not pay partners. See [Read your dashboard](dashboard.md#the-other-pages).
- **Payments.** Every sale with its source. Open one to see the visits before it and which one got the credit.

Each payment is credited under your site's [model and lookback window](../attribution/model.md). With the default, last non-direct over 90 days, a buyer who found you through a post and came back later by typing your URL is still credited to the post.

[Start free](https://app.virling.com/login) and see which post sold your product.
