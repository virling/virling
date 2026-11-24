# Troubleshooting

The problems people hit most, with what to check first. On hosted Virling, still stuck? Open **Help and support** from the account menu in the app, or email help@virling.com. Self-hosted: open an issue on [GitHub](https://github.com/virling/virling/issues).

## No visitors show up

1. Open your site, then your browser's network tab. You should see a request to `/api/event` on your Virling server answered with `202`.
2. No request at all: the script tag is missing on that page, or an ad blocker stopped it. Try a private window with extensions off.
3. A request answered with anything but `202`: check that `data-site` matches the domain exactly as you added it, without `www.` or a path. See [Add the script](track/script.md).
4. Your own site sends a `Content-Security-Policy`? Allow the Virling server in `script-src` and `connect-src`.
5. The script stays quiet on `localhost` unless the tag has `data-dev`.

The setup checklist on the dashboard turns **Script installed** green with the first event.

## Payments show as Unattributed

A payment is Unattributed when no visitor ID reached checkout, or the visitor's sessions are outside the lookback window. The usual fix is to pass `virling_vid` into checkout: see [Link checkout to visits](payments/checkout.md). Every cause is listed in [Unattributed revenue](attribution/unattributed.md).

## Payments do not appear at all

1. In Site settings, check that the provider shows as connected and when it last synced.
2. A refused key shows its error there. Create a new read-only key with the permissions on the provider's page under [Payments](payments/index.md) and save it again.
3. Virling adds the webhook for LemonSqueezy, Creem and RevenueCat when you connect, which needs Virling to be reachable over https. Only RevenueCat needs it for payments to appear; the other two need it to link them to visits.

## Checkout is not linked

The setup checklist shows **Link checkout** until a payment arrives with a visitor ID. Open the payment on the Payments page: if it lists no visitor ID, checkout did not carry `virling_vid`. Follow [Link checkout to visits](payments/checkout.md) for your provider, then make a test purchase.

## Problems by payment provider

The most likely problems for each provider. A refused key shows its error on the Connect payments screen and is not stored. Errors that start later show in Site settings.

### Stripe

- **"Use a restricted read-only key, not a secret key."** Keys that start with `sk_` are refused. Create a restricted key, which starts with `rk_live_` or `rk_test_`.
- **The key connects, but sync fails with a 403.** Connecting only tests Charges and PaymentIntents. Set **Read** on Checkout Sessions, Customers, Events, Invoices and Subscriptions too.
- **Metadata is empty on the charge, or the payment is Unattributed.** Metadata on a Checkout Session is not copied to its PaymentIntent or charge, and Virling does not need it to be: it reads the session's `client_reference_id` or `metadata.virling_vid`. For your own PaymentIntents, set `metadata.virling_vid` on the PaymentIntent. Metadata on a customer, subscription or product is not read.

See [Stripe](payments/stripe.md) and [its checkout setup](payments/checkout.md#stripe).

### Paddle

- **"This Paddle API key is missing a permission."** Give the key `transaction.read` and `adjustment.read`, plus `subscription.read` for retention.
- **A transaction is missing.** Only completed and paid Paddle Billing transactions are imported. One created more than about 10 minutes before it was paid can take up to 6 hours, when Virling reads recent transactions again.
- **The payment is Unattributed.** Pass `customData: { virling_vid: ... }` when you open Paddle.js, or set `custom_data` on the transaction you create. Virling reads it from the transaction only, so custom data on a customer, product or price does not count.

See [Paddle](payments/paddle.md) and [its checkout setup](payments/checkout.md#paddle).

### Shopify

- **"Use your store's myshopify.com address."** The Admin API does not answer on a custom domain. Enter `https://your-store.myshopify.com`.
- **"This Shopify token cannot read orders."** Give the custom app the `read_orders` scope, install it, and paste its Admin API access token, which starts with `shpat_`.
- **An order is missing.** Pending and authorized orders wait until they are paid, free orders are skipped, and the first import goes back 60 days.
- **The order is Unattributed.** Check that its note attributes include `virling_vid`. The theme snippet must come after the Virling script in `theme.liquid`. Headless stores set the attribute from their own cart code, and carts made outside your storefront have none. UTM tags that Shopify stores on the order are not read.

See [Shopify](payments/shopify.md) and [its checkout setup](payments/checkout.md#shopify).

### WooCommerce

- **"Paste the consumer key and secret as ck_...:cs_..., joined by a colon."** Paste both parts in one field, not only the consumer key.
- **"No WooCommerce REST API at this store URL."** Turn on pretty permalinks in WordPress and check the store address, so `/wp-json/wc/v3/` answers over https.
- **An order is missing.** Pending and on-hold orders wait until they are Processing or Completed. One that changes later can take up to 6 hours, when Virling reads recent orders again.
- **Orders have no visitor ID.** Virling reads only `virling_vid` from the order's meta data, not UTM fields stored on the order. Add the `functions.php` snippet, which covers the classic and the block checkout, and place a new order: older ones are not filled in, and orders you create in the admin never run it.

See [WooCommerce](payments/woocommerce.md) and [its checkout setup](payments/checkout.md#woocommerce).

### Polar

- **"This Polar token is missing the ... scope."** Create an Organization Access Token with `orders:read` and `refunds:read`, plus `subscriptions:read` for retention.
- **An order is missing.** Unpaid orders are checked every 10 minutes for up to 30 days, and imported once paid.
- **The order is Unattributed.** Set `metadata.virling_vid` on checkouts you create. The script adds `reference_id` to `buy.polar.sh` links only, not to sandbox links, and never replaces one that is already there.

See [Polar](payments/polar.md) and [its checkout setup](payments/checkout.md#polar).

### LemonSqueezy

- **"LemonSqueezy did not accept this API key."** Copy it again from Settings, API. It may have been deleted or expired.
- **Every payment is Unattributed.** The visitor ID arrives only by webhook. If Virling was not on a public `https://` address when you connected, no webhook was added: set `VIRLING_BASE_URL`, then use **Replace key** in Site settings to add it. Orders from before the webhook stay Unattributed.
- **Only checkouts you create are Unattributed.** Set `checkout_data.custom.virling_vid` when you create them. Checkout links on `*.lemonsqueezy.com` need no code.

See [LemonSqueezy](payments/lemonsqueezy.md) and [its checkout setup](payments/checkout.md#lemonsqueezy).

### Dodo Payments

- **"Dodo Payments rejected this API key."** Copy a fresh key from Developer, API Keys. Live and test mode keys both work.
- **A payment is missing.** Only succeeded payments are imported. Check **Only these product IDs** in Site settings: when it is filled in, payments for other products are left out. A large first import can take a few minutes.
- **The payment is Unattributed.** Set `metadata.virling_vid` on the checkout session. The script adds the visitor ID to links on `checkout.dodopayments.com` only, so add `metadata_virling_vid` yourself to test links and to URLs your own code opens.

See [Dodo Payments](payments/dodo.md) and [its checkout setup](payments/checkout.md#dodo-payments).

### Creem

- **"This Creem API key cannot read transactions."** Give the key `transactions:read`, plus `subscriptions:read` for retention.
- **"Creem did not accept the webhook."** Give the key `webhooks:write`, and make sure Virling is on a public https address.
- **The payment is Unattributed.** Set `metadata.virling_vid` when you create the checkout. Creem returns it only by webhook, so payments from before you connected, or while Virling was on plain http, stay Unattributed. After moving to https, set `VIRLING_BASE_URL` and use **Replace key** in Site settings to add the webhook.

See [Creem](payments/creem.md) and [its checkout setup](payments/checkout.md#creem).

### RevenueCat

- **"RevenueCat did not accept this key."** Paste a V2 secret key from your project settings.
- **"This RevenueCat key is missing a permission", or the webhook is refused.** Give the key read access to Project configuration and Customer information, and read and write access to Integrations. The webhook also needs Virling on a public https address.
- **Payments are missing.** New purchases and renewals arrive only by webhook, and subscription payments from before you connected are not imported. If Virling was on plain http when you connected, set `VIRLING_BASE_URL` to its https address and use **Replace key** in Site settings to add the webhook.
- **The purchase is Unattributed.** Set the customer attribute `virling_vid` before the purchase. In-app purchases from people who never came through your site have no visit to credit.

See [RevenueCat](payments/revenuecat.md) and [its checkout setup](payments/checkout.md#revenuecat).

## Event limits on hosted plans

Personal includes 100,000 events a month and Agency 1,000,000. At 80% and at 100% the account owner gets an email and the app shows a banner. Virling never stops collecting and never drops data when you go over. If you keep going over, move to a larger plan from Account settings.

## Numbers differ from another tool

Different tools count visitors, sessions and bots differently, and credit payments with different models. See [How attribution works](attribution/index.md) and [Model and lookback window](attribution/model.md).
