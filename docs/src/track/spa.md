# Single-page apps

Client-side routing is tracked without extra code. The script hooks into the History API:

- `history.pushState` and `history.replaceState`, which routers call when you navigate to a new page.
- The `popstate` event, fired by the back and forward buttons.

## When a pageview is sent

A pageview is sent only when the path or the query string changes. Changing only the `#hash` sends nothing, and the same URL twice in a row, pushed or replaced, sends one pageview.

The referrer is sent with the first event of a page load only, normally its pageview. Route changes after it carry no referrer, so they stay in the same channel as the landing.

## Sessions

Pageviews from the same visitor belong to one session until there are 30 minutes without an event. See [How attribution works](../attribution/index.md).

## Frameworks

This covers Next.js, Nuxt, SvelteKit, Remix, React Router, Vue Router and Astro view transitions, since all of them navigate with `pushState` or `replaceState`. Load the script once in the root layout. See [Framework guides](frameworks.md).

## Check that it works

1. Open your published site and your browser's network tab, show all request types and filter by `api/event`. Events go out with `navigator.sendBeacon`, so a Fetch/XHR filter can hide them.
2. Reload. One request has `"n":"pageview"` in its body and is answered with `202`.
3. Click a link to another route. One more pageview is sent, with the new address in `u`. A `"n":"read"` request may come just before it: it reports the page you left, when it was visible for more than a second.
4. Press Back. One more pageview is sent, for the page you returned to.
5. Click a link that changes only the `#hash`. No pageview is sent.

A `"n":"ping"` every 30 seconds while the tab is visible is normal. Then run a test visit and payment through [Test your setup](../guides/test-your-setup.md).

## Common problems

| Problem | Cause | Fix |
| --- | --- | --- |
| Two pageviews per route | The script is loaded twice, such as in a layout and in a tag manager. Each copy sends its own pageview. | Load it once, in the root layout. See [Framework guides](frameworks.md). |
| Only the first page is counted | The router keeps the route in the hash, such as `/#/pricing`, so the path and query never change. | Switch the router to history mode, with real paths. |
| Only the first page is counted, with real paths | The router navigates without `pushState`, `replaceState` or `popstate`. The script listens to nothing else. | Use the router's History API mode. |
| Too many pageviews on one page | A search box or filter writes to the query string. Each new query is a pageview, and the stored path drops the query. | Keep that state out of the query string. |
| A redirect counts as a page | The app pushes one URL, then replaces it with another. Both are different URLs, so both are sent. | Redirect before the route is pushed, or on the server. |
| Nothing is sent | The script reads `data-site` from its own tag. Bundled, or loaded as `type="module"`, it cannot find it. | Keep it a plain external script, as in the [framework guides](frameworks.md). |
| Nothing is sent on `localhost` | It is skipped there, and on `127.*` and `[::1]`. | Test on the published site, or see [`data-dev`](../reference/script.md#attributes). |
| A `virling("pageview")` call is not counted | There is no pageview command. | Remove it. Route changes are sent on their own. |
| A payment button loses the visitor | Checkout links are tagged on page load and on click. A checkout your code opens is not a link. | Pass the visitor ID yourself. See [Link checkout to visits](../payments/checkout.md#read-the-visitor-id). |

Requests that are blocked or not answered with `202` are covered in [Troubleshooting](../troubleshooting.md#no-visitors-show-up). For a whole product setup, see [Set up Virling for a SaaS product](../guides/saas.md).
