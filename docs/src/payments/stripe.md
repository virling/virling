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
