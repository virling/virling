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

<div class="tabset" data-tabs="Stripe|Dodo Payments|Polar|LemonSqueezy|Paddle" data-key="provider" aria-label="Payment provider"></div>

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

// Checkout links: add a custom field with the slug virling_vid
// to the link. The Virling script fills it on click
// (custom_field_data.virling_vid), so nothing else is needed.
```

```js
// LemonSqueezy: add the visitor ID as checkout custom data.
// Not attributed yet: LemonSqueezy only returns custom data in
// webhooks, which Virling does not receive yet.
// Tag checkouts now and they will match once that ships.

// In the browser, on a checkout URL:
const url = new URL("https://STORE.lemonsqueezy.com/checkout/buy/VARIANT_ID");
const vid = window.virling?.vid?.();
if (vid) url.searchParams.set("checkout[custom][virling_vid]", vid);
location.href = url.href;

// Or on your server, POST /v1/checkouts with:
// data.attributes.checkout_data.custom = { virling_vid: req.cookies.virling_vid }
```

```js
// Paddle (Paddle.js): pass the visitor ID as custom data
Paddle.Checkout.open({
  items: [{ priceId: "pri_...", quantity: 1 }],
  customData: { virling_vid: window.virling?.vid?.() },
});

// Or on your server, POST /transactions with:
// custom_data: { virling_vid: req.cookies.virling_vid }
```

## Stripe

Virling reads `metadata.virling_vid` on the charge or its PaymentIntent, then `metadata.virling_vid` or `client_reference_id` on the Checkout Session. See [Stripe](stripe.md).

## Dodo Payments

Virling reads `metadata.virling_vid` on the payment. For checkout sessions you create on your server, set it in `metadata`. Static payment links need nothing: the script adds `metadata_virling_vid` on click. See [Dodo Payments](dodo.md).

## Polar

Virling reads `virling_vid` from the order's `metadata`, its custom field data, or the customer's `metadata`. See [Polar](polar.md).

## LemonSqueezy

Tag checkouts with `checkout[custom][virling_vid]` now. Matching them is <span class="na">Not available yet</span> and waits on webhook support. See [LemonSqueezy](lemonsqueezy.md).

## Paddle

Virling reads `virling_vid` from the transaction's `custom_data`. With Paddle.js, pass it as `customData`. See [Paddle](paddle.md).

## Payment links

The script tags three kinds of hosted links when a visitor clicks them, so no code is needed:

| Link host | Parameter added |
| --- | --- |
| `buy.stripe.com` | `client_reference_id=<visitor ID>` |
| `buy.polar.sh` | `custom_field_data.virling_vid=<visitor ID>`, which needs a `virling_vid` custom field on the link |
| `checkout.dodopayments.com` | `metadata_virling_vid=<visitor ID>` |

The parameter is only added when the link does not already have it, and only when there is a visitor ID, so not in cookieless mode. Links to other providers are left alone.

## Check that it works

Make a test payment from a page that runs the script, wait for the next sync, and open the payments list at `/app/yourdomain.com/payments`. The payment should show a channel instead of Unattributed. Open it to see its journey.
