# Add the script

One tag, about 1.2 KB gzipped. It records pageviews, referrers, UTM tags, route changes in single-page apps and the goals you send.

```html
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

Replace `stats.example.com` with the public address of your Virling server, the same value as `VIRLING_BASE_URL`. Replace `yourdomain.com` with the domain you added with `add-site` or in the app. The app's setup screen and site settings show the exact tag for each site.

## Where it goes

Put it in the `<head>` of every page, before `</head>`. With `defer` it downloads in parallel and runs after the page is parsed, so it never blocks rendering. See [Framework guides](frameworks.md) for Next.js, Astro, WordPress, Framer and Webflow.

`data-site` must match the domain exactly as it was added, without `www.` and without a path. Events from a page are kept when the page's host is that domain or one of its subdomains: with `data-site="example.com"`, pages on `www.example.com` and `app.example.com` count too.

## What it sends

On load, and on each route change, the script sends one pageview to `/api/event` on the host it was loaded from:

| Field | Value |
| --- | --- |
| Page URL | `location.href`. UTM tags are read from its query string. The stored path drops the query. |
| Referrer | `document.referrer` on the first event of the page load. Later route changes send none. |
| Site | The `data-site` value. |
| Visitor ID | The `virling_vid` first-party cookie, unless cookieless. |
| Screen width | `screen.width` in CSS pixels, used for Desktop, Tablet or Mobile. |
| Language | `navigator.language`, stored as a code such as `en-US`. In cookieless mode only the base code, such as `en`, is kept. |
| Bot flag | Sent only when `navigator.webdriver` is set or `navigator.languages` is empty, as in Playwright, Puppeteer or Selenium. |

Each event is sent immediately, with `navigator.sendBeacon` when the browser has it and `fetch` with `keepalive` otherwise. Nothing is batched or held back, so an event still arrives when the visitor leaves right after.

The server adds the country from a proxy header and the browser name from the user agent. See [Event API](../reference/event-api.md) for the full format.

## Outbound links

A click on a link to another site is sent as an exit with the link's origin and path. Links between `www.` and the bare domain, and to subdomains, stay inside the site. Add other domains that belong to the same site, such as a separate app or docs domain, under Same-visit domains in Site settings, and clicks to them are not counted as exits either.

## Leaving and coming back

When the page is closed or the visitor navigates away, the script sends a leave, used for time on page and the exit page. Switching tabs is not a leave, and neither is closing one of several open tabs of the site: open tabs find each other with `BroadcastChannel`.

Each page load sends its navigation type (`navigate`, `reload` or `back_forward`). A page restored by Back from the browser's back/forward cache sends a new pageview with `back_forward`. The server uses both to tell a return from a reload; see [Sessions](../attribution/index.md#sessions). A return to the visit shows as "Came back" on the visitor's timeline, after a "Left to" row when the visitor had clicked out.

## UTM tags and ref

These query parameters on the landing URL are stored with the pageview: `utm_source`, `utm_medium`, `utm_campaign`, `utm_content` and `utm_term`. When `utm_source` is missing, `ref` is used instead, so `?ref=producthunt` works. Each value is trimmed and cut to 200 characters. They feed the [channel rules](../attribution/channels.md).

## Visitor ID

On the first visit the script creates a random ID such as `vlx3k2a9b7f1c0d4e8` and stores it in the `virling_vid` cookie for one year, with `SameSite=Lax`, and `Secure` on HTTPS. It is first-party: it is set on your registrable domain (`example.com` for `www.example.com` or `shop.example.co.uk` for itself), not on the Virling server's, so subdomains such as `app.example.com` share the same ID. This ID is what links a payment back to the visit. See [Link checkout to visits](../payments/checkout.md).

With [cookieless mode](cookieless.md) no cookie is set.

## When it does not track

- On `localhost`, `127.*` and `[::1]`, unless the tag has `data-dev`.
- When the tag has `data-respect-gpc` and the browser sends Global Privacy Control.
- When `data-site` is missing.

On the server, events from bots, from pages on another host and for sites that do not exist are dropped. Events from automated browsers and pages with no screen are kept apart as bots and left out of every number. See [Privacy and consent](privacy.md).

## Check that it works

Open your site in another tab. The setup screen in the app updates on its own when the first pageview arrives. You can also watch the browser's network panel for a `POST` to `/api/event` that returns `202`.
