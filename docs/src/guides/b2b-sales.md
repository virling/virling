# Set up Virling for sales-led B2B SaaS

This guide is for B2B teams where buyers book a demo, talk to sales, and pay a Stripe invoice weeks after their first visit. It sets up Virling so that invoice is still credited to the channel that brought the buyer: add the script, record demo requests and signups as goals, carry the visitor through to the customer record, and connect Stripe. To start, sign in at [app.virling.com](https://app.virling.com/login) for the hosted plan, or [self-host it](../quickstart.md).

## What you need

- Your marketing site, and your app if you have one, where you can add one tag to the `<head>`.
- Access to the code behind your demo or signup form.
- Permission to create a read-only restricted key in Stripe.

## 1. Add the script

1. Click **+ Add a site** in the sidebar, or open `/app/setup`, and enter your domain, such as `yourproduct.com`, and its timezone.
2. Open `/app/yourproduct.com/setup/script` for the exact tag, and paste it before `</head>` on every page of your marketing site and your app:

```html
<script defer src="https://app.virling.com/v.js"
  data-site="yourproduct.com"></script>
```

The `virling_vid` cookie is set on your registrable domain, so `yourproduct.com` and `app.yourproduct.com` share the same visitor ID. Leave [cookieless mode](../track/cookieless.md) off: without the cookie there is no visitor ID to carry to the invoice. See [Add the script](../track/script.md).

## 2. Record demo requests and signups as goals

Send a [goal](../track/goals.md) when someone books a demo or signs up:

```js
// After the demo form succeeds
virling("goal", "demo request");

// After a signup succeeds
virling("goal", "signup");
```

Goals show up next to pageviews and in each payment's journey, so you can see the demo request that came before an invoice. Add the stub from [Goals](../track/goals.md#calling-before-the-script-loads) if your form code can run before the script loads.

## 3. Carry the visitor through to the customer record

An invoice you send from Stripe does not pass through a checkout on your site, so it arrives with no visitor ID. Virling joins it to the visitor by email instead: when the buyer gives you their email on your demo or signup form, send it once as an `identify` event, with the visitor ID:

```js
// After the demo or signup form succeeds
fetch("https://app.virling.com/api/event", {
  method: "POST",
  body: JSON.stringify({ n: "identify", u: location.href, s: "yourproduct.com", v: window.virling?.vid?.(), m: email }),
});
```

On your server, read the visitor ID from the `virling_vid` cookie instead. Virling keeps only a SHA-256 hash of the trimmed, lowercased email with a salt of this site. A Stripe payment with no visitor ID whose payer email (billing details or receipt email) matches is credited to that visitor, and shows **Matched by email** on the Payments page.

Matching is exact, never crosses sites, and applies to payments synced after the identify, so send it at the demo request, not after the deal closes. See [Recover payments by email](../attribution/unattributed.md#recover-payments-by-email).

If some deals close self-serve through a Checkout Session or PaymentIntent your server creates, pass the visitor ID there too, as `client_reference_id` or `metadata.virling_vid`. Those payments are matched directly, with no email needed. See [Link checkout to visits](../payments/checkout.md).

## 4. Connect Stripe

Open `/app/yourproduct.com/setup/payments`, pick Stripe, paste a restricted key with read access and click **Connect and backfill**. The resources the key needs are listed in [Stripe](../payments/stripe.md#create-a-restricted-key). Virling imports the last 90 days of charges, then checks for new ones every minute. It never creates, changes or refunds anything.

Virling finds the invoice each charge paid. A charge whose invoice has billing reason `subscription_cycle` or `subscription_update` is a renewal and takes the channel of the customer's first payment, matched by Stripe customer ID. So once the first invoice is attributed, the renewals and upgrades of that customer follow it. See [Renewals](../attribution/renewals.md).

## Long sales cycles

### Lookback window

Only sessions that started inside the lookback window before the payment count. The default is 90 days; choose 30, 60, 90 or 365 in site settings at `/app/yourproduct.com/settings`. Pick a window longer than your usual sales cycle, or a buyer's first visits fall outside it. Changing it rebuilds attribution for every past payment. See [Model and lookback window](../attribution/model.md).

### First touch or last touch

- **Last non-direct**, the default, credits the latest session that was not Direct. Buyers who come back by typing your URL before a call do not take the credit from the channel before them.
- **First touch** credits the earliest session in the window: the channel that introduced the buyer. It fits long buying cycles.
- **Last touch** credits the latest session, Direct included: how buyers arrive when they are ready.

The **Time to pay** card shows the median time from first visit to first payment per channel, which helps you choose a window.

### Reading unattributed revenue

Unattributed payments still count toward revenue; they are just not credited to a visit. For invoiced deals the usual causes are:

- **A different payer.** If the invoice is paid by someone other than the person who booked the demo, such as an accounts team, the emails do not match.
- **No identify before the payment.** The form did not send the email, or the invoice was synced before the identify.
- **No session in the window.** The buyer is known, but their visits started before the lookback window.
- **A renewal of an unattributed first payment.**

Keep Unattributed as its own line. A growing share usually means a form stopped sending the email or a checkout stopped sending the visitor ID. Every cause is in [Unattributed revenue](../attribution/unattributed.md).

Read more: [Stripe invoice attribution for sales-led B2B SaaS](https://virling.com/blog/stripe-invoice-attribution) and [B2B SaaS attribution for small teams](https://virling.com/blog/b2b-saas-attribution).

## Check it works

1. Open your site in a private window from a tagged link, such as `https://yourproduct.com/?utm_source=test&utm_medium=test`.
2. Submit your demo or signup form with an email you can pay with.
3. In Stripe test mode, with an `rk_test_` key connected, send an invoice to that email and pay it with a test card.
4. Wait a minute for the next sync, then open `/app/yourproduct.com/payments`. The payment should show the `test` source and **Matched by email**. Open it to see its journey, with the `demo request` goal in it.

The full checklist, per provider, is in [Test your setup](test-your-setup.md).
