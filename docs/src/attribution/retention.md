# Retention by channel

The Retention page shows which channels bring customers who keep paying, not only customers who pay once. A channel that sends many signups who leave after a month can look best on a revenue-this-month view and still be worth less than a smaller channel whose customers stay for a year.

## Who counts as a customer

A customer is one provider's customer ID. Payments without a customer ID count as their visitor, else as their own customer. Each customer is grouped by the channel, referrer and campaign credited for their **first payment**, the earliest that did not fail. Renewals keep that first channel, see [Renewals](renewals.md).

The range picker selects customers whose first payment falls in the range. A customer whose earliest payment in Virling is a renewal started before the import began, so they are left out.

## Still paying

A customer is still paying when:

1. **The provider reports a live subscription for them.** Live means:

   | Provider | Live statuses |
   | --- | --- |
   | Stripe | `active`, `trialing`, `past_due` |
   | Dodo Payments | `active`, `on_hold` |
   | Polar | `active`, `trialing`, `past_due` |
   | LemonSqueezy | `on_trial`, `active`, `past_due`, and `cancelled` until its `ends_at` |

   A subscription cancelled at the end of its period stays live until the period ends. When a customer has any subscription with the provider, its status decides, even if they also made one-off payments.

2. **Otherwise, payment recency.** The customer has a payment that was not fully refunded within the last billing interval plus 7 days. The billing interval is the time between their last two such payments, between 7 and 366 days, or 30 days when there is only one.

Subscription status is read with the same read-only key, once an hour. Stripe needs read access to Subscriptions, which the setup already asks for. Dodo Payments and LemonSqueezy keys can always read it. Polar needs the optional `subscriptions:read` scope; without it, Polar customers use payment recency and the page says so.

## MRR

The same hourly read stores each subscription's price, currency and billing interval. MRR is the sum of those prices per month, converted to USD, over subscriptions that are live, past any trial and not ended. It is the list price before discounts and tax. Stripe adds up every item on the subscription; items without a fixed recurring price (metered or tiered) leave that subscription out. LemonSqueezy subscriptions carry only a price ID, so Virling also reads each distinct price and the store's currency. MRR shows on Latest payments, and Customers who stay adds what the customers from the range pay per month now.

## Metrics

| Metric | Meaning |
| --- | --- |
| Customers acquired | Customers whose first payment is in the range. |
| Still paying | Of those, how many are still paying today. |
| First-payment revenue | Gross USD of their first payments. |
| Refunded | USD refunded or lost to disputes on any of their payments. |
| Net LTV | Every payment they made so far, net of refunds. |
| Avg LTV | Net LTV per customer acquired. |
| Refund rate | Share of customers acquired with any refund. |
| 30-day churn | Of the customers whose first payment was more than 30 and at most 60 days ago, whatever the range, the share with no payment (not fully refunded) and no live subscription in the last 30 days. The first payment itself does not count as activity. |
| Share of customers, share still paying | The row's customers over all customers acquired, and its still-paying customers over all still-paying customers. |

## The headline

The page leads with the most striking true statement, such as "X sent 60% of your customers but only 25% of the ones still paying." It names a referrer or channel with at least 5 customers whose share of customers and share of still-paying customers differ by 15 points or more, the biggest gap first. When no gap is that large and there are 60 days of payments, it compares average LTV against the largest source when one is worth at least twice as much. With fewer than 20 customers or fewer than 5 still paying, there is no headline.

## The first 60 days

LTV needs time. Until a site has 60 days of payments, the page leads with refund rate and 30-day churn, and says how many days of payments there are.

The same numbers are in the [Read API](../reference/api.md), `virling query retention` in the [CLI](../reference/cli.md) and the `get_retention` tool of the [MCP server](../reference/mcp.md).
