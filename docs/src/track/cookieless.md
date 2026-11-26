# Cookieless mode

In cookieless mode Virling sets no cookie and stores nothing in the browser. Visitors are counted with a hash instead.

## Turn it on

Do both:

1. In the app, open your site's settings and set **Visitor cookie** to **Cookieless**. The server then ignores any visitor ID the browser sends.
2. Add `data-cookieless` to the script tag, so the `virling_vid` cookie is never set:

```html
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com" data-cookieless></script>
```

The settings page shows the tag with the attribute added.

## How visitors are counted

Each event gets a visitor ID made from a hash of the site, the IP address and the user agent, with a random salt. The salt lives only in memory. It changes every day at midnight UTC, and on a server restart, and the old one is never stored. The IP address itself is not stored.

The same salted hash is also used for any event that arrives without a valid visitor ID, for example from a browser that blocks cookies.

## What you give up

> [!WARNING]
> **Payments become Unattributed.** Without a visitor ID there is nothing to pass into checkout, so new payments cannot be linked to a visit. They still count toward revenue, as Unattributed.

- A visitor is recognised only until midnight UTC or the next server restart, whichever comes first. After that they count as a new visitor.
- A visit that runs past midnight UTC splits into two visits by two visitors.
- Two people behind the same IP address with the same browser version count as one.
- Sessions still work within a day, since they only need 30 minutes of continuity.
- The browser language is stored without its region, such as `en` instead of `en-GB`.

Payments imported before you switched keep the attribution they already had.

### A payment days later

Say a visitor arrives from a newsletter on Monday and pays on Thursday. Checkout carries no visitor ID, so the payment is Unattributed, whatever the [model and lookback window](../attribution/model.md). Even a payment a minute after the visit is Unattributed, for the same reason.

The one way back is an [identify event](../attribution/unattributed.md#recover-payments-by-email) with the visitor's email, for Stripe, Shopify and WooCommerce. A later payment with the same payer email is then credited to the visitor who identified. In cookieless mode that visitor is the daily hash, so only visits on the day they identified, from the same browser, can get the credit. If they identify on Thursday, the newsletter visit on Monday was a different visitor and gets none.

## Cookieless until consent

Set **Visitor cookie** to **Until consent** to start every visitor cookieless and switch to the cookie once they accept it. Keep `data-cookieless` on the script tag, and call this from your consent tool when the visitor accepts:

```js
virling("consent");
```

Before consent, visitors are counted with the daily hash, exactly as above. On consent, the script asks your Virling server for that hash ID and stores it in the `virling_vid` cookie, so the visit that is already running continues as the same visitor and session. From then on the visitor is counted by the cookie, returns are recognised, and checkout can carry the ID, so payments are attributed.

- The script never sets or renews the cookie by itself in this mode. Calling `virling("consent")` again on later pages does nothing once the cookie is there.
- The call loads a small module from `/api/event` on your Virling server. If you serve the script through your own proxy with `data-api`, forward `GET` requests to that path too.
- On a site in plain Cookieless mode the call does nothing.
- If a visitor withdraws consent, delete the `virling_vid` cookie in your consent tool's callback.

## When to use it

| Mode | Visitor recognised for | Payments linked to visits | Pick it when |
| --- | --- | --- | --- |
| **Cookie** | One year | Yes, when checkout carries the ID | Revenue by channel matters and the cookie may be set on the first page |
| **Until consent** | One day before consent, one year after | Only for visitors who accept | You ask for consent and still want revenue by channel |
| **Cookieless** | One day, or until a server restart | No, except by email match | You would rather not ask for consent and revenue attribution does not matter for that site |

This is not legal advice. Whether your site needs a consent banner depends on where you and your visitors are. See [Privacy and consent](privacy.md) for what Virling stores, and the [privacy policy](https://virling.com/privacy) for the hosted service.
