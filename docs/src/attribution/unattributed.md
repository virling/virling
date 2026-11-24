# Unattributed revenue

An Unattributed payment counts toward revenue but is not credited to any visit. In charts it is listed last.

## Why a payment is Unattributed

- **No visitor ID reached checkout.** The most common cause. Pass `virling_vid` into checkout: see [Link checkout to visits](../payments/checkout.md). Check that your server reads the cookie on the request that creates the checkout.
- **Cookieless mode.** Without a cookie there is no visitor ID to pass. The same goes for a visitor who has not accepted under **Until consent**. See [Cookieless mode](../track/cookieless.md).
- **No session in the window.** The visitor ID is known, but their sessions all started before the [lookback window](model.md), or after the payment.
- **The visit was never tracked.** The script was missing on that page, blocked by an ad blocker, the visitor had Global Privacy Control with `data-respect-gpc`, or the visit was filtered as a bot.
- **The link was opened elsewhere.** A Stripe, Polar, Dodo or LemonSqueezy payment link opened from an email, a QR code or another site carries no visitor ID.
- **LemonSqueezy without its webhook.** LemonSqueezy payments are linked through a webhook that Virling adds when you connect. It needs `VIRLING_BASE_URL` to be a public `https://` address, and the checkout must carry the visitor ID. See [LemonSqueezy](../payments/lemonsqueezy.md).
- **A renewal of an unattributed first payment.** See [Renewals](renewals.md).
- **Imported history.** Payments in the backfill, 90 days or 60 for Shopify, were usually made before the script was on your site.

## Causes by provider

| Provider | Where the visitor ID is read | Usual reason it is missing |
| --- | --- | --- |
| [Stripe](../payments/stripe.md) | `metadata.virling_vid` on the charge, its PaymentIntent or the Checkout Session, or the session's `client_reference_id` | Your server sets neither field, or a Payment Link was opened from an email, a QR code or another site |
| [Paddle](../payments/paddle.md) | `custom_data.virling_vid` on the transaction | Checkout is opened without `customData`, or your server creates the transaction without `custom_data` |
| [Shopify](../payments/shopify.md) | `virling_vid` in the order's note attributes | The theme snippet is missing, or the cart was made outside your storefront, such as a cart permalink or Buy Button |
| [WooCommerce](../payments/woocommerce.md) | `virling_vid` in the order's meta data | The `functions.php` snippet is missing |
| [Polar](../payments/polar.md) | `virling_vid` in the order's `metadata`, its custom field data or the customer's `metadata`, then `reference_id` in the order's `metadata` | The checkout session has no `metadata`, or a checkout link is not a normal link on a page that runs the script |
| [LemonSqueezy](../payments/lemonsqueezy.md) | `virling_vid` in the checkout custom data, sent by webhook | `VIRLING_BASE_URL` was not a public `https://` address when you connected, or a checkout your server creates has no `checkout_data.custom` |
| [Dodo Payments](../payments/dodo.md) | `metadata.virling_vid` on the payment | The checkout session has no `metadata`, or a payment link is built in code without `metadata_virling_vid` |
| [Creem](../payments/creem.md) | `virling_vid` in the checkout's, subscription's or customer's `metadata`, sent by webhook | The checkout has no `metadata`, or Virling was not reachable over https, so the webhook is missing |
| [RevenueCat](../payments/revenuecat.md) | The customer attribute `virling_vid` | The attribute was not set before the purchase, or the buyer never came through your site |

## Recover payments by email

When checkout cannot carry the visitor ID, send the visitor's email once you know it, for example right after signup, as an `identify` event:

```js
fetch("https://stats.example.com/api/event", {
  method: "POST",
  body: JSON.stringify({ n: "identify", u: location.href, s: "example.com", v: window.virling?.vid?.(), m: email }),
});
```

Virling keeps only a SHA-256 hash of the trimmed, lowercased email with a salt of this site. A Stripe, Shopify or WooCommerce payment with no visitor ID whose payer email matches is credited to that visitor. The email is the billing or receipt email on Stripe, the order's email on Shopify and the billing email on WooCommerce. Matching is exact and never crosses sites. On the Payments page such a payment shows **Matched by email**. It applies to payments synced after the identify.

## Why is my revenue unattributed?

1. Open an Unattributed payment from `/app/yourdomain.com/payments`.
2. Look for **Visitor** under **Details**. It is the visitor ID, from checkout or from an email match.
3. **No visitor ID:** checkout did not carry one. Find your provider in [Causes by provider](#causes-by-provider), fix that cause and [make a test payment](../guides/test-your-setup.md). If checkout cannot carry the ID, [match by email](#recover-payments-by-email) on Stripe, Shopify or WooCommerce.
4. **A visitor ID, but still Unattributed:** no visit of that visitor was recorded inside the window before the payment. Choose a longer [lookback window](model.md) if your buyers take longer, and check that the script runs on every page where visitors arrive and is not in [cookieless mode](../track/cookieless.md).
5. **A renewal:** check the customer's first payment instead. If it is Unattributed, or older than the backfill, its renewals usually are too. See [Renewals](renewals.md).
6. **Every LemonSqueezy or Creem payment:** on your own server, the webhook may be missing. Set `VIRLING_BASE_URL` to a public `https://` address, restart Virling, then connect the key again so the webhook is added.

Payments made before a fix usually stay Unattributed, since their checkout carried no visitor ID. New ones are credited.
