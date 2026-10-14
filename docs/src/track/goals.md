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

## What is not available yet

- Goal properties, such as a plan name on a signup <span class="na">Not available yet</span>
- Goal revenue, a money value attached to a goal <span class="na">Not available yet</span>

Revenue comes from your payment provider instead. See [Payments](../payments/index.md).
