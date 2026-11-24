# WooCommerce

Virling reads WooCommerce orders, their refunds and, with WooCommerce Subscriptions, subscriptions from your store's REST API with a read-only key.

## Create a key

1. In WordPress, open **WooCommerce**, **Settings**, **Advanced**, **REST API**, and add a key. WooCommerce's guide: [REST API](https://woocommerce.com/document/woocommerce-rest-api/).
2. Set **Permissions** to **Read**.
3. In Virling, paste the consumer key and the consumer secret joined by a colon, as `ck_...:cs_...`, and enter your store's address, such as `https://shop.example.com`.

The store must be reachable over https, with pretty permalinks on so `/wp-json/wc/v3/` answers.

## What is imported

- **Orders** from the last 90 days on the first run, then new ones every minute, newest first. Processing and completed orders count as paid, failed and cancelled ones as failed. Pending and on-hold orders wait until they are paid.
- **Refunds.** Each order's refunds lower its revenue. Orders from the last 60 days are read again every 6 hours, so later refunds show up.
- **Renewals.** With WooCommerce Subscriptions, a renewal order is marked as a renewal and keeps the channel of the first payment. See [Renewals](../attribution/renewals.md).
- **Subscription status**, once an hour, when WooCommerce Subscriptions is installed. Without it, [Retention](../attribution/retention.md) uses payment recency.

Guest orders have no customer ID, so each counts as its own customer.

## How a payment finds its visitor

Virling reads `virling_vid` from the order's meta data. Add this to your theme's `functions.php`, or to a snippets plugin, so every new order stores the visitor ID from the `virling_vid` cookie. It covers the classic and the block checkout:

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

## Check that it works

1. Add the `functions.php` snippet, and put your payment gateway in its test or sandbox mode, or turn on Cash on delivery.
2. Open a private window and visit your store through a tagged link, such as `https://shop.example.com/?utm_source=test&utm_medium=email`.
3. In the same window, place a test order as a customer would.
4. In WordPress, open the order and check that its custom fields include `virling_vid` and that its status is Processing or Completed.
5. Wait a minute for the next sync, then open the payments list at `/app/yourdomain.com/payments`. The payment shows the `test` source and the **Email** channel of your tagged visit instead of Unattributed. An order paid more than about 10 minutes after it was created can take up to 6 hours to show.

Use a new account for each test, or check out as a guest: a later order by the same account is a renewal and keeps the first order's channel. If a step fails, see [Test your setup](../guides/test-your-setup.md#woocommerce).

## Common questions

### Do WooCommerce guest orders link to a visit?

Yes. The snippet saves the visitor ID on the order itself, so no account is needed. Each guest order counts as its own customer, so a guest's next order is credited to its own visit.

### Does block checkout keep the visitor ID?

Yes. The snippet hooks both the classic checkout and the block checkout. A gateway or express button that creates orders outside those two hooks is not covered, so place a test order with each one and check for `virling_vid`.

### How are WooCommerce refunds counted?

A full or partial refund lowers the revenue of its order, on that order's date, and an order set to Cancelled or Failed no longer counts. Orders created in the last 60 days are read again every 6 hours, so a change can take that long to show, and one on an older order is not picked up automatically.

### How are WooCommerce renewals and free trials credited?

Renewal orders from WooCommerce Subscriptions, and any later order by the same account, keep the channel of that account's first paid order. A $0 order, such as a free trial, is not imported, so the first paid renewal after it is credited only through its own visitor ID or an email match. See [Renewals](../attribution/renewals.md).

### How far back does WooCommerce history go?

The first import reads orders created in the last 90 days. Orders from before the snippet count as revenue but are Unattributed, unless their email matches.

### Why is a WooCommerce order Unattributed?

Usually because it has no `virling_vid`: the snippet was missing, the visitor had no cookie, or the order was created in the admin or by another system. Virling then compares the billing email with `identify` events, and an order that matches neither stays Unattributed. See [Unattributed revenue](../attribution/unattributed.md).

## Guides

- [WooCommerce revenue attribution](https://virling.com/integrations/woocommerce)
- [How to track WooCommerce revenue by traffic source](https://virling.com/blog/track-woocommerce-revenue-by-traffic-source)
- [WooCommerce UTM tracking](https://virling.com/blog/woocommerce-utm-tracking)
- [WooCommerce Subscriptions: subscribers by channel](https://virling.com/integrations/woocommerce-subscriptions)
