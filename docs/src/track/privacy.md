# Privacy and consent

## What is stored

For each event Virling stores: the time, the path without its query string, the referrer host, UTM tags, the channel, the country, the device type, the browser name, the operating system, the browser language, the site host, a visitor ID and a session ID. The region and city are stored too when the proxy or CDN in front of Virling sends them, as Cloudflare and Vercel do. For goals, the goal name. For outbound link clicks, the destination host and path without its query string. When the page is closed or left, a leave event with no extra data, used for time on page and the exit page. When a page is left after more than a second, how long it was visible and how far it was scrolled, and with the first one, four timings of the page load: largest contentful paint, the slowest interaction, layout shift and time to first byte.

Events do not include IP addresses, full user agents, full referrer URLs, or anything a visitor types.

The [privacy policy](https://virling.com/privacy) covers the hosted service.

## The visitor cookie

| Property | Value |
| --- | --- |
| Name | `virling_vid` |
| Content | A random ID, not derived from the visitor. With **Until consent**, the daily hash the visitor was counted under |
| Lifetime | One year |
| Scope | First-party, path `/`, on your site's domain |
| Flags | `SameSite=Lax`, plus `Secure` on HTTPS |

It is readable by your own code on purpose, so you can pass it into checkout. To use no cookie at all, see [Cookieless mode](cookieless.md). To set it only after consent, see [Cookieless until consent](cookieless.md#cookieless-until-consent).

## Which mode should I use?

Set **Visitor cookie** in your site's settings. **Until consent** and **Cookieless** also need `data-cookieless` on the script tag.

| Mode | Use it when | Stored in the browser | What counts | Do payments link to visits? |
| --- | --- | --- | --- | --- |
| **Cookie** | You want revenue by channel, and the cookie may be set on the first page | `virling_vid` for one year | Every visit, and returning visitors are recognised | Yes, when checkout carries the ID |
| **Until consent** | You ask for consent and want revenue by channel from visitors who accept | Nothing before consent, then `virling_vid` for one year | Every visit. Before consent a returning visitor counts as new each day, after it they are recognised | Only for visitors who accept, when checkout carries the ID |
| **Cookieless** | You would rather not ask, and revenue by channel does not matter for that site | Nothing | Every visit, but a returning visitor counts as new each day | No. New payments are Unattributed, unless [matched by email](../attribution/unattributed.md#recover-payments-by-email) |

Unattributed payments still count toward revenue. See [Cookieless mode](cookieless.md) for how visitors are counted without a cookie.

## Global Privacy Control

Add `data-respect-gpc` to the tag and the script sends nothing when the browser has Global Privacy Control turned on (`navigator.globalPrivacyControl`).

## Bots

Events from user agents that look automated are dropped: an empty user agent, or one containing `bot`, `crawl`, `spider`, `slurp`, `headless`, `lighthouse`, `preview`, `curl/`, `wget/`, `python-`, `go-http`, `phantomjs`, `facebookexternalhit`, `playwright`, `puppeteer`, `selenium`, `webdriver`, `scrapy`, `pingdom`, `uptime`, `gtmetrix`, `pagespeed`, `prerender`, `okhttp`, `axios/`, `node-fetch`, `java/` or `postman`. Events whose referrer is a known referrer-spam domain, such as `semalt.com`, are dropped too.

## Rate limit

Each site accepts up to 200 events per second by default. Events over the limit are dropped. Change it with `--events-per-second` or `VIRLING_EVENTS_PER_SECOND`.

## Consent banners

Whether you need consent for the `virling_vid` cookie depends on your jurisdiction and how you use the data. To count every visit but set the cookie only after consent, use [Cookieless until consent](cookieless.md#cookieless-until-consent) and call `virling("consent")` from your consent tool. To send nothing at all before consent, add the tag from your consent tool's callback instead, and include `data-site` on it as usual.

These are facts you can put in your own privacy or cookie policy. They are not legal advice.

- **Name:** `virling_vid`.
- **Purpose:** to recognise a returning visitor and link a payment to the visits before it.
- **Lifetime:** one year. In Cookie mode it is renewed on each page load. With Until consent it runs from the day of consent and is not renewed.
- **First-party:** the script sets it on your own domain, shared with its subdomains. Other websites cannot read it.
- **Content:** a random ID. With Until consent, a salted hash of the site, the IP address and the user agent.
- **Who receives it:** your Virling server, and your payment provider when checkout carries the ID. The script adds the ID to the [hosted checkout links](../payments/checkout.md#checkout-links-need-no-code) it knows.
- **Without the cookie:** in Cookieless mode, and before consent with Until consent, the IP address and the user agent are used in memory to make a hash that changes every day. The IP address is not stored.
- **Opt-out cookie:** `virling_ignore`, one year, set only in a browser that opens your site with `#virling-ignore`. While it is set the script sends nothing.

## Common questions

### Is the IP address stored?

Not for visitors. It is used in memory, to skip [ignored addresses](script.md#when-it-does-not-track) and to make the daily hash when there is no visitor ID. The country comes from a proxy header or the browser's time zone. A proxy or CDN in front of Virling can keep its own logs, and Virling does store the IP address that requests a sign-in link to the app, to limit how many are sent.

### Can I delete one visitor's data?

Not yet. Deleting one visitor in the app, the API or the CLI is <span class="na">Not available yet</span>. On the hosted service, email hello@virling.com with the site and the visitor ID, as the [privacy policy](https://virling.com/privacy) describes. On your own server, an [admin](../self-host/admin.md) can delete a whole site with its events, but there is no tested way to delete one visitor. Clearing the cookie makes the browser a new visitor and leaves stored events in place.

### Does it work behind a consent tool?

Yes, with any tool that can run your code when a visitor accepts: call `virling("consent")` with [Until consent](cookieless.md#cookieless-until-consent), or add the tag at that point. Virling has no banner of its own and does not read a consent tool's signals. If a visitor withdraws, your code has to delete the `virling_vid` cookie. The page that is open can keep using the ID until the next page load, and events already stored stay.

### Where is data kept when I self-host?

In the [data folder](../self-host/data.md#the-data-folder) on your server: events and payments in `events.duckdb`, accounts and settings in `virling.sqlite`. Visitors' browsers send events only to your server, and there is no telemetry. [Outbound connections](../reference/outbound.md) lists every host the server itself contacts.

### What happens with Global Privacy Control?

Nothing, unless the tag has `data-respect-gpc`. With it, for a browser with Global Privacy Control on, the script sends nothing, sets no cookie and adds no ID to checkout links, so the visit is not counted. It does not delete a cookie that is already there. The older Do Not Track setting is not read.

### How long is data kept?

Until it is deleted. Nothing expires, and a retention setting is <span class="na">Not available yet</span>. See [Retention and export](../self-host/data.md#retention-and-export).
