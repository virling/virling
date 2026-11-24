# Model and lookback window

Each site has one attribution model and one lookback window. Change both in the app under site settings, at `/app/yourdomain.com/settings`.

## Models

| Model | Setting value | Credits |
| --- | --- | --- |
| Last non-direct | `last_non_direct`, the default | The latest session in the window that was not Direct. If every session was Direct, the latest Direct session. |
| First touch | `first_touch` | The earliest session in the window. |
| Last touch | `last_touch` | The latest session in the window, Direct included. |

Last non-direct suits most products: a visitor who found you on Hacker News and came back a week later by typing your URL is credited to Hacker News, not Direct.

First touch shows which channels introduce people who later pay, and fits longer buying cycles.

Last touch credits the visit right before the payment, even when it was Direct. Use it to see how people arrive when they are ready to buy.

## Lookback window

Only sessions that started inside the window before the payment are considered. Choose 30, 60, 90 or 365 days. The default is 90.

A shorter window credits only recent visits. Sessions older than the window are ignored, and if none are left the payment is Unattributed.

## Which model should I choose?

These are starting points. If you are unsure, keep the defaults.

- **SaaS with a trial:** `last_non_direct` with a 90-day window, so a trial user who comes back by typing your URL is still credited to the channel that brought them. Pick a window longer than the time people usually take from first visit to first charge, trial included.
- **Ecommerce:** `last_non_direct` with a 30-day window when most orders follow within days of the first visit. Use 60 or 90 days for expensive or seasonal products that people compare for weeks.
- **Agencies reporting to clients:** `last_non_direct` with a 90-day window on every client's site, so reports are comparable between clients. Both are set per site, so note in the report when a client uses something else.
- **Sales-led B2B:** `first_touch` with a 365-day window, so the visit that introduced the buyer still counts after months of calls and demos.

To choose a window from your own buying cycle, read [how long a visit gets credit](https://virling.com/blog/attribution-window).

## Worked example

A made-up journey on a site with a 90-day window. A visitor comes three times from the same browser and pays $100 on day 41, right after the third visit.

| Visit | Channel | Last non-direct | First touch | Last touch |
| --- | --- | --- | --- | --- |
| Day 1: a link on Hacker News | Communities | | Credited | |
| Day 21: a Google search | Organic search | Credited | | |
| Day 41: typed the URL | Direct | | | Credited |

Each model credits the full $100 to one visit. With a 30-day window the day 1 visit is too old, so first touch credits the Google search instead, and the other two stay the same.

To try other journeys, use the [attribution model calculator](https://virling.com/tools/attribution-model-calculator).

## Changing either

Saving the settings rebuilds attribution for every payment of the site right away, history included. The dashboard and payments list reflect the new model on the next page load.

Other models, such as linear or position-based, are <span class="na">Not available yet</span>.

## Common questions

### Does changing the model or window rewrite history?

Yes. Saving the settings recomputes the credit of every payment of the site, and renewals follow the new credit of the customer's first payment, see [Renewals](renewals.md). The payments themselves do not change, so total revenue stays the same, and changing the settings back restores the earlier credit for the same visits and payments.

### Why did Direct grow after I changed the model or window?

Last touch counts Direct visits like any other, so a buyer who typed your URL before paying moves to Direct. First touch does the same when the earliest visit in the window was Direct. Under last non-direct, a shorter window can leave only Direct visits inside, and then the latest of them is credited.

### What happens to visits outside the window?

They are ignored under every model, so first touch is the earliest visit inside the window, not the first visit ever. If no visit is left, the payment is Unattributed and still counts toward revenue, see [Unattributed revenue](unattributed.md). The window counts back from the payment, or from the [start of the free trial](renewals.md#free-trials) when the payment has no visitor ID of its own.

### Is linear attribution available?

No. Each payment is credited to one visit by one of the three models above, and linear and position-based are not available yet. The [attribution model calculator](https://virling.com/tools/attribution-model-calculator) shows how they would split a payment.
