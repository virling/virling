# Set up Virling for an online store

This guide sets up Virling for a Shopify or WooCommerce store, so every order shows the channel that brought the buyer: add the script, connect the store with a read-only key, store the visitor ID on each order, tag your campaigns, and read orders and revenue by channel. To start, sign in at [app.virling.com](https://app.virling.com/login) for the hosted plan, or [self-host it](../quickstart.md).

## What you need

- Access to your store's theme (Shopify) or to WordPress (WooCommerce), where you can add one tag to the `<head>` and a short snippet.
- Permission to create a read-only key in your store admin.

## 1. Add the script

1. Click **+ Add a site** in the sidebar, or open `/app/setup`, and enter your store's domain, such as `yourstore.com`, and its timezone.
2. Open `/app/yourstore.com/setup/script` for the exact tag, and paste it before `</head>` on every page:

```html
<script defer src="https://app.virling.com/v.js"
  data-site="yourstore.com"></script>
```

On Shopify, paste it into your theme's `theme.liquid`. On WooCommerce, use the Virling WordPress plugin, which adds the tag for you and survives theme updates. See [Add the script](../track/script.md) and the [framework guides](../track/frameworks.md).

Leave [cookieless mode](../track/cookieless.md) off for a store: without the `virling_vid` cookie there is no visitor ID for the order to carry.

## 2. Connect Shopify or WooCommerce

Open `/app/yourstore.com/setup/payments`, pick your store, paste the key and click **Connect and backfill**.

- **Shopify:** a custom app with only the `read_orders` scope. Paste its Admin API access token, which starts with `shpat_`, and your `https://your-store.myshopify.com` address. Orders from the last 60 days are imported first. See [Shopify](../payments/shopify.md).
- **WooCommerce:** a REST API key with **Read** permission, pasted as `ck_...:cs_...`, and your store's https address. Orders from the last 90 days are imported first. See [WooCommerce](../payments/woocommerce.md).

Virling then checks for new orders every minute, counts refunds, and never creates, changes or refunds anything.

## 3. Store the visitor ID on each order

This is the step that links an order to the visits before it. Add the snippet for your store once:

- **Shopify:** a short script in `theme.liquid`, after the Virling script, stores the visitor ID as a cart attribute, which Shopify copies onto the order.
- **WooCommerce:** a few lines in your theme's `functions.php`, or a snippets plugin, save the visitor ID in the order's meta data, for the classic and the block checkout.

The code for both is in [Link checkout to visits](../payments/checkout.md#per-provider). Orders without it still count toward revenue, as [Unattributed](../attribution/unattributed.md).

## 4. Tag your campaigns

Emails, SMS, ads, bio links and creator posts often arrive with no referrer. Add `utm_source`, `utm_medium` and `utm_campaign` to every link you control, so each campaign shows up by name. The [UTM builder](https://virling.com/tools/utm-builder) makes the links, and [tracked links](../track/links.md) give you short links for places where long URLs do not fit. See [Channels](../attribution/channels.md) for how each visit is classified.

## 5. Read orders and revenue by channel

- **Channels** card: visitors, revenue and revenue per visitor for each channel, with the share refunded. Use the Source and Campaign tabs to see the sites, posts and UTM campaigns behind a channel.
- **Matched revenue**: the line under the five numbers gives the share of revenue matched to a visit, and why the rest was not.
- **Payments**: every order with the channel it was credited to. Open one to see the visits before it. Filter by unattributed to find orders with no visitor ID.
- **Recent customers**: the last 10 buyers with their first source and how many visits they made before paying.
- **Retention**: which channels bring customers who order again.

By default an order is credited to the latest visit in the last 90 days that was not Direct. See [Read your dashboard](dashboard.md) and [Model and lookback window](../attribution/model.md).

## Common store problems

### Orders show as Direct or Unknown

Store reports file an order as Direct or Unknown when nothing about the visit says where the buyer came from. Usually that is an untagged email or SMS link, an in-app browser that sends no referrer, or a source cookie that did not last until checkout. In Virling, an order with no visitor ID is Unattributed, and an order whose visits all lack a source is Direct.

- Check that the step 3 snippet is in place, and that new orders carry `virling_vid`.
- Tag every campaign link, as in step 4.
- On the Channels card, Direct is split into typed or bookmarked, a shared link, and a return from checkout.

Read more: [Why Shopify orders show up as Direct](https://virling.com/blog/shopify-orders-show-as-direct) and [Why WooCommerce order attribution says Unknown](https://virling.com/blog/woocommerce-order-attribution-unknown).

### Checkout on another domain

The `virling_vid` cookie is set on your registrable domain, so subdomains share it: a blog on `yourstore.com` and a store on `shop.yourstore.com` keep the same visitor ID. Two separate domains, such as `yourbrand.com` and `yourbrand-shop.com`, do not, so a visit to one is not linked to an order on the other. Keep your landing pages and store on the same domain or its subdomains where you can.

A return from Stripe, Dodo Payments, Polar, LemonSqueezy and Paddle checkouts never starts a new session. Add any other payment or checkout domain you send buyers to under **Same-visit domains** in **Site settings**, so a return from it continues the visit. See [How attribution works](../attribution/index.md).

Read more: [Cross-domain checkout: keep the source of every sale](https://virling.com/blog/cross-domain-checkout-attribution).

### Discount codes

Virling credits an order by the visit before it, not by a discount code, and does not read codes, so code redemptions stay in your store's reports. A code counts only buyers who type it, and a code that leaks to a coupon site credits sales its owner did not drive. Give each creator or campaign a tagged link as well as a code, and compare the two.

Read more: [How to track influencer sales](https://virling.com/blog/influencer-revenue-tracking), which compares discount codes and links.

## Check it works

1. Open your store in a private window from a tagged link, such as `https://yourstore.com/?utm_source=test&utm_medium=test`.
2. Place an order the way a customer would, from a page that runs the script.
3. Open the order in your store admin and check that it has a `virling_vid` value. On Shopify it is a note attribute; on WooCommerce it is order meta data.
4. Wait a minute for the next sync, then open `/app/yourstore.com/payments`. The order should show the `test` source instead of Unattributed. Open it to see its journey.

If it shows Unattributed, see [Unattributed revenue](../attribution/unattributed.md). Refund the test order in your store afterwards. The refund shows up in Virling within a minute or two on Shopify, and within 6 hours on WooCommerce.
