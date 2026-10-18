# Script attributes and API

The tracking script is served at `/v.js` by every Virling server. It is about 1 KB gzipped, and its source is [`assets/v.src.js`](https://github.com/virling/virling/blob/main/assets/v.src.js).

```html
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

## Attributes

| Attribute | | What it does |
| --- | --- | --- |
| `data-site` | Required | The site's domain as added to Virling. Without it the script does nothing. |
| `data-api` | Optional | Full URL to send events to. Default: `/api/event` on the host the script was loaded from. |
| `data-cookieless` | Optional | Never read or set the `virling_vid` cookie. See [Cookieless mode](../track/cookieless.md). |
| `data-respect-gpc` | Optional | Send nothing when `navigator.globalPrivacyControl` is on. |
| `data-dev` | Optional | Also track on `localhost`, `127.*` and `[::1]`, which are skipped by default. |

Boolean attributes only need to be present: `data-cookieless`, not `data-cookieless="true"`.

## JavaScript API

### `virling("goal", name)`

Sends a goal. See [Goals](../track/goals.md).

```js
virling("goal", "signup");
```

### `virling.vid()`

Returns the visitor ID, or `null` in cookieless mode. Pass it into checkout. See [Link checkout to visits](../payments/checkout.md).

Call it as `window.virling?.vid?.()`, which returns nothing instead of throwing when the script is blocked or has not loaded yet:

```js
const id = window.virling?.vid?.();
if (id) { /* pass it into checkout */ }
```

### `virling.q`

Calls queued in `window.virling.q` before the script loads are replayed in order. See [Calling before the script loads](../track/goals.md#calling-before-the-script-loads).

## The cookie

| Name | Value | Lifetime | Flags |
| --- | --- | --- | --- |
| `virling_vid` | `v` followed by a timestamp and random characters | 1 year, renewed on each visit | Domain is the registrable domain, so subdomains share it; Path `/`, `SameSite=Lax`, `Secure` on HTTPS |

The server accepts visitor IDs of 8 to 64 letters, digits and dashes. Anything else is replaced with the daily cookieless hash.

## Automatic behavior

- A pageview on load, and on `history.pushState` and `popstate` when the path or query changes. See [Single-page apps](../track/spa.md).
- Clicks on links to `buy.stripe.com` get `client_reference_id`, links to `buy.polar.sh` get `custom_field_data.virling_vid`, and links to `checkout.dodopayments.com` get `metadata_virling_vid`, set to the visitor ID. See [Payment links](../payments/checkout.md#payment-links).

## Caching

`/v.js` is served with `Cache-Control: public, max-age=3600` and `Access-Control-Allow-Origin: *`. After an upgrade, browsers pick up the new script within an hour.
