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

## Headless and Hydrogen stores

A headless store, built with Hydrogen or another custom storefront on the Storefront API, is matched the same way: Virling reads the order through the Admin API and looks for `virling_vid` in its note attributes. Cart attributes set through the Storefront API are copied onto the order as note attributes, just like theme carts. What does not work on a headless store is the theme snippet above: there is no `theme.liquid`, and `/cart/update.js` only exists on the Online Store, so you set the attribute from your own cart code instead.

A Shopify order is matched to a visit in this order:

1. **Visitor ID.** `virling_vid` in the order's note attributes. Without it, the order has no visitor.
2. **Customer email.** For an order with no visitor ID, the order's email is compared with emails your site sent as `identify` events, kept only as salted hashes. An exact match credits that visitor, and the payment shows **Matched by email**. It only applies to orders synced after the identify. See [Recover payments by email](../attribution/unattributed.md#recover-payments-by-email).

Anything else, such as UTM tags Shopify stores on the order or the landing page, is not read. Orders that match neither way still count toward revenue, as [Unattributed](../attribution/unattributed.md).

### 1. Add the script to the Hydrogen app

Put the tag in the `<head>` of `app/root.tsx`, with the nonce Hydrogen uses for its content security policy:

```tsx
// app/root.tsx, inside <head>; nonce comes from useNonce() in @shopify/hydrogen.
<script defer src="https://app.virling.com/v.js"
  data-site="yourstore.com" nonce={nonce}></script>
```

Then allow your Virling host in `app/entry.server.tsx`, or the browser blocks the script and its events:

```ts
const { nonce, header, NonceProvider } = createContentSecurityPolicy({
  shop: { checkoutDomain: context.env.PUBLIC_CHECKOUT_DOMAIN, storeDomain: context.env.PUBLIC_STORE_DOMAIN },
  scriptSrc: ["'self'", "https://app.virling.com"],
  connectSrc: ["'self'", "https://app.virling.com"],
});
```

Self-hosting, use your own Virling address in all three places. Hydrogen navigates with the History API, so route changes are tracked with no extra code. See [Single-page apps](../track/spa.md).

### 2. Put the visitor ID on the cart

The script stores the visitor ID in the `virling_vid` cookie on your storefront's domain, so the Hydrogen server receives it with every cart request. In the cart route's action, `app/routes/cart.tsx`, set it as a cart attribute after lines are added:

```ts
case CartForm.ACTIONS.LinesAdd: {
  result = await cart.addLines(inputs.lines);
  // Store the Virling visitor ID on the cart; Shopify copies it onto the order.
  const vid = request.headers.get("Cookie")?.match(/(?:^|;\s*)virling_vid=([^;]+)/)?.[1];
  const cartId = result.cart?.id;
  if (vid && cartId) {
    result = await cart.updateAttributes([{ key: "virling_vid", value: vid }], { cartId });
  }
  break;
}
```

If you set other cart attributes, pass them in the same call. On a custom storefront without Hydrogen, do the same with the Storefront API: pass `attributes: [{ key: "virling_vid", value: vid }]` to `cartCreate`, or call `cartAttributesUpdate` on an existing cart, with the ID from the cookie or from `window.virling?.vid?.()` in the browser.

Keep the storefront on your own domain, where the script runs. Checkout can stay on Shopify's checkout domain, since the visitor ID travels on the cart, not in the browser.

### What does not carry the visitor ID

- **Carts made outside your storefront**, such as cart permalinks, Buy Button embeds, or orders from other sales channels, have no `virling_vid`. They can still match by email.
- **Cookieless mode.** With `data-cookieless` there is no cookie and no visitor ID to store. Leave it off for a store.
- **A cart created before the script loaded**, on a visitor's first page, gets the attribute on the next line added.

### Check it works

Run the test in [Test your setup](../guides/test-your-setup.md#shopify) on the storefront: open it from a tagged link in a private window, add a product, and check that the cart has a `virling_vid` attribute, for example in the Storefront API's `cart { attributes { key value } }`. Place a test order, check that the order's note attributes include `virling_vid`, and wait a minute for the next sync. For local development, add `data-dev` to the tag. For the rest of a store setup, see [Set up an online store](../guides/ecommerce.md).

## Guides

- [Shopify revenue attribution](https://virling.com/integrations/shopify)
- [How to track Shopify revenue by traffic source](https://virling.com/blog/track-shopify-revenue-by-traffic-source)
- [Shopify UTM tracking](https://virling.com/blog/shopify-utm-tracking)
- [Shopify subscriptions: subscribers by channel](https://virling.com/integrations/shopify-subscriptions)
