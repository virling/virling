# Unattributed revenue

An Unattributed payment counts toward revenue but is not credited to any visit. In charts it is listed last.

## Why a payment is Unattributed

- **No visitor ID reached checkout.** The most common cause. Pass `virling_vid` into checkout: see [Link checkout to visits](../payments/checkout.md). Check that your server reads the cookie on the request that creates the checkout.
- **Cookieless mode.** Without a cookie there is no visitor ID to pass. See [Cookieless mode](../track/cookieless.md).
- **No session in the window.** The visitor ID is known, but their sessions all started before the [lookback window](model.md), or after the payment.
- **The visit was never tracked.** The script was missing on that page, blocked by an ad blocker, the visitor had Global Privacy Control with `data-respect-gpc`, or the visit was filtered as a bot.
- **The link was opened elsewhere.** A Stripe, Polar or Dodo payment link opened from an email, a QR code or another site carries no visitor ID.
- **LemonSqueezy.** LemonSqueezy payments cannot be linked yet. See [LemonSqueezy](../payments/lemonsqueezy.md).
- **A renewal of an unattributed first payment.** See [Renewals](renewals.md).
- **Imported history.** Payments in the 90-day backfill were made before the script was on your site.

## Checking a single payment

Open the payment from `/app/yourdomain.com/payments`. Its details list the visitor ID when checkout carried one. A visitor ID with no journey means the visitor's sessions are outside the window. No visitor ID means checkout did not carry one.
