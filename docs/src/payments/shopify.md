# Shopify

Virling reads Shopify orders and their refunds with the Admin API token of a custom app that can only read orders.

## Create a token

1. In your Shopify admin, open **Settings**, **Apps and sales channels**, **Develop apps**, and create an app. Shopify's guide: [custom apps](https://help.shopify.com/en/manual/apps/app-types/custom-apps).
2. Under **Admin API integration**, give it only the `read_orders` scope, then install the app.
3. In Virling, paste the **Admin API access token**, which starts with `shpat_`, and your store's `https://your-store.myshopify.com` address. A custom domain does not work here; the Admin API only answers on the myshopify.com address.

## What is imported

- **Orders** from the last 60 days on the first run, the most Shopify gives without the extra `read_all_orders` permission, then new ones every minute. Paid, partially refunded and refunded orders count; voided ones count as failed; pending and authorized orders wait until they are paid.
- **Refunds.** Every round also reads the orders updated since the last one, so refunds show up within a minute or two.
- **Renewals.** An order from a Shopify Subscriptions renewal (source `subscription_contract`) is marked as a renewal and keeps the channel of the first payment. See [Renewals](../attribution/renewals.md).
- **Subscription status** is not read, so [Retention](../attribution/retention.md) uses payment recency for Shopify customers.

## How a payment finds its visitor

Virling reads `virling_vid` from the order's note attributes. Add this to your theme's `theme.liquid`, after the Virling script, so the visitor ID is stored on the cart, which Shopify copies onto the order:

```html
<script>
// Shopify: in your theme's theme.liquid, after the Virling script.
// It stores the visitor ID on the cart, and Shopify copies cart
// attributes onto the order as note attributes.
addEventListener("load", function () {
  var vid = window.virling && virling.vid && virling.vid();
  if (vid) fetch("/cart/update.js", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ attributes: { virling_vid: vid } })
  });
});
</script>
```
