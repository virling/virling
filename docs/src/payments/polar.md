# Polar

Virling reads Polar orders, refunds and, when allowed, subscriptions with an organization access token.

## Create a token

1. In Polar, open your organization's settings and create an **Organization Access Token**. Polar's guide: [organization access tokens](https://docs.polar.sh/integrate/oat).
2. Give it the `orders:read` and `refunds:read` scopes, plus `subscriptions:read` so [Retention](../attribution/retention.md) can tell who still pays. No other scopes are needed.
3. Paste it into Virling. It starts with `polar_oat_`.

Production and sandbox tokens both work. Virling tries `api.polar.sh` first and `sandbox-api.polar.sh` second. If a scope is missing, connecting fails with a message naming it.

## What is imported

- **Orders** from the last 90 days on the first run, then new orders every minute. Unpaid orders are checked again every 10 minutes for up to 30 days, and imported once paid.
- **Refunds.** New refunds update the order they belong to. A pending refund is read again once it settles.
- **Renewals.** An order with billing reason `subscription_cycle` is marked as a renewal.
- **Subscription status**, once an hour, when the token has `subscriptions:read`. Without it, retention falls back to payment recency.

## How a payment finds its visitor

Virling reads `virling_vid` from, in order: the order's `metadata`, the order's custom field data, then the customer's `metadata`.

With a checkout session, set it in `metadata`:

```js
const checkout = await polar.checkouts.create({
  products: ["PRODUCT_ID"],
  success_url: "https://yourdomain.com/thanks",
  metadata: { virling_vid: req.cookies.virling_vid },
});
// Send the customer to checkout.url
```

## Checkout links

Checkout links on `buy.polar.sh` need no code. Keep the link as a normal `<a href>` on a page that runs the script: the script adds `reference_id=<visitor ID>` to it, unless it is already there, and Polar stores it in the order's metadata, where Virling reads it. A `virling_vid` custom field on the link still works too.

## Check that it works

1. Connect a token from the Polar sandbox.
2. Open a private window and visit a page that runs the script through a tagged link, such as `https://yourdomain.com/?utm_source=test&utm_medium=email`.
3. In the same window, go to a sandbox checkout your server creates with `virling_vid` in `metadata`, and pay with a test card. The script does not add the visitor ID to sandbox links.
4. Wait a minute for the next sync, then open the payments list at `/app/yourdomain.com/payments`. The payment shows the `test` source and the **Email** channel of your tagged visit instead of Unattributed.

Use a new email address for each test: a later payment by the same customer is a renewal and keeps the first payment's channel. If a step fails, see [Test your setup](../guides/test-your-setup.md).

## Common questions

### Does the Polar sandbox work?

Yes. Paste a sandbox token and Virling reads sandbox orders and refunds. A site has one Polar connection, and sandbox orders stay in your reports after you replace the token with a production one.

### Why is a Polar payment Unattributed?

Usually because neither the order nor its customer carries a visitor ID: the checkout was created without `metadata.virling_vid`, or a checkout link was opened from an email or another site. A `reference_id` you set on the link yourself is kept, so the script cannot add the visitor ID there. See [Unattributed revenue](../attribution/unattributed.md).

### How are Polar refunds counted?

A succeeded refund lowers the revenue of the order it belongs to, on that order's date. Refunds made after you connect count even when the order is older than 90 days. A pending refund counts once it succeeds, if it does so within 7 days.

### How are Polar renewals credited?

Orders with billing reason `subscription_cycle` or `subscription_update`, and any later order by the same customer, keep the channel of that customer's first order. If that order is older than the first import, its renewals are usually Unattributed. See [Renewals](../attribution/renewals.md).

### How far back does Polar history go?

The first import reads orders from the last 90 days. Orders from before the script was on your site count as revenue but are Unattributed.

### Can I remove the Polar token?

Yes. **Disconnect** under Payment connections in site settings deletes the stored token and stops syncing, and imported orders stay in your reports. Delete the token in Polar too to revoke access completely.

## Guides

- [Polar revenue attribution](https://virling.com/integrations/polar)
- [How to track Polar revenue by traffic source](https://virling.com/blog/track-polar-revenue-by-traffic-source)
- [Polar UTM tracking](https://virling.com/blog/polar-utm-tracking)
- [Polar for developer tools: payments by traffic source](https://virling.com/integrations/polar-developer-tools)
