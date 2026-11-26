# Goals

A goal is a named action, such as a signup or a demo request. Send one from your code:

```js
virling("goal", "signup");
```

Goals are stored with the page they happened on, the session and the visitor, so they show up next to pageviews and in payment journeys.

## Rules

- The name is trimmed and cut to 64 bytes.
- A goal without a name, or with only spaces, is dropped by the server.
- Only the `"goal"` command exists. Other commands are ignored.
- Goals are sent immediately, like pageviews.

## Calling before the script loads

The script loads with `defer`, so code that runs earlier can find `virling` undefined. Add this stub once, before your own code, to queue calls:

```html
<script>
  window.virling = window.virling || function () {
    (window.virling.q = window.virling.q || []).push(arguments);
  };
</script>
```

When the script loads it replays everything in `virling.q` in order.

## Examples

```js
// After a signup form succeeds
form.addEventListener("submit", () => virling("goal", "signup"));

// On a button
document.querySelector("#book-demo").addEventListener("click", () => {
  virling("goal", "demo request");
});
```

## Which goals to send

Send a goal for the steps that come before money, and only once the step has succeeded. A few are enough. Names are compared exactly, so `Signup` and `signup` are two goals: pick lowercase names and keep them.

| Site | Goal names | When to send |
| --- | --- | --- |
| SaaS | `signup`, `trial`, `demo request` | The account is created, the trial starts, the demo form is accepted |
| Store | `add to cart`, `checkout started`, `newsletter signup` | The item is in the cart, the buyer opens checkout, the form is accepted |
| Agency client site | `contact form`, `call click`, `booking` | The form is accepted, a phone link is clicked, the booking is confirmed |

Do not send a goal for a purchase to count revenue: payments are imported from your provider. Leave names, emails and order numbers out of goal names.

For a SaaS, send the goal in the code that runs after your server answers. `/api/signup` stands for your own URL. Add the [stub](#calling-before-the-script-loads) first, so the call is safe when the script has not loaded or is blocked:

```js
const res = await fetch("/api/signup", { method: "POST", body: new FormData(form) });
if (res.ok) virling("goal", "signup");
```

In a single-page app, send it before you change the route if you want it stored with the page of the form. See [Set up Virling for a SaaS product](../guides/saas.md).

For a store, send `checkout started` when the buyer clicks your checkout button:

```js
document.querySelector("#checkout").addEventListener("click", () => {
  virling("goal", "checkout started");
});
```

See [Set up Virling for an online store](../guides/ecommerce.md).

For an agency client site, count clicks on phone links. Use the same names on every client site, so their reports read the same way:

```js
document.querySelectorAll('a[href^="tel:"]').forEach((a) => {
  a.addEventListener("click", () => virling("goal", "call click"));
});
```

See [Run Virling for client sites](../guides/agencies.md).

To check a goal, do the action and look in your browser's network tab for a request to `/api/event` whose body has `"n":"goal"` and your name in `g`. The script sends nothing on `localhost` unless the tag has `data-dev`. See [Test your setup](../guides/test-your-setup.md), [Framework guides](frameworks.md) for where the tag goes, and [Troubleshooting](../troubleshooting.md) if nothing is sent.

## What is not available yet

- Goal properties, such as a plan name on a signup <span class="na">Not available yet</span>
- Goal revenue, a money value attached to a goal <span class="na">Not available yet</span>

Revenue comes from your payment provider instead. See [Payments](../payments/index.md).
