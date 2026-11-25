# FAQ

Short answers about attribution, payments, privacy and running Virling.

## How do I see which channel brings paying customers?

Add the [script](track/script.md) to your site, connect your payment provider with a read-only key, and [pass the visitor ID into checkout](payments/checkout.md). Virling then credits each payment to the visit that led to it and shows revenue and revenue per visitor for every channel. The [Quickstart](quickstart.md) walks through all three.

## Is Virling free and open source?

Yes. Virling is licensed under AGPL-3.0 and self-hosting is free. Hosted plans at app.virling.com start with a 14-day free trial; see [Hosted plan](hosted.md).

## Is hosted Virling open today?

Yes. Sign in with your email at [app.virling.com](https://app.virling.com/login) to start a 14-day free trial. You add a card, and if you cancel before day 15 nothing is charged. See [Hosted plan](hosted.md).

## Do ad blockers block Virling?

Some block requests to hosts or paths they know as analytics. Serving the script and `/api/event` from your own domain through a proxy helps, see [Reverse proxy](self-host/reverse-proxy.md#serving-the-script-from-your-own-domain). A tested first-party proxy setup per platform is <span class="na">Not available yet</span>. Payments are imported from your provider either way, so blocked visitors still count toward revenue, as Unattributed.

## Does it run on an Intel Mac or Windows?

Intel Macs, yes: releases cover Linux x86_64 and arm64 and macOS on Intel and Apple silicon, and the Docker image is linux/amd64 and linux/arm64. Windows builds are <span class="na">Not available yet</span>. On Windows, use Docker or try [building from source](install/source.md).

## Can I track several sites?

Yes. Add each domain as its own site, with `add-site` or in the app, and use its domain in `data-site`. Each site has its own settings, payment connections and reports. Subdomains of a site are counted as that site.

## Which payment providers work?

Nine: Stripe, Dodo Payments, Polar, LemonSqueezy, Creem, RevenueCat, Paddle, WooCommerce and Shopify. You can connect more than one to the same site. [Which key to create](payments/index.md#which-key-to-create) lists the access each one needs.

## Does it work with Stripe Payment Links and Checkout?

Yes. For Checkout Sessions you create, pass the `virling_vid` cookie as `client_reference_id` or in `metadata.virling_vid`. Payment Links on `buy.stripe.com` need no code: the script adds `client_reference_id` to the link when a visitor clicks it. See [Stripe](payments/stripe.md) and [Stripe Payment Links UTM tracking](https://virling.com/blog/stripe-payment-links-utm-tracking).

## Are subscription renewals credited to the original channel?

Yes. A renewal takes the attribution of the customer's first payment with the same provider, so it keeps that payment's channel even years later. Upgrades and plan changes count as renewals too. Subscriptions whose first payment is older than the 90-day backfill are Unattributed; see [Renewals](attribution/renewals.md).

## Can I attribute revenue when checkout is on another domain?

Yes, as long as the payment carries the visitor ID. A hosted checkout from Stripe, Dodo Payments, Polar, LemonSqueezy or Paddle is matched by the ID you [pass into checkout](payments/checkout.md), and a return from it never starts a new session. Add any other checkout domain under **Same-visit domains** in Site settings. The `virling_vid` cookie is shared across subdomains, but not between two separate domains of your own; see [Checkout on another domain](guides/ecommerce.md#checkout-on-another-domain) and [Cross-domain checkout](https://virling.com/blog/cross-domain-checkout-attribution).

## Does it work with Shopify and WooCommerce stores?

Yes. Connect the store with a read-only key, and add a short snippet that stores the visitor ID on each order: a cart attribute on Shopify, order meta data on WooCommerce. New orders are imported every minute, and refunds and subscription renewals are counted too. See [Set up an online store](guides/ecommerce.md), [Shopify](payments/shopify.md) and [WooCommerce](payments/woocommerce.md).

## Can I self-host it for free?

Yes. Self-hosting is free, with every feature, on your own server. Run the Docker image or a release binary for Linux or macOS, or build from source. See [Install](install/index.md), then the [Quickstart](quickstart.md).

## Can I move from hosted to self-hosted?

Yes, but your history does not move with you: there is no export of a hosted account. Add your sites to your own server, point the script tag at it and connect your payment providers again. Payments are [imported again](payments/index.md#what-virling-reads) as far back as each provider's backfill goes, but past visits stay behind, so payments from before the move are usually Unattributed. Start with the [Quickstart](quickstart.md).

## Can agencies run it for many client sites?

Yes. Add a site per client, each with its own script and payment connection, and group them by client name. Clients can open a branded report through a link without a Virling login, and the [Read API](reference/api.md) reads every site on the account. See [Run Virling for client sites](guides/agencies.md).

## Can I invite team members?

Yes, on an Agency account, hosted or self-hosted. The owner invites people by email in **Account settings > Team**, and each member can open and manage every site on the account. Three users are included, counting the owner and open invites. Self-hosted teams stop at 3, and the hosted Agency plan can add paid seats past that. Only the owner can invite or remove people. Roles other than owner and member are <span class="na">Not available yet</span>. See [Run Virling for client sites](guides/agencies.md).

## Can I import data from another analytics tool?

Yes, when self-hosting. Two commands import daily traffic from other analytics tools' CSV exports: [`virling import-plausible`](track/import-plausible.md) and [`virling import-ga`](track/import-google-analytics.md). Imported days fill dashboard ranges from before your first tracked event, but payments are only credited to tracked visits. The commands run on the server, so hosted accounts cannot use them, and uploading an export in site settings is <span class="na">Not available yet</span>.

## Why is my revenue Unattributed?

See [Unattributed revenue](attribution/unattributed.md). Most often checkout does not carry the `virling_vid` visitor ID.

## Do I need a cookie banner?

That depends on your jurisdiction. Virling sets one first-party cookie, `virling_vid`. [Cookieless mode](track/cookieless.md) sets none, at the cost of revenue attribution. See [Privacy and consent](track/privacy.md).

## Why does the script not track on my laptop?

It skips `localhost`, `127.*` and `[::1]`. Add `data-dev` to the tag to test locally, and add a site for the domain your pages run on.

## Is there an API?

Yes. The [Read API](reference/api.md) returns the dashboard's numbers as JSON with a read-only key, the [MCP server](reference/mcp.md) lets AI assistants read the same data, and the [event endpoint](reference/event-api.md) accepts events from your server.

## How do I report a security problem?

Email security@virling.com. Do not open a public issue.
