# Renewals

A subscription renewal takes the attribution of the customer's first payment. If someone subscribed after a visit from Reddit, every renewal is credited to that Reddit visit, even years later and even though the renewal itself has no visit.

## How a renewal is detected

| Provider | A payment is a renewal when |
| --- | --- |
| Stripe | Its invoice's billing reason is `subscription_cycle`. |
| Dodo Payments | It belongs to a subscription and was made more than 12 hours after the subscription started. |
| Polar | The order's billing reason is `subscription_cycle`. |
| LemonSqueezy | It is a subscription invoice with billing reason `renewal`. |

## Which payment is the first

The customer's first payment is their earliest payment that did not fail, from the same provider, matched by the provider's customer ID. If that first payment was attributed, the renewal copies its channel, referrer, campaign, landing page, country and device. If it was not, the renewal falls back to its own visitor ID, which renewals rarely carry, so it is usually Unattributed too.

Payments without a customer ID cannot be linked to a first payment.

The first payment must be in Virling. Payments older than the 90-day backfill are not imported, so renewals of subscriptions that started before that are Unattributed.

Customers are matched per provider. A customer who moved from one provider to another counts as two customers.
