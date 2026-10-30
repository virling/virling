# Switch in two minutes

Coming from DataFast? Virling understands its script API, so you swap the script tag and keep the rest of your code: goal calls, `data-fast-goal` buttons and the checkout code that passes the visitor ID.

## 1. Add your site

Add your domain in the app, or with `add-site` on your own server. See [Quickstart](../quickstart.md).

## 2. Swap the script tag

Remove the DataFast tag:

```html
<script defer data-website-id="dfid_..." data-domain="yourdomain.com"
  src="https://datafa.st/js/script.js"></script>
```

Put these two tags in its place, in this order:

```html
<script defer src="https://stats.example.com/v.js" data-site="yourdomain.com"></script>
<script defer src="https://stats.example.com/v-compat.js"></script>
```

Replace `stats.example.com` with the address of your Virling server, as in [Add the script](script.md). Keep the `datafast-queue` snippet in your `<head>` if you have it: calls queued before load are sent once both scripts run.

That is the whole switch. Payments start matching within a minute of the next sync once your [payment provider](../payments/index.md) is connected.

## What keeps working

| Your code | What Virling does |
| --- | --- |
| `window.datafast("signup")` | Sends the goal `signup`, like `virling("goal", "signup")`. |
| Calls queued in `window.datafast.q` before load | Sent in order when `v-compat.js` runs. |
| `<button data-fast-goal="initiate_checkout">` | Sends the goal on click. |
| Checkout code that reads the `datafast_visitor_id` cookie | `v-compat.js` sets that cookie to the Virling visitor ID. |
| `datafast_visitor_id` in Stripe, Dodo Payments or Polar metadata, or in LemonSqueezy custom data | Read as the visitor ID when `virling_vid` is absent. |

## What is dropped

- Goal parameters, such as the object in `datafast("signup", { plan: "pro" })` and `data-fast-goal-*` attributes. Goals keep their name only. See [Goals](goals.md).
- `datafast_session_id`. Virling builds sessions from the visitor's events.
- Scroll events (`data-fast-scroll`), user identification and the other `data-` options on the old tag.
- Past data. Your DataFast history stays in DataFast.

## Later

`v-compat.js` is small and optional. When you have time, switch your calls to `virling("goal", ...)` and your checkout code to the `virling_vid` cookie, as in [Link checkout to visits](../payments/checkout.md), then remove the second tag.
