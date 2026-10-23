# Privacy and consent

## What is stored

For each event Virling stores: the time, the path without its query string, the referrer host, UTM tags, the channel, the country, the device type, the browser name, the operating system, the browser language, the site host, a visitor ID and a session ID. The region and city are stored too when the proxy or CDN in front of Virling sends them, as Cloudflare and Vercel do. For goals, the goal name. For outbound link clicks, the destination host and path without its query string. When the page is closed or left, a leave event with no extra data, used for time on page and the exit page.

Events do not include IP addresses, full user agents, full referrer URLs, or anything a visitor types.

The [privacy policy](https://virling.com/privacy) covers the hosted service.

## The visitor cookie

| Property | Value |
| --- | --- |
| Name | `virling_vid` |
| Content | A random ID, not derived from the visitor |
| Lifetime | One year |
| Scope | First-party, path `/`, on your site's domain |
| Flags | `SameSite=Lax`, plus `Secure` on HTTPS |

It is readable by your own code on purpose, so you can pass it into checkout. To use no cookie at all, see [Cookieless mode](cookieless.md).

## Global Privacy Control

Add `data-respect-gpc` to the tag and the script sends nothing when the browser has Global Privacy Control turned on (`navigator.globalPrivacyControl`).

## Bots

Events from user agents that look automated are dropped: an empty user agent, or one containing `bot`, `crawl`, `spider`, `slurp`, `headless`, `lighthouse`, `preview`, `curl/`, `wget/`, `python-`, `go-http`, `phantomjs`, `facebookexternalhit`, `playwright`, `puppeteer`, `selenium`, `webdriver`, `scrapy`, `pingdom`, `uptime`, `gtmetrix`, `pagespeed`, `prerender`, `okhttp`, `axios/`, `node-fetch`, `java/` or `postman`. Events whose referrer is a known referrer-spam domain, such as `semalt.com`, are dropped too.

## Rate limit

Each site accepts up to 200 events per second by default. Events over the limit are dropped. Change it with `--events-per-second` or `VIRLING_EVENTS_PER_SECOND`.

## Consent banners

Whether you need consent for the `virling_vid` cookie depends on your jurisdiction and how you use the data. Guidance on consent wording is <span class="na">Not available yet</span>, and these docs are not legal advice. If you only want to load the script after consent, add the tag from your consent tool's callback, and include `data-site` on it as usual.
