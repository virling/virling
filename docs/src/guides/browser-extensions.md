# Set up Virling for a browser extension

This guide sets up Virling for a paid Chrome or Firefox extension, so you can see which channel brought the users who upgrade, not only the ones who install: add the script to your landing page, tag the links that lead to it, send upgrades through your own site with the visitor ID, and connect your payment provider. To start, sign in at [app.virling.com](https://app.virling.com/login) for the hosted plan, or [self-host it](../quickstart.md).

## What you need

- A landing page on your own domain, where you can add one tag to the `<head>`.
- A pricing or upgrade page on the same domain or a subdomain. The extension opens this page instead of opening checkout itself.
- A payment provider that Virling [reads](../payments/index.md).

## 1. Add the script

1. Click **+ Add a site** in the sidebar, or open `/app/setup`, and enter your domain, such as `yourextension.com`, and its timezone.
2. Open `/app/yourextension.com/setup/script` for the exact tag, and paste it before `</head>` on your landing page, your upgrade page and every other page of your site:

```html
<script defer src="https://app.virling.com/v.js"
  data-site="yourextension.com"></script>
```

Put it on your site, not in the extension: only events from pages on your domain and its subdomains are kept. The `virling_vid` cookie is set on your registrable domain for one year, so a landing page on `yourextension.com` and an upgrade page on `app.yourextension.com` share the same visitor ID. See [Add the script](../track/script.md).

Leave [cookieless mode](../track/cookieless.md) off: without the `virling_vid` cookie there is no visitor ID to pass on.

## 2. Tag your links

Send people to your landing page first and to the store from there. A link that goes straight to the store skips your site, so there is no visit to credit.

- **Posts, newsletters, videos and creators.** Make one [tracked link](../track/links.md) per place, with your landing page as the destination. Each one shows under the Campaign tab. You can also write the tags yourself: `utm_source`, `utm_medium` and `utm_campaign` are stored with the pageview. See [UTM tags and ref](../track/script.md#utm-tags-and-ref).
- **The link from your store listing to your site.** Tag the website link on the listing, with a different `utm_campaign` per store, and look for them under the Campaign tab:

```text
https://yourextension.com/?utm_source=chrome-web-store&utm_medium=referral&utm_campaign=chrome-listing
https://yourextension.com/?utm_source=firefox-add-ons&utm_medium=referral&utm_campaign=firefox-listing
```

- **The link from your site to your store listing.** A click on it is recorded as an exit with the listing's origin and path. Tags on this link are not kept, because an exit is stored without its query string. See [Outbound links](../track/script.md#outbound-links).

## 3. Carry the visitor ID to the upgrade

Point the upgrade button in the extension at your upgrade page:

```html
<a href="https://yourextension.com/upgrade" target="_blank">Upgrade</a>
```

Someone who visited your landing page in the same browser still has the `virling_vid` cookie, so the upgrade page sees the same visitor ID. Leave UTM tags off this link. They would mark the upgrade visit with a source of its own.

Checkout then has to start from that page:

- **Checkout links.** Links on `buy.stripe.com`, `buy.polar.sh`, `checkout.dodopayments.com` and `*.lemonsqueezy.com` need no code when they are normal `<a href>` links on the upgrade page. The script adds the visitor ID to them. See [Checkout links need no code](../payments/checkout.md#checkout-links-need-no-code).
- **Checkouts you create on your server.** Read the `virling_vid` cookie from the request your upgrade page makes, and pass it into the checkout. With Stripe Checkout:

```js
const session = await stripe.checkout.sessions.create({
  client_reference_id: req.cookies.virling_vid,
  line_items: [{ price: "price_...", quantity: 1 }],
  mode: "subscription",
  success_url: "https://yourextension.com/thanks",
});
```

For Creem, Paddle and the other providers, see [Link checkout to visits](../payments/checkout.md#per-provider).

Do not link to checkout straight from the extension. The script does not run there, so nothing adds the visitor ID to the link and the payment shows as [Unattributed](../attribution/unattributed.md).

## 4. Connect your payment provider

Open `/app/yourextension.com/setup/payments`, pick your provider, paste the key and click **Connect and backfill**. Virling imports past payments, then checks for new ones every minute. LemonSqueezy and Creem return the visitor ID through a webhook, so they need Virling on a public `https://` address. The hosted plan already is. See [Payments](../payments/index.md) for which key to create.

## What cannot be attributed

- **Installs straight from the store search.** Someone who finds the extension in the store and never opens your site has no earlier visit. The first visit Virling sees is the upgrade page the extension opened, so a payment made there is credited to that visit, not to the store search.
- **The install itself.** It happens in the store, which Virling does not see. An exit to your listing is a click, not an install.
- **Use of the extension.** Nothing inside the extension's own pages is tracked.
- **Upgrades in another browser or on another device.** They have their own cookie and a new visitor ID, so the earlier visit is not found.
- **Payments without a visitor ID.** When the script was blocked, checkout did not pass the visitor ID on, or a checkout link was opened from an email, the payment still counts toward revenue, as Unattributed.

## 5. Read which channel brings upgrades

- **Channels** card on the dashboard: visitors, revenue and revenue per visitor for each channel. Use the Source and Campaign tabs to see the posts, sites and UTM campaigns behind a channel.
- **Payments**: every payment with the channel it was credited to. Open one to see its journey.

Which of the visitor's visits gets the credit follows your site's [model and lookback window](../attribution/model.md). See [Read your dashboard](dashboard.md) for the rest.

## Check it works

1. Create a new browser profile and load your extension in it. A private window would not do: it has its own cookies, and extensions are usually off there.
2. Open your landing page from a tagged link, such as `https://yourextension.com/?utm_source=test&utm_medium=test`.
3. Click the upgrade button in the extension and pay in your provider's test mode.
4. Wait a minute, then open `/app/yourextension.com/payments`. The payment should show the `test` source instead of Unattributed, and its journey should start at your tagged visit.

See [Test your setup](test-your-setup.md) for each provider's test mode and the full checklist.

Read more: [Revenue attribution for browser extensions](https://virling.com/integrations/browser-extensions).

[Start free](https://app.virling.com/login) and see which channel brings the users who upgrade.
