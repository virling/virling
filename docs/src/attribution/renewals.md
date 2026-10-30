# Renewals

A subscription renewal takes the attribution of the customer's first payment. If someone subscribed after a visit from Reddit, every renewal is credited to that Reddit visit, even years later and even though the renewal itself has no visit.

## How a renewal is detected

| Provider | A payment is a renewal when |
| --- | --- |
| Stripe | Its invoice's billing reason is `subscription_cycle`. |
| Dodo Payments | It belongs to a subscription that already had a paid payment before it. |
| Polar | The order's billing reason is `subscription_cycle`. |
| LemonSqueezy | It is a subscription invoice with billing reason `renewal`. |
| Creem | The customer already had a payment before it. |
| RevenueCat | It arrives as a `RENEWAL` webhook event. |
| Paddle | Its origin is `subscription_recurring`, `subscription_update` or `subscription_charge`. |

Upgrades and other plan changes count as renewals too, so they keep the first payment's channel: Stripe and Polar `subscription_update`, LemonSqueezy `updated`, and any later Dodo subscription payment.

## Which payment is the first

The customer's first payment is their earliest payment that did not fail, from the same provider, matched by the provider's customer ID. If that first payment was attributed, the renewal copies its channel, referrer, campaign, landing page, country and device. If it was not, the renewal falls back to its own visitor ID, which renewals rarely carry, so it is usually Unattributed too.

## Free trials

A free trial starts with nothing charged, so there is no payment to carry the visitor ID from checkout. Virling keeps the trial start apart from payments, never counting it as revenue, a payment or a customer, and uses it only for credit: a later payment of that customer with no visitor ID of its own is credited from the visits before the trial started. Stripe trial starts come from `checkout.session.completed` events with a total of 0, Polar from $0 orders with billing reason `subscription_create`, and Dodo Payments from $0 subscription payments. Stripe reads events from the time it was connected, so trials that started before that are not seen.

Payments without a customer ID cannot be linked to a first payment.

The first payment must be in Virling. Payments older than the 90-day backfill are not imported, so renewals of subscriptions that started before that are Unattributed.

Customers are matched per provider. A customer who moved from one provider to another counts as two customers.

[Retention by channel](retention.md) uses the same rule: a customer's whole lifetime value counts toward the channel of their first payment.
