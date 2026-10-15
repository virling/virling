# Model and lookback window

Each site has one attribution model and one lookback window. Change both in the app under site settings, at `/app/yourdomain.com/settings`.

## Models

| Model | Setting value | Credits |
| --- | --- | --- |
| Last non-direct | `last_non_direct`, the default | The latest session in the window that was not Direct. If every session was Direct, the latest Direct session. |
| First touch | `first_touch` | The earliest session in the window. |

Last non-direct suits most products: a visitor who found you on Hacker News and came back a week later by typing your URL is credited to Hacker News, not Direct.

First touch shows which channels introduce people who later pay, and fits longer buying cycles.

## Lookback window

Only sessions that started inside the window before the payment are considered. Choose 30, 60, 90 or 365 days. The default is 90.

A shorter window credits only recent visits. Sessions older than the window are ignored, and if none are left the payment is Unattributed.

## Changing either

Saving the settings rebuilds attribution for every payment of the site right away, history included. The dashboard and payments list reflect the new model on the next page load.

Other models, such as linear or position-based, are <span class="na">Not available yet</span>.
