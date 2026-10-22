# Event API

The script talks to one endpoint. You can call it yourself, for example to send events from a server.

## POST /api/event

The body is one JSON event, or an array of up to 50. Extra items past 50 are ignored. Any content type is accepted, so `navigator.sendBeacon` with a string works.

```http
POST /api/event HTTP/1.1
Host: stats.example.com
User-Agent: Mozilla/5.0 ...

[{"n":"pageview","u":"https://example.com/pricing?utm_source=newsletter","r":"https://mail.google.com/","s":"example.com","v":"vlx3k2a9b7f1c0d4e8","w":1440}]
```

| Field | Type | | Meaning |
| --- | --- | --- | --- |
| `n` | string | Required | `"pageview"`, `"goal"`, `"exit"`, `"leave"` or `"ping"`. A ping only keeps the visitor live and is not stored. Other values are dropped. |
| `u` | string | Required | Full page URL. Its host must be the site's domain or a subdomain. |
| `s` | string | Required | The site's domain, as in `data-site`. |
| `r` | string or null | Optional | Referrer URL. Only its host is kept, and only when it is another site. |
| `v` | string or null | Optional | Visitor ID, 8 to 64 letters, digits or dashes. Missing or invalid: a daily hash is used. |
| `w` | number | Optional | Screen width in CSS pixels. Below 768 is Mobile, below 1024 Tablet, else Desktop. Missing: Unknown. |
| `d` | number | Optional | `devicePixelRatio`. Only used with `w` to spot a browser that claims to be a Mac on a screen no Mac has. Not stored. |
| `l` | string | Optional | Browser language tag. Kept as `en` or `en-US`; cookieless sites keep only `en`. Missing: the first `Accept-Language` entry. |
| `k` | string | Optional | Navigation type of a page load: `navigate`, `reload` or `back_forward`. Absent on route changes and other events. With it, a pageview can count as a return to the site; see [Sessions](../attribution/index.md#sessions). |
| `z` | string | Optional | Browser time zone, such as `Europe/Berlin`. Gives the country when no country header does. Not stored. |
| `b` | boolean | Optional | `true` when the browser is automated (`navigator.webdriver`) or reports no languages. The event is kept apart as a bot and left out of every number. |
| `g` | string | For goals and exits | Goal name, trimmed and cut to 64 bytes, or the link URL for an exit. Goals without one are dropped. |

When a pageview comes back to the visit after a leave, the server also stores a `back` event just before it, shown as "Came back" on the timeline. The server adds the time, the channel, the session, the country from `cf-ipcountry`, `x-vercel-ip-country` or `x-country-code` (or from `z` when none is sent), and the browser from `User-Agent`.

## Responses

| Status | Body | When |
| --- | --- | --- |
| `202` | empty | The body parsed. Events may still have been dropped, see below. |
| `400` | `invalid event` | The body is not a JSON event or array of events. |
| `503` | `try again` | Events could not be written. Retry later. |

These are dropped silently, still with `202`:

- Requests whose `User-Agent` looks like a bot, or is empty.
- Events with `b: true`, a screen width of `0`, or a Chrome user agent on macOS with a screen under 1,024 device pixels (`w` times `d`), which no Mac has but headless browsers posing as Macs report. They are stored apart in the `bot_events` table for debugging, and Site settings shows how many bot visitors were filtered in the last 30 days.
- Events for a site that does not exist.
- Events whose page host is not the site's domain or a subdomain.
- Events over the site's rate limit.
- Goals without a name.
- Exits to the same site (with or without `www.`, or a subdomain), to a domain on the site's same-visit list in Site settings, or to a non-web link, and exits or leaves with no visit in the last 30 minutes.

CORS is open: responses carry `Access-Control-Allow-Origin: *`, and `OPTIONS /api/event` answers preflight requests.

## GET /v.js

The tracking script. See [Script attributes and API](script.md).

## GET /health

Returns `200` with the body `ok` while the server runs. Use it for uptime checks and container health checks.

## Reading stats

To read totals, breakdowns, payments and visitor timelines, use the [Read API](api.md).

## POST /api/crawl

Counts a page fetch by an AI crawler, sent from your server or edge. The body is JSON with `s` (site domain), `u` (full page URL) and `ua` (the crawler's user agent). Only known AI crawlers on pages of that site are kept; the response is always `202`. See [AI crawlers](../track/ai-crawlers.md).
