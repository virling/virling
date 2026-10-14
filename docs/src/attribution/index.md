# How attribution works

Attribution answers one question per payment: which visit brought it. Virling credits exactly one session per payment.

## Sessions

Pageviews and goals from one visitor form a session until there are 30 minutes without an event. The next event after that starts a new session.

A session is described by its first pageview: its channel, referrer, campaign, landing page, country and device. That first pageview is the session's touch.

## Matching a payment

After every sync round, and when you change the model or window, Virling rebuilds attribution for the site:

1. Take the payment's visitor ID, which checkout carried. See [Link checkout to visits](../payments/checkout.md).
2. Find that visitor's sessions that started before the payment and inside the [lookback window](model.md), 90 days by default.
3. Pick one session with the site's [model](model.md): the latest session that was not Direct, by default.
4. Credit the whole payment, after refunds, to that session's channel, referrer, campaign and landing page.

[Renewals](renewals.md) skip steps 1 to 3 and take the attribution of the customer's first payment.

A payment with no visitor ID, or with no session in the window, is [Unattributed](unattributed.md). It still counts toward revenue.

## Where you see it

- **Dashboard** at `/app/yourdomain.com`: revenue, visitors and revenue per visitor, broken down by channel, referrer, landing page and country.
- **Payments** at `/app/yourdomain.com/payments`: every imported payment with its credited channel, or Unattributed.
- **Payment journey**: open a payment to see the payer's sessions in the window before it, oldest first, with the credited session marked.

## Rebuilds

Attribution is a derived table. It is rebuilt from events and payments, so changing the model or window recomputes history for the whole site. Nothing is lost by switching back and forth.
