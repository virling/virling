# Test your setup

Before you trust the numbers, run one test visit and one test payment through the whole path: the script loads, the visit shows its source, the payment arrives with the right channel, and checkout keeps the visitor. It takes about ten minutes. To start, sign in at [app.virling.com](https://app.virling.com/login) for the hosted plan, or [self-host it](../quickstart.md).

## Checklist

1. **The script loads.** Open your site and your browser's network tab, then reload. You should see `v.js` load from your Virling server and a request to `/api/event` answered with `202`. The setup checklist on the dashboard turns **Script installed** green with the first event.
2. **A tagged visit shows its source.** Open a private window and visit your site from a tagged link, such as `https://yourdomain.com/?utm_source=test&utm_medium=test`. The visit shows under **Visitors** on the dashboard right away, and the `test` source shows on the Channels card. See [Channels](../attribution/channels.md) for how tags map to a channel.
3. **A test payment shows the right channel.** In the same private window, go from a page that runs the script to checkout and pay in your provider's test mode, as described for each provider below. Wait a minute for the next sync, then open the payments list at `/app/yourdomain.com/payments`. The payment should show the `test` source instead of Unattributed. Open it to see its visitor ID and journey.
4. **Checkout keeps the visitor.** The payment's journey should start at your tagged visit, not at the checkout page. Subdomains such as `app.yourdomain.com` share the `virling_vid` cookie, so `window.virling.vid()` in the browser console returns the same ID on each. If checkout starts on a second domain you own, pass the visitor ID along in the link and on to checkout, and add that domain under **Same-visit domains** in **Site settings**. See [Link checkout to visits](../payments/checkout.md) and [How attribution works](../attribution/index.md#sessions).

Test with a browser you have not opted out with `#virling-ignore`, from an address that is not under Ignored IP addresses, and not on `localhost` unless the tag has `data-dev`.

A provider is connected with one key at a time. Payments imported with a test key stay in your reports after you replace it with a live key, so if you want only real payments in your numbers, make one small live payment instead and refund it.

## Per provider

### Stripe

Connect a restricted key that starts with `rk_test_`, then pay with one of Stripe's test cards through your Checkout Session or a test Payment Link on `buy.stripe.com`. Payment Links on your pages get the visitor ID with no code. Replace the key with an `rk_live_` one when you are done. See [Stripe](../payments/stripe.md).

### Paddle

Connect a sandbox key that starts with `pdl_sdbx_`, set Paddle.js to the sandbox environment, and pass `virling_vid` as `customData` when you open checkout. Pay with one of Paddle's test cards. See [Paddle](../payments/paddle.md).

### Shopify

Shopify has no separate test store API, so test on your connected store with Shopify Payments in test mode or the Bogus Gateway. Test orders are imported like any other paid order. Check that the order's note attributes include `virling_vid`, which the theme snippet sets. Cancel and refund the order afterwards. See [Shopify](../payments/shopify.md).

### WooCommerce

Put your payment gateway in its test or sandbox mode, or use a manual method such as Cash on delivery and set the order to Processing, which counts as paid. Check that the order's custom fields include `virling_vid`, which the `functions.php` snippet sets. See [WooCommerce](../payments/woocommerce.md).

### Polar

Connect an organization token from the Polar sandbox, then create a sandbox checkout with `virling_vid` in its `metadata` from your server. The script adds the visitor ID by itself only to links on `buy.polar.sh`, not to sandbox links. See [Polar](../payments/polar.md).

### LemonSqueezy

Switch your store to test mode and create the API key there, so Virling reads test orders and registers its webhook in test mode. Checkout links on `*.lemonsqueezy.com` get the visitor ID with no code. The webhook needs Virling on a public `https://` address. See [LemonSqueezy](../payments/lemonsqueezy.md).

### Dodo Payments

Connect a test mode key, then create a test checkout with `metadata.virling_vid` from your server, or add `metadata_virling_vid` to the link in your own code. The script adds it by itself only to links on `checkout.dodopayments.com`, not to test links. See [Dodo Payments](../payments/dodo.md).

### Creem

Connect a key that starts with `creem_test_`, then create a test checkout with `virling_vid` in its `metadata`. Virling adds its webhook in test mode and needs to be reachable over https. See [Creem](../payments/creem.md).

### RevenueCat

Set the `virling_vid` customer attribute for your test user, then make a sandbox purchase. RevenueCat sends sandbox events to the same webhook, and Virling imports them like live ones. The webhook needs Virling to be reachable over https. See [RevenueCat](../payments/revenuecat.md).

## If it does not show up

- No request to `/api/event`, or not answered with `202`: [No visitors show up](../troubleshooting.md#no-visitors-show-up).
- The payment is missing from the payments list: [Payments do not appear at all](../troubleshooting.md#payments-do-not-appear-at-all).
- The payment shows but lists no visitor ID: [Checkout is not linked](../troubleshooting.md#checkout-is-not-linked).
- The payment has a visitor ID but shows as Unattributed: [Payments show as Unattributed](../troubleshooting.md#payments-show-as-unattributed).
