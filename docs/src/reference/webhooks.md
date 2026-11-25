# Webhooks

Virling can post four kinds of events to a URL of yours, within a minute of them happening:

| Event | When |
| --- | --- |
| `payment.attributed` | A new paid payment is credited to a visit. Payments that stay unattributed for 10 minutes are skipped. |
| `goal.completed` | A visitor completes a [goal](../track/goals.md). |
| `alert.traffic_spike` | Visitors today reach at least 20 and three times the daily average of the week before, once a day at most. |
| `alert.unusual_change` | With **Unusual changes** on in Site settings > Email alerts: revenue or a channel moved sharply yesterday, a referrer sent its first buyer, or tracking stopped. Once a day at most, at the alert hour. |

Set it up in **Site settings > Webhooks**: paste a public `https://` URL and click **Send a test and turn on**. Virling sends a `ping` event first and only saves the URL when it answers with a 2xx status. Only the account owner can set it, and the URL and secret are stored encrypted. Webhooks stay off while a site holds sample data.

## Slack

Paste a Slack incoming webhook URL (`https://hooks.slack.com/...`) to get each event as a short message in a channel instead of JSON, such as "New payment on example.com: $49 from ChatGPT".

## Payload

Every other URL gets a `POST` with a JSON body:

```json
{
  "type": "payment.attributed",
  "site": "example.com",
  "created": "2026-11-01T09:30:00Z",
  "data": {
    "provider": "stripe",
    "payment_id": "pi_123",
    "paid_at": "2026-11-01T09:29:12Z",
    "amount": 4900,
    "currency": "usd",
    "amount_usd": 49.0,
    "renewal": false,
    "visitor_id": "v1abc",
    "customer_id": "cus_123",
    "source": "ChatGPT",
    "channel": "AI",
    "referrer": "chatgpt.com",
    "campaign": null,
    "landing_page": "/pricing"
  }
}
```

`amount` is in the currency's minor unit. A `goal.completed` event has `goal`, `path`, `visitor_id` and `completed_at` in `data`, an `alert.traffic_spike` event has `visitors_today` and `usual_per_day`, and an `alert.unusual_change` event has `changes`, a list with `headline`, `name`, `usual` and `yesterday`.

To keep a copy in a warehouse, point the webhook at a small endpoint or pipeline of yours that inserts each body as a row.

## Verify the signature

Each request carries two headers:

- `Virling-Event`: the event type.
- `Virling-Signature`: `t=<unix seconds>,v1=<hex>`, where the hex is the HMAC-SHA256 of `<t>.<raw body>` keyed with the signing secret shown in Site settings.

Compute the HMAC over the raw body before parsing it, compare it in constant time, and reject requests whose `t` is more than five minutes old. In Node.js:

```js,file:verify.mjs
import crypto from "node:crypto";

export function verify(secret, header, rawBody) {
  const { t, v1 = "" } = Object.fromEntries((header ?? "").split(",").map((p) => p.split("=")));
  const mac = Buffer.from(crypto.createHmac("sha256", secret).update(`${t}.${rawBody}`).digest("hex"));
  const sent = Buffer.from(v1);
  const fresh = Math.abs(Date.now() / 1000 - Number(t)) < 300;
  return fresh && mac.length === sent.length && crypto.timingSafeEqual(mac, sent);
}
```

In Python, with `raw_body` as bytes:

```python,file:verify.py
import hashlib
import hmac
import time


def verify(secret, header, raw_body):
    parts = dict(p.partition("=")[::2] for p in (header or "").split(","))
    t, v1 = parts.get("t", ""), parts.get("v1", "")
    mac = hmac.new(secret.encode(), t.encode() + b"." + raw_body, hashlib.sha256).hexdigest()
    fresh = t.isdecimal() and abs(time.time() - int(t)) < 300
    return fresh and hmac.compare_digest(mac.encode(), v1.encode())
```

The signing secret appears in Site settings only after the `ping` succeeds, so answer `ping` with a 2xx without checking its signature, and do nothing else with it. Check every other event.

## Recipes

Each recipe is a small Node.js server (18 or newer) that needs a public `https://` address, for example behind a [reverse proxy](../self-host/reverse-proxy.md). Save `verify.mjs` from above and this file next to it. It answers `ping`, refuses a request with a bad signature before reading the event, and answers 500 when the work fails so that Virling sends the event again:

```js,file:receive.mjs
import http from "node:http";
import { verify } from "./verify.mjs";

export function receive(handle) {
  const server = http.createServer(async (req, res) => {
    const answer = (status) => res.writeHead(status).end();
    try {
      if (req.headers["virling-event"] === "ping") return answer(204);
      const chunks = [];
      for await (const chunk of req) chunks.push(chunk);
      const body = Buffer.concat(chunks).toString();
      const secret = process.env.VIRLING_SECRET;
      if (!secret || !verify(secret, req.headers["virling-signature"], body)) return answer(401);
      await handle(JSON.parse(body));
      answer(204);
    } catch (err) {
      console.error(err);
      answer(500);
    }
  });
  server.listen(8080);
}
```

Start the recipe without `VIRLING_SECRET`, turn the webhook on, then restart it with the secret from Site settings, such as `VIRLING_SECRET=whsec_... node chat.mjs`. Events that arrive in between get a 401 and are sent again, except `alert.unusual_change`. A site has one webhook, so to run several recipes, call them from one `receive`.

### Add payments to a spreadsheet

In a Google Sheet, open **Extensions > Apps Script**, paste this with a long random token of your own, and deploy it as a web app that runs as you and that anyone can access. Apps Script cannot read request headers, so your endpoint checks the signature and passes on the row with the token:

```js,file:Code.gs
const TOKEN = "your-long-random-token";

function doPost(e) {
  const { token, row } = JSON.parse(e.postData.contents);
  if (token !== TOKEN) return ContentService.createTextOutput("refused");
  const lock = LockService.getScriptLock();
  lock.waitLock(2000);
  const sheet = SpreadsheetApp.getActiveSpreadsheet().getSheets()[0];
  const id = row[1].slice(1);
  if (!sheet.createTextFinder(id).matchEntireCell(true).matchCase(true).findNext()) sheet.appendRow(row);
  SpreadsheetApp.flush();
  return ContentService.createTextOutput("ok");
}
```

Run this with `SHEET_URL` set to the web app's URL and `SHEET_TOKEN` to the token:

```js,file:sheet.mjs
import { receive } from "./receive.mjs";

// A leading apostrophe keeps IDs and campaign names as plain text, never a formula.
const text = (v) => (v == null ? "" : `'${v}`);

receive(async ({ type, data }) => {
  if (type !== "payment.attributed") return;
  const row = [
    data.paid_at, text(data.payment_id), data.amount_usd ?? "", text(data.source),
    text(data.channel), text(data.campaign), text(data.landing_page), data.renewal,
  ];
  const res = await fetch(process.env.SHEET_URL, {
    method: "POST",
    body: JSON.stringify({ token: process.env.SHEET_TOKEN, row }),
    signal: AbortSignal.timeout(4000),
  });
  if ((await res.text()) !== "ok") throw new Error("the sheet did not take the row");
});
```

The script skips a payment ID that is already in the sheet, so an event sent twice adds one row.

### Post to a chat room

For Slack, paste its URL in Site settings as [above](#slack). For Discord, create a webhook in the channel's settings and run this with `CHAT_URL` set to its URL:

```js,file:chat.mjs
import { receive } from "./receive.mjs";

const price = (d) => (d.amount_usd == null ? `${d.amount} ${d.currency} (minor units)` : `$${d.amount_usd.toFixed(2)}`);

const messages = {
  "payment.attributed": (d) => `${d.renewal ? "Renewal" : "New payment"}: ${price(d)} from ${d.source}`,
  "goal.completed": (d) => `Goal ${d.goal} completed on ${d.path}`,
  "alert.traffic_spike": (d) => `Traffic spike: ${d.visitors_today} visitors today, usually ${d.usual_per_day}`,
  "alert.unusual_change": (d) => d.changes.map((c) => c.headline).join("\n"),
};

receive(async ({ type, site, data }) => {
  if (!messages[type]) return;
  const content = `${site}: ${messages[type](data)}`.slice(0, 2000);
  const res = await fetch(process.env.CHAT_URL, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ content, allowed_mentions: { parse: [] } }),
    signal: AbortSignal.timeout(4000),
  });
  if (!res.ok) throw new Error(`the chat answered ${res.status}`);
});
```

Rooms that take a `text` field need `{ text: content }` as the body instead. A message can repeat when the room took it but answered too late.

### Insert payments into Postgres

Create the table:

```sql
CREATE TABLE virling_payments (
  site text NOT NULL,
  provider text NOT NULL,
  payment_id text NOT NULL,
  paid_at timestamptz NOT NULL,
  amount bigint NOT NULL,
  currency text NOT NULL,
  amount_usd double precision,
  renewal boolean NOT NULL,
  visitor_id text,
  customer_id text,
  source text,
  channel text,
  referrer text,
  campaign text,
  landing_page text,
  PRIMARY KEY (site, provider, payment_id)
);
```

Run `npm install pg`, then run this with `DATABASE_URL` set:

```js,file:postgres.mjs
import pg from "pg";
import { receive } from "./receive.mjs";

const columns = [
  "provider", "payment_id", "paid_at", "amount", "currency", "amount_usd", "renewal",
  "visitor_id", "customer_id", "source", "channel", "referrer", "campaign", "landing_page",
];
const insert = `INSERT INTO virling_payments (site, ${columns.join(", ")})
  VALUES ($1, ${columns.map((_, i) => `$${i + 2}`).join(", ")}) ON CONFLICT DO NOTHING`;

const db = new pg.Pool({
  connectionString: process.env.DATABASE_URL,
  connectionTimeoutMillis: 2000,
  query_timeout: 2000,
});
db.on("error", console.error);

receive(async ({ type, site, data }) => {
  if (type !== "payment.attributed") return;
  await db.query(insert, [site, ...columns.map((c) => data[c])]);
});
```

The primary key makes an event sent twice insert one row.

## Delivery

Virling checks for new events once a minute and sends them oldest first. A request that fails or answers with a non-2xx status stops that round; the same event is tried again the next minute, and the error shows next to the webhook in Site settings until a send succeeds. An `alert.unusual_change` event is the exception: it is sent once and not tried again. Answer quickly: requests time out after 5 seconds. Saving a new URL rotates the signing secret.

## Troubleshooting

- **The signature does not match.** Compute the HMAC over the raw body as received, not over JSON your framework parsed and wrote again. Use the whole secret, including `whsec_`, and copy it again after you save a URL, since that rotates it. If only the age check fails, set your server's clock.
- **The test fails when you turn it on.** The `ping` is signed with a new secret you do not have yet, so answer it with a 2xx without checking it. The URL must be public and start with `https://`.
- **The same event comes again.** An answer that is not 2xx, or that takes longer than 5 seconds, counts as failed even if your code did the work. Each try has a new `created` and signature, so skip repeats by `provider` and `payment_id` for a payment, and by `visitor_id`, `goal` and `completed_at` for a goal.
- **Requests time out.** Virling waits 5 seconds. Give slow calls a shorter limit, as the recipes do, or store the event, answer, and do the work after.
- **No events arrive.** Webhooks stay off while the site holds sample data, although the `ping` still goes through: click **Remove sample data**. Otherwise look for an error next to the webhook in Site settings, since one failing event holds back the ones after it. A payment that stays unattributed is never sent.
