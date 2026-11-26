# Switch in two minutes

Coming from another revenue analytics tool? If it tracks goals with a global function and passes a visitor ID to your checkout, Virling's compatibility script keeps that code working. You swap the script tag and keep the rest: goal calls, goal buttons and the checkout code that passes the visitor ID.

## 1. Add your site

Add your domain in the app, or with `add-site` on your own server. See [Quickstart](../quickstart.md).

## 2. Swap the script tag

Remove your previous provider's script tag and put these two tags in its place, in this order:

```html
<script defer src="https://stats.example.com/v.js" data-site="yourdomain.com"></script>
<script defer src="https://stats.example.com/v-compat.js"></script>
```

Replace `stats.example.com` with the address of your Virling server, as in [Add the script](script.md). Keep your previous provider's queue snippet in your `<head>` if you have it: calls queued before load are sent once both scripts run.

That is the whole switch. Payments start matching within a minute of the next sync once your [payment provider](../payments/index.md) is connected.

## What keeps working

| Your code | What Virling does |
| --- | --- |
| Goal calls through your previous provider's global function | Sends each as a Virling goal, like `virling("goal", "signup")`. |
| Calls queued before the script loaded | Sent in order when `v-compat.js` runs. |
| Buttons marked with your previous provider's goal attribute | Sends the goal on click. |
| Checkout code that reads your previous provider's visitor ID cookie | `v-compat.js` sets that cookie to the Virling visitor ID. |
| Your previous provider's visitor ID key in Stripe, Dodo Payments or Polar metadata, or in LemonSqueezy custom data | Read as the visitor ID when `virling_vid` is absent. |

If a goal call is not picked up, switch it to `virling("goal", ...)`.

## What is dropped

- Goal parameters. Goals keep their name only. See [Goals](goals.md).
- Your previous provider's session ID. Virling builds sessions from the visitor's events.
- Scroll events, user identification and the other options on the old tag.
- Past visits. Your history stays with your previous provider, except the daily traffic you can [import](#import-your-history).

## Check that the new script counts

1. **The tag loads.** Open a page on your site with the network tab open. `v.js` loads from your Virling server, and the request to `/api/event` is answered with `202`.
2. **The visit is stored.** **Script installed** in the dashboard's setup checklist turns green. A `202` alone is not proof: events for a site you have not added are answered the same way and dropped.
3. **The source is right.** Open `https://yourdomain.com/?utm_source=test&utm_medium=test` in a private window. The visit shows under **Visitors**, with `test` on the Channels card.
4. **Your old goal calls arrive.** In the same window, trigger a goal your site already sends. It shows in that visitor's journey. The button attribute and the global function only work with `v-compat.js` loaded.
5. **The visitor ID is there.** `window.virling.vid()` in the browser console returns an ID, and the same value is in the `virling_vid` cookie. In [cookieless mode](cookieless.md) it returns `null`, which is expected.

Nothing shows up? See [No visitors show up](../troubleshooting.md#no-visitors-show-up).

## Run both scripts side by side

You can compare first and swap later. Keep your old tag and add only the `v.js` tag. Leave `v-compat.js` out until you remove the old one: it replaces your previous provider's global function and sets its visitor ID cookie to the Virling ID, so your old reports would lose goals and returning visitors.

| What you compare | How long | Why |
| --- | --- | --- |
| Visitors and channels | One full week, two if your traffic is uneven | Covers every weekday |
| Revenue by channel | As long as most buyers take from first visit to payment | Virling only credits visits it tracked, so early payments have no earlier visit to match. See [how long a visitor takes to become a customer](https://virling.com/blog/how-long-does-a-visitor-take-to-become-a-customer) |
| Renewals | Do not wait for them | A [renewal](../attribution/renewals.md) of a first payment made before the switch is usually Unattributed |

Two things to set while both run:

- **Checkout.** Keep the old visitor ID and add `virling_vid` next to it in the same metadata or custom data. Virling reads `virling_vid` first. With only the old key it reads your previous provider's ID, which matches none of its visits.
- **Payment links.** Where a link already carries a value in the field Virling uses, such as `client_reference_id` on `buy.stripe.com`, Virling leaves it alone. If your old script fills that field first, those payments are not linked until you remove it.

Do not expect equal totals. Two tools can cut visits, filter bots and treat returning visitors differently. Compare the order of your channels and the trend. Virling ends a visit after 30 minutes without activity and keeps [bots](privacy.md#bots) out of every number. Then swap the tags as in [step 2](#2-swap-the-script-tag).

## Import your history

| History | Can it come along? |
| --- | --- |
| Daily traffic from Plausible | Yes: [Import from Plausible](import-plausible.md) |
| Daily traffic from Google Analytics 4 | Yes: [Import from Google Analytics](import-google-analytics.md) |
| Daily traffic from other tools | <span class="na">Not available yet</span> |
| Payments | From your payment provider, not your old tool: the last 90 days, 60 for Shopify, and only one-time purchases for RevenueCat |
| Single visits, journeys, goals and which channel earned a past payment | No |

Imports run on your own server from the command line, with Virling stopped, so hosted accounts cannot use them. You can import before or after the switch: only days before the first event Virling tracked are kept, so no day counts twice.

Payments are only credited to tracked visits, so imported days add visitors but no revenue by channel. Payments from before the switch usually show as [Unattributed](../attribution/unattributed.md).

## Connect payments afterwards

1. Create a key with only the access in [Which key to create](../payments/index.md#which-key-to-create).
2. Open `/app/yourdomain.com/setup/payments`, pick the provider, paste the key and click **Connect and backfill**. You do not have to disconnect your old tool first: the two keys are separate.
3. Pass the visitor ID into checkout. With `v-compat.js`, the code from your old setup keeps working for the providers in [What keeps working](#what-keeps-working). For every other case, see [Link checkout to visits](../payments/checkout.md).
4. Make a test payment and check that it shows the right channel, as in [Test your setup](../guides/test-your-setup.md).

Once payments show a channel and you no longer use the old tool, revoke the key it had.

## Later

`v-compat.js` is small and optional. When you have time, switch your calls to `virling("goal", ...)` and your checkout code to the `virling_vid` cookie, as in [Link checkout to visits](../payments/checkout.md), then remove the second tag.
