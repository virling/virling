# Currencies

Payments keep their original amount and currency. For reports, every amount is also converted to USD.

## Exchange rates

Rates come from the European Central Bank through [frankfurter.dev](https://frankfurter.dev), with USD as the base. Virling fetches them when it first needs them and refreshes them every 12 hours. A payment is converted with the rates current when it is imported, not the rates on the day it was made.

If a currency is not in the ECB list, the payment is imported without a USD amount and is left out of USD totals.

## Minor units

Providers report amounts in minor units. These currencies have no minor unit and are read as whole units: BIF, CLP, DJF, GNF, JPY, KMF, KRW, MGA, PYG, RWF, UGX, VND, VUV, XAF, XOF and XPF. Every other currency is read as two decimals, so `2900` USD is 29.00.

## Refunds

Revenue is shown after refunds. The refunded amount is subtracted in the payment's currency and converted the same way.

A reporting currency other than USD is <span class="na">Not available yet</span>.
