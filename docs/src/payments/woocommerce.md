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
