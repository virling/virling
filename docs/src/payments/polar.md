# Polar

Virling reads Polar orders and refunds with an organization access token.

## Create a token

1. In Polar, open your organization's settings and create an **Organization Access Token**. Polar's guide: [organization access tokens](https://docs.polar.sh/integrate/oat).
2. Give it only the `orders:read` and `refunds:read` scopes.
3. Paste it into Virling. It starts with `polar_oat_`.

Production and sandbox tokens both work. Virling tries `api.polar.sh` first and `sandbox-api.polar.sh` second. If a scope is missing, connecting fails with a message naming it.

## What is imported

- **Orders** from the last 90 days on the first run, then new orders every minute. Unpaid orders are checked again every 10 minutes for up to 30 days, and imported once paid.
- **Refunds.** New refunds update the order they belong to. A pending refund is read again once it settles.
- **Renewals.** An order with billing reason `subscription_cycle` is marked as a renewal.

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

Checkout links on `buy.polar.sh` need no code beyond one setting:

1. In Polar, add a custom field with the slug `virling_vid` to the checkout link.
2. Keep the link as a normal `<a href>` on a page that runs the script.

When a visitor clicks it, the script adds `custom_field_data.virling_vid=<visitor ID>` to the link, unless it is already there, and Polar prefills the field.
