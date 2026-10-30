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
- Past data. Your history stays with your previous provider.

## Later

`v-compat.js` is small and optional. When you have time, switch your calls to `virling("goal", ...)` and your checkout code to the `virling_vid` cookie, as in [Link checkout to visits](../payments/checkout.md), then remove the second tag.
