# Outbound connections

Every host a Virling server connects to. Your visitors' browsers only connect to your Virling server.

| Host | When | Why |
| --- | --- | --- |
| `api.stripe.com` | With Stripe connected, every minute | Read charges, events and Checkout Sessions, and subscriptions hourly |
| `live.dodopayments.com`, `test.dodopayments.com` | With Dodo Payments connected, every minute | Read payments, refunds and subscriptions. Both are tried when connecting to detect the key's mode. |
| `api.polar.sh`, `sandbox-api.polar.sh` | With Polar connected, every minute | Read orders and refunds, and subscriptions hourly when the token allows. Both are tried when connecting. |
| `api.creem.io`, `test-api.creem.io` | With Creem connected, every minute | Read transactions, and subscriptions hourly when the key allows. Both are tried when connecting, and the webhook is added once. |
| `api.lemonsqueezy.com` | With LemonSqueezy connected, every minute | Read orders and subscription invoices, and subscriptions hourly |
| `api.revenuecat.com` | With RevenueCat connected | Check the key and add the webhook when connecting, then read one-time purchases from the last 90 days once |
| `api.paddle.com`, `sandbox-api.paddle.com` | With Paddle connected, every minute | Read transactions with their refunds, and subscriptions hourly when the key allows. Both are tried when connecting. |
| Your WooCommerce store | With WooCommerce connected, every minute | Read orders with their refunds through the REST API, and subscriptions hourly when WooCommerce Subscriptions is installed |
| `<your-store>.myshopify.com` | With Shopify connected, every minute | Read orders and their refunds through the Admin API |
| `api.openai.com`, `api.anthropic.com`, `api.perplexity.ai`, `generativelanguage.googleapis.com` | With [AI answers](../track/ai-answers.md) keys set, on each site's schedule (weekly by default) and on **Run now** | Ask the site's buyer questions with web search |
| `api.frankfurter.dev` | When converting a non-USD payment, at most every 12 hours | ECB exchange rates. See [Currencies](../attribution/currencies.md). |
| Your SMTP server | When someone requests a sign-in link | Send the email, if `VIRLING_SMTP_URL` is set |
| Your sites and their referrers, `https://<host>/` | When the app shows a site, or a referrer without a bundled logo. Site icons at most daily, referrer icons at most every 30 days. | Fetch the homepage's favicon, and for your sites its title to suggest AI answer questions, from public addresses only. Nothing about visitors is sent, and pages, including shared reports, load the cached copy from Virling. |
| `extensions.duckdb.org` | First start | DuckDB downloads its ICU extension into `~/.duckdb/extensions`. Later starts use the cached copy. |

Payment sync and AI answers only run with `--app`. A server started without it only makes the first-start DuckDB download.

Requests use the user agent `virling/<version>` and a 30-second timeout. There is no telemetry, no update check and no license check.

## Firewalls

To run with egress filtering, allow HTTPS to the hosts of the payment and AI providers you connect, `api.frankfurter.dev`, your SMTP host and port, and `extensions.duckdb.org` for the first start. Blocked favicon fetches only mean letters instead of icons.
