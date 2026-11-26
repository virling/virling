# Currencies

Payments keep their original amount and currency. For reports, every amount is also converted to USD. A reporting currency other than USD is <span class="na">Not available yet</span>.

## Exchange rates

Rates come from the European Central Bank through [frankfurter.dev](https://frankfurter.dev), with USD as the base. Virling fetches them when it first needs them and refreshes them every 12 hours. A payment is converted with the rates current when it is imported, not the rates on the day it was made. When Virling reads a payment again, for example after a refund, it converts it again with the rates at that time.

If a currency is not in the ECB list, or the rates cannot be fetched, the payment is imported without a USD amount and is left out of USD totals.

## Minor units

Providers report amounts in minor units. These currencies have no minor unit and are read as whole units: BIF, CLP, DJF, GNF, JPY, KMF, KRW, MGA, PYG, RWF, UGX, VND, VUV, XAF, XOF and XPF. Every other currency is read as two decimals, so `2900` USD is 29.00.

## Refunds

Revenue is shown after refunds. The refunded amount is subtracted in the payment's currency and converted at the same rate as the payment, so a partial refund lowers USD revenue by the same share. It counts on the payment's date, not the refund's.

## Worked example

Made-up amounts and rates: a EUR 200 payment, then a EUR 50 refund, with 1 USD worth 0.80 EUR.

| Step | Calculation | Revenue |
| --- | --- | --- |
| Payment imported | 200 / 0.80 | $250.00 |
| EUR 50 refunded, rate still 0.80 | 250.00 x 150 / 200 | $187.50 |
| EUR 50 refunded, rate now 0.85 | 200 / 0.85 = 235.29, then 235.29 x 150 / 200 | $176.47 |

Virling reads the payment again to see the refund, so the last row uses the new rate for the whole payment. Amounts are rounded to the cent.

## Common questions

### Why does my total differ from my provider's?

- **Rates.** Your provider converts with its own rates, Virling with ECB rates at the time it reads each payment. RevenueCat one-time purchases from the first import come in USD from RevenueCat.
- **Refunds.** They lower revenue on the original payment's date, and lost disputes count as refunds on some providers. On LemonSqueezy, Creem, Paddle and WooCommerce, a refund of a payment older than 60 days is not picked up automatically.
- **Fees.** Provider fees are not subtracted from revenue.
- **Dates.** Report days start in your site's timezone.
- **History.** The first import goes back 90 days, or 60 on Shopify, and RevenueCat subscription payments from before you connected are not imported.
- **Test payments.** They stay in your reports after you replace a test key with a live one.
- **A currency that is not listed.** See the next question.

### What happens to a currency that is not listed?

The payment is imported and keeps its amount and currency. It counts as a payment and its customer as a customer, but it adds nothing to revenue in USD. [The rates Virling reads](https://api.frankfurter.dev/v1/latest?base=USD) show which currencies are covered.

### Can I report in another currency?

Not yet. Reports are in USD. Each payment keeps its original currency, which the [API](../reference/api.md) returns next to the USD amount.
