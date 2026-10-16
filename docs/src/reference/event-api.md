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
| `n` | string | Required | `"pageview"`, `"goal"`, `"exit"` or `"leave"`. Other values are dropped. |
| `u` | string | Required | Full page URL. Its host must be the site's domain or a subdomain. |
| `s` | string | Required | The site's domain, as in `data-site`. |
| `r` | string or null | Optional | Referrer URL. Only its host is kept, and only when it is another site. |
| `v` | string or null | Optional | Visitor ID, 8 to 64 letters, digits or dashes. Missing or invalid: a daily hash is used. |
| `w` | number | Optional | Screen width in CSS pixels. Below 768 is Mobile, below 1024 Tablet, else Desktop. Missing: Unknown. |
| `l` | string | Optional | Browser language tag. Kept as `en` or `en-US`; cookieless sites keep only `en`. Missing: the first `Accept-Language` entry. |
| `g` | string | For goals and exits | Goal name, trimmed and cut to 64 bytes, or the link URL for an exit. Goals without one are dropped. |

The server adds the time, the channel, the session, the country from `cf-ipcountry`, `x-vercel-ip-country` or `x-country-code`, and the browser from `User-Agent`.

## Responses

| Status | Body | When |
| --- | --- | --- |
| `202` | empty | The body parsed. Events may still have been dropped, see below. |
| `400` | `invalid event` | The body is not a JSON event or array of events. |
| `503` | `try again` | Events could not be written. Retry later. |

These are dropped silently, still with `202`:

- Requests whose `User-Agent` looks like a bot, or is empty.
- Events for a site that does not exist.
- Events whose page host is not the site's domain or a subdomain.
- Events over the site's rate limit.
- Goals without a name.
- Exits to the same site or to a non-web link, and exits or leaves with no visit in the last 30 minutes.

CORS is open: responses carry `Access-Control-Allow-Origin: *`, and `OPTIONS /api/event` answers preflight requests.

## GET /v.js

The tracking script. See [Script attributes and API](script.md).

## GET /health

Returns `200` with the body `ok` while the server runs. Use it for uptime checks and container health checks.

## Reading stats

To read totals, breakdowns, payments and visitor timelines, use the [Read API](api.md).
