# Renewals

A subscription renewal takes the attribution of the customer's first payment. If someone subscribed after a visit from Reddit, every renewal is credited to that Reddit visit, even years later and even though the renewal itself has no visit.

## How a renewal is detected

Any payment after a customer's first one counts as a renewal, with every provider, so a later one-time order by the same customer keeps the first channel too. A payment is also a renewal when the provider marks it as one, even if it is the customer's earliest payment in Virling:

| Provider | Also a renewal when |
| --- | --- |
| Stripe | Its invoice's billing reason is `subscription_cycle` or `subscription_update`. |
| Dodo Payments | It belongs to a subscription that already had a paid payment before it. |
| Polar | The order's billing reason is `subscription_cycle` or `subscription_update`. |
| LemonSqueezy | It is a subscription invoice with billing reason `renewal` or `updated`. |
| Creem | No other case: only an earlier payment of the customer counts. |
| RevenueCat | It arrives as a `RENEWAL` webhook event. |
| Paddle | Its origin is `subscription_recurring`, `subscription_update` or `subscription_charge`. |
| WooCommerce | It is a WooCommerce Subscriptions renewal order. |
| Shopify | Its source is `subscription_contract`, a Shopify Subscriptions renewal. |

## Which payment is the first

The customer's first payment is their earliest payment that did not fail, from the same provider, matched by the provider's customer ID. If that first payment was attributed, the renewal copies its channel, referrer, campaign, landing page, country and device. If it was not, the renewal falls back to its own visitor ID, which renewals rarely carry, so it is usually Unattributed too.

Payments without a customer ID cannot be linked to a first payment. Each guest order on WooCommerce, and each Shopify order without a customer, stands alone.

The first payment must be in Virling. The first import reads the last 90 days of payments, 60 days on Shopify. On RevenueCat it reads only one-time purchases from the last 90 days, and subscription payments from before you connected are not imported. Renewals of a subscription that started earlier are usually Unattributed: the earliest payment in Virling stands in as the first, and it rarely has a visit.

Customers are matched per provider. A customer who moved from one provider to another counts as two customers.

[Retention by channel](retention.md) uses the same rule: a customer's whole lifetime value counts toward the channel of their first payment.

## Free trials

A free trial starts with nothing charged, so there is no payment to carry the visitor ID from checkout. Virling keeps the trial start apart from payments, never counting it as revenue, a payment or a customer, and uses it only for credit: a later payment of that customer with no visitor ID of its own is credited from the visits before the trial started.

- **Stripe:** `checkout.session.completed` events with a total of 0. Stripe reads events from the time it was connected, so trials that started before that are not seen.
- **Polar:** $0 orders with billing reason `subscription_create`.
- **Dodo Payments:** $0 subscription payments.
- **Paddle:** $0 subscription transactions with origin `web`.
- **Creem:** completed checkouts of a subscription with nothing charged, by webhook.
- **RevenueCat:** $0 events in a trial period, by webhook.

Shopify and WooCommerce orders with a total of 0 are not imported.

## Common questions

### How is a customer who cancels and returns credited?

To the channel of their first payment, however long they were away, when they return with the same provider and the same customer ID. The channel that brought them back is not credited, unless the first payment is Unattributed and the new one has its own visitor ID. With a new customer ID they are a new customer, credited from their own visits.

### How is a plan change credited?

A charge for an upgrade or another plan change keeps the channel of the customer's first payment, like any later payment. RevenueCat product change events are not read, so there a plan change counts only when RevenueCat also sends a purchase or renewal.

### What happens when a renewal fails and is paid later?

A failed payment adds no revenue and is never used as the first payment. The payment that succeeds keeps the channel of the customer's first payment. Dodo Payments, Paddle and RevenueCat failures are not imported at all. When the provider updates the same order, invoice or transaction instead of making a new one, the change can take up to 6 hours to show on LemonSqueezy, Creem and WooCommerce, which are read again for 60 days.

### Does changing the model later change old renewals?

Yes. Saving another model or lookback window recomputes every payment of the site, and renewals follow the new credit of the customer's first payment. If no visit is left in the window for that first payment, its renewals are usually Unattributed too. See [Changing either](model.md#changing-either).
