# Stripe

Virling reads Stripe charges with a restricted key and attributes them through Checkout Sessions, PaymentIntents and Payment Links.

## Create a restricted key

1. In the Stripe Dashboard, open Developers, API keys, then **Create restricted key**. Stripe's guide: [restricted API keys](https://docs.stripe.com/keys/restricted-api-keys).
2. Give it a name such as "Virling".
3. Set **Read** on these resources and leave everything else at None:
   - Charges
   - Checkout Sessions
   - Customers
   - Events
   - Invoices
   - Subscriptions
   - PaymentIntents
4. Create the key and paste it into Virling. It starts with `rk_live_`, or `rk_test_` for test mode.

Secret keys (`sk_live_`, `sk_test_`) are refused: Virling asks for a key that cannot write. If the key cannot read Charges or PaymentIntents, connecting fails with a message listing the resources it needs.

## What is imported

- **Charges**, newest first, back 90 days on the first run. Pending and uncaptured charges are skipped until they settle.
- **Events** of type `charge.succeeded`, `charge.failed`, `charge.captured`, `charge.refunded`, `charge.refund.updated` and `charge.dispute.closed`, so refunds, lost disputes and late settlements update payments that were already imported.
- **Renewals.** A charge whose invoice has billing reason `subscription_cycle` is marked as a renewal. See [Renewals](../attribution/renewals.md).

Virling uses Stripe API version `2026-09-30.endive` for its requests, whatever your account's default version is. It finds the invoice a charge paid through Invoice Payments.

## How a payment finds its visitor

Virling looks for the visitor ID in this order:

1. `metadata.virling_vid` on the charge.
2. `metadata.virling_vid` on its PaymentIntent.
3. `metadata.virling_vid` or `client_reference_id` on the Checkout Session that created it.

So any of these works:

```js
// Stripe Checkout
await stripe.checkout.sessions.create({
  client_reference_id: req.cookies.virling_vid,
  // ...
});

// Your own PaymentIntent (Elements)
await stripe.paymentIntents.create({
  amount: 4900,
  currency: "usd",
  metadata: { virling_vid: req.cookies.virling_vid },
});
```

If you already use `client_reference_id` for something else, put the ID in the session's `metadata.virling_vid` instead.

## Payment Links

Payment Links on `buy.stripe.com` need no code. When a visitor clicks one, the script adds `client_reference_id=<visitor ID>` to the link, unless the link already has one. Stripe copies it onto the Checkout Session.

The link must be a normal `<a href>` on a page that runs the script. Links opened from emails, QR codes or other sites carry no visitor ID and show as Unattributed.

See [Link checkout to visits](checkout.md) for all providers.

## Check that it works

1. Connect a restricted key that starts with `rk_test_`.
2. Open a private window and visit a page that runs the script through a tagged link, such as `https://yourdomain.com/?utm_source=test&utm_medium=email`.
3. In the same window, go to a test Checkout Session your server creates with the visitor ID, or click a test Payment Link on `buy.stripe.com`, and pay with one of Stripe's test cards.
4. Wait a minute for the next sync, then open the payments list at `/app/yourdomain.com/payments`. The payment shows the `test` source and the **Email** channel of your tagged visit instead of Unattributed.

If your server sets `customer`, use a new customer for each test: a later payment by the same customer is a renewal and keeps the first payment's channel. If a step fails, see [Test your setup](../guides/test-your-setup.md).

## Common questions

### Does Stripe test mode work?

Yes. With an `rk_test_` key, Virling reads test charges and refunds, and the script adds the visitor ID to test Payment Links on `buy.stripe.com` too. A site has one Stripe connection, and test payments stay in your reports after you replace the key with a live one.

### Why is a Stripe payment Unattributed?

Usually because the charge, its PaymentIntent and its Checkout Session carry no visitor ID, or a Payment Link was opened from an email, another site or a custom domain. A `client_reference_id` you set to your own ID is read as the visitor ID unless the session's `metadata.virling_vid` is set. See [Unattributed revenue](../attribution/unattributed.md).

### Does Virling work with Stripe invoices?

Yes, when the invoice is paid with a charge; one marked as paid outside Stripe has no charge and is not imported. The first invoice of a subscription finds its visitor through its Checkout Session. For an invoice without a subscription, Virling reads only the metadata of the charge and its PaymentIntent, so set `metadata.virling_vid` there or [match by email](../attribution/unattributed.md#recover-payments-by-email).

### How are Stripe refunds and disputes counted?

A refund lowers the revenue of its charge, on that charge's date, and one made after you connect counts even when the charge is older than 90 days. A dispute that closes as lost after you connect counts as refunded, and an open or won one changes nothing. **Replace key** reads the last 90 days again without lost disputes.

### How are Stripe renewals credited?

Charges with billing reason `subscription_cycle` or `subscription_update`, and any later charge of the same customer, keep the channel of that customer's first payment. If a free trial started through Checkout after you connected, its first charge is credited to the visit before the trial when it has no visitor ID of its own. If the first payment is older than the first import, renewals are usually Unattributed. See [Renewals](../attribution/renewals.md).

### How far back does Stripe history go?

The first import reads charges from the last 90 days. Charges from before the script was on your site count as revenue but are Unattributed.

## Guides

- [Stripe revenue attribution](https://virling.com/integrations/stripe)
- [How to track Stripe revenue by traffic source](https://virling.com/blog/track-stripe-revenue-by-traffic-source)
- [Stripe Checkout UTM parameters: how to pass them](https://virling.com/blog/stripe-checkout-utm-parameters)
- [Stripe for SaaS: subscriptions by channel](https://virling.com/integrations/stripe-saas)
