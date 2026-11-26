# Set up a merchant of record checkout

A merchant of record such as Paddle, LemonSqueezy, Polar, Creem or Dodo Payments runs checkout on its own page or in an overlay, so your UTM tags and referrer never reach it. Virling does not need them to: it credits the payment to the visitor's earlier visits, as long as checkout carries the visitor ID from the `virling_vid` cookie. This guide shows how to pass it for each provider. To start, sign in at [app.virling.com](https://app.virling.com/login) for the hosted plan, or [self-host it](../quickstart.md).

Read the visitor ID on your server from the `virling_vid` cookie, or in the browser with `window.virling?.vid?.()`. See [Read the visitor ID](../payments/checkout.md#read-the-visitor-id).

## Paddle

Virling reads `virling_vid` from the transaction's `custom_data`. With Paddle.js, pass it as `customData` when you open checkout:

```js
Paddle.Checkout.open({
  items: [{ priceId: "pri_...", quantity: 1 }],
  customData: { virling_vid: window.virling?.vid?.() },
});
```

For transactions you create on your server, set `custom_data: { virling_vid: req.cookies.virling_vid }`. See [Paddle](../payments/paddle.md).

## LemonSqueezy

LemonSqueezy only returns checkout custom data in webhooks, so Virling registers a webhook on each of your stores when you connect. It needs Virling on a public `https://` address.

Checkout links on `STORE.lemonsqueezy.com` need no code: the script adds `checkout[custom][virling_vid]` to them. For a checkout you build yourself, pass the visitor ID as custom data:

```js
const url = new URL("https://STORE.lemonsqueezy.com/checkout/buy/VARIANT_ID");
const vid = window.virling?.vid?.();
if (vid) url.searchParams.set("checkout[custom][virling_vid]", vid);
location.href = url.href;
```

Or on your server, when you create a checkout with `POST /v1/checkouts`, set `data.attributes.checkout_data.custom` to `{ "virling_vid": "<the visitor's virling_vid cookie>" }`. See [LemonSqueezy](../payments/lemonsqueezy.md).

## Polar

Virling reads `virling_vid` from, in order: the order's `metadata`, the order's custom field data, then the customer's `metadata`. With a checkout session, set it in `metadata`:

```js
const checkout = await polar.checkouts.create({
  products: ["PRODUCT_ID"],
  success_url: "https://yourdomain.com/thanks",
  metadata: { virling_vid: req.cookies.virling_vid },
});
// Send the customer to checkout.url
```

Checkout links on `buy.polar.sh` need no code: the script adds `reference_id=<visitor ID>` to them. See [Polar](../payments/polar.md).

## Creem

Creem returns checkout metadata only in webhooks, so Virling adds one webhook for `checkout.completed` when you connect. It needs Virling to be reachable over https. Set `virling_vid` in `metadata` when you create the checkout on your server:

```js
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

See [Creem](../payments/creem.md).

## Dodo Payments

Virling reads `metadata.virling_vid` on the payment. Static payment links on `checkout.dodopayments.com/buy/...` need no code: the script adds `metadata_virling_vid=<visitor ID>` to them. For a checkout session you create on your server, set it in `metadata`:

```js
const session = await client.checkoutSessions.create({
  product_cart: [{ product_id: "pdt_...", quantity: 1 }],
  return_url: "https://yourdomain.com/thanks",
  metadata: { virling_vid: req.cookies.virling_vid },
});
// Send the customer to session.checkout_url
```

See [Dodo Payments](../payments/dodo.md).

## Common pitfalls

- **Overlay vs hosted checkout.** The script adds the visitor ID only to hosted checkout links that are normal `<a href>` links on a page that runs it. An overlay or checkout opened from your own code is not touched, so pass the ID yourself as shown above. The script loads with `defer`, so if your code can run first, [wait for it](../payments/checkout.md#read-the-visitor-id).
- **Custom domains.** The script only tags links on `buy.polar.sh`, `checkout.dodopayments.com` and `*.lemonsqueezy.com` (plus `buy.stripe.com`). A checkout on any other domain needs the ID passed in code. If checkout or sign-up runs on a second domain you own, add it under **Same-visit domains** in **Site settings**, so a return from it does not start a new session. See [How attribution works](../attribution/index.md#sessions).
- **Redirects stripping parameters.** A redirect from `http` to `https`, from `www.` to the bare domain, or to a clean URL can drop `?utm_source=...` before the script can read it. Keep the query string through every redirect. Read more: [UTM parameters lost after redirect: causes and fixes](https://virling.com/blog/utm-parameters-lost-after-redirect).

Make one test payment through the whole path before you trust the numbers: [Test your setup](test-your-setup.md).

[Start free](https://app.virling.com/login) and pass the visitor ID into your checkout.
