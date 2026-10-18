# Single-page apps

Client-side routing is tracked without extra code. The script hooks into the History API:

- `history.pushState` and `history.replaceState`, which routers call when you navigate to a new page.
- The `popstate` event, fired by the back and forward buttons.

## When a pageview is sent

A pageview is sent only when the path or the query string changes. Changing only the `#hash` sends nothing, and the same URL twice in a row, pushed or replaced, sends one pageview.

The referrer is sent with the first pageview of a page load only. Route changes after it carry no referrer, so they stay in the same channel as the landing.

## Sessions

Pageviews from the same visitor belong to one session until there are 30 minutes without an event. See [How attribution works](../attribution/index.md).

## Frameworks

This covers Next.js, Nuxt, SvelteKit, Remix, React Router, Vue Router and Astro view transitions, since all of them navigate with `pushState` or `replaceState`. Load the script once in the root layout. See [Framework guides](frameworks.md).
