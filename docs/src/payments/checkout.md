# Link checkout to visits

A payment is credited to a visit only when checkout carries the visitor's ID. Pass it from the `virling_vid` cookie into your checkout, and Virling matches the payment within a minute of the next sync.

Payments without it still count toward revenue, as [Unattributed](../attribution/unattributed.md).

## Read the visitor ID

- **On your server**, read the `virling_vid` cookie from the request. It is a first-party cookie on your domain, so your backend receives it with every request from the browser.
- **In the browser**, `window.virling?.vid?.()` returns the same ID. It returns nothing when the script is blocked or not loaded yet, and `null` in cookieless mode, so check the value before using it.

The script loads with `defer`. If your checkout code can run before it, wait for it:

```js
async function visitorId() {
  for (let i = 0; i < 20 && !window.virling?.vid; i++) {
    await new Promise((r) => setTimeout(r, 100));
  }
  return window.virling?.vid?.() || null;
}
```

## Per provider

<div class="tabset" data-tabs="Stripe|Dodo Payments|Polar|LemonSqueezy|Creem|RevenueCat|WooCommerce" data-key="provider" aria-label="Payment provider"></div>

```js
// Stripe Checkout (Node, stripe): pass the visitor ID on the session
const session = await stripe.checkout.sessions.create({
  client_reference_id: req.cookies.virling_vid,
  line_items: [{ price: "price_...", quantity: 1 }],
  mode: "subscription",
  success_url: "https://yourdomain.com/thanks",
});

// Your own PaymentIntent (Elements): put it in metadata
const intent = await stripe.paymentIntents.create({
  amount: 4900,
  currency: "usd",
  metadata: { virling_vid: req.cookies.virling_vid },
});

// Payment Links (buy.stripe.com) need nothing:
// the Virling script adds client_reference_id on click.
```

```js
// Dodo Payments (Node, dodopayments): checkout session with metadata
const session = await client.checkoutSessions.create({
  product_cart: [{ product_id: "pdt_...", quantity: 1 }],
  return_url: "https://yourdomain.com/thanks",
  metadata: { virling_vid: req.cookies.virling_vid },
});
// Send the customer to session.checkout_url

// Static payment links (checkout.dodopayments.com/buy/...) need nothing:
// the Virling script adds metadata_virling_vid on click.
```

```js
// Polar (Node, @polar-sh/sdk): checkout session with metadata
const checkout = await polar.checkouts.create({
  products: ["PRODUCT_ID"],
  success_url: "https://yourdomain.com/thanks",
  metadata: { virling_vid: req.cookies.virling_vid },
});
// Send the customer to checkout.url

// Checkout links (buy.polar.sh) need nothing:
// the Virling script adds reference_id.
```

```js
// LemonSqueezy: checkout links (STORE.lemonsqueezy.com/checkout/buy/...)
// need nothing: the Virling script adds checkout[custom][virling_vid].

// For checkouts you create on your server, POST /v1/checkouts with:
// data.attributes.checkout_data.custom = { virling_vid: req.cookies.virling_vid }
```

```js
// Creem (Node): checkout with metadata
const res = await fetch("https://api.creem.io/v1/checkouts", {
  method: "POST",
  headers: { "x-api-key": process.env.CREEM_API_KEY, "content-type": "application/json" },
  body: JSON.stringify({
    product_id: "prod_...",
    success_url: "https://yourdomain.com/thanks",
    metadata: { virling_vid: req.cookies.virling_vid },
  }),
});
// Send the customer to (await res.json()).checkout_url
```

```js
// RevenueCat (Node): set the customer attribute virling_vid before the purchase
await fetch(`https://api.revenuecat.com/v2/projects/${projectId}/customers/${appUserId}/attributes`, {
  method: "POST",
  headers: { authorization: `Bearer ${process.env.REVENUECAT_SECRET_KEY}`, "content-type": "application/json" },
  body: JSON.stringify({ attributes: [{ name: "virling_vid", value: req.cookies.virling_vid }] }),
});
```

```php
// WooCommerce: add to your theme's functions.php (or a snippets plugin)
// so every order keeps the visitor ID from the virling_vid cookie.
add_action('woocommerce_checkout_create_order', 'virling_vid_on_order');
add_action('woocommerce_store_api_checkout_update_order_from_request', 'virling_vid_on_order');
function virling_vid_on_order($order) {
  $vid = $_COOKIE['virling_vid'] ?? '';
  if (preg_match('/^[A-Za-z0-9-]{8,64}$/', $vid)) $order->update_meta_data('virling_vid', $vid);
}
```

## Stripe

Virling reads `metadata.virling_vid` on the charge or its PaymentIntent, then `metadata.virling_vid` or `client_reference_id` on the Checkout Session. See [Stripe](stripe.md).

## Dodo Payments

Virling reads `metadata.virling_vid` on the payment. For checkout sessions you create on your server, set it in `metadata`. Static payment links need nothing: the script adds `metadata_virling_vid` on click. See [Dodo Payments](dodo.md).

## Polar

Virling reads `virling_vid` from the order's `metadata`, its custom field data, or the customer's `metadata`. See [Polar](polar.md).

## LemonSqueezy

Virling reads `virling_vid` from the checkout custom data in its webhook. Checkout links need no code; for checkouts you create, set `checkout_data.custom.virling_vid`. See [LemonSqueezy](lemonsqueezy.md).

## Creem

Virling reads `virling_vid` from the checkout's `metadata` in the `checkout.completed` webhook, then from the subscription's or customer's `metadata`. See [Creem](creem.md).

## RevenueCat

Virling reads the customer attribute `virling_vid` from each webhook event. Set it from your app with the SDK's `setAttributes`, or from your server. See [RevenueCat](revenuecat.md).

## WooCommerce

Virling reads `virling_vid` from the order's meta data. Add the snippet above to your theme once, and every order keeps the visitor ID from the cookie. See [WooCommerce](woocommerce.md).

## Checkout links need no code

Hosted checkout links carry the visitor ID by themselves. The script adds it to every such link on the page when the page loads, and again when a visitor clicks one, so links added later by your own code are covered too:

| Link host | Parameter added | Virling reads it from |
| --- | --- | --- |
| `buy.stripe.com` | `client_reference_id=<visitor ID>` | The Checkout Session's `client_reference_id` |
| `buy.polar.sh` | `reference_id=<visitor ID>` | The order's `metadata.reference_id` |
| `checkout.dodopayments.com` | `metadata_virling_vid=<visitor ID>` | The payment's `metadata.virling_vid` |
| `*.lemonsqueezy.com` | `checkout[custom][virling_vid]=<visitor ID>` | The webhook's custom data |

A parameter that is already on the link is never replaced, and nothing is added without a visitor ID, so not in cookieless mode. Links must be normal `<a href>` links on a page that runs the script. Links to other providers are left alone.

## Check that it works

Make a test payment from a page that runs the script, wait for the next sync, and open the payments list at `/app/yourdomain.com/payments`. The payment should show a channel instead of Unattributed. Open it to see its journey.
