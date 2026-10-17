# Quickstart

Run Virling on your own server, add the tracking script to your site and connect a payment provider. From then on, every new payment shows the visit that brought it.

<div class="meta"><span class="chip"><i></i>Self-host</span><span class="chip">6 steps</span><span class="chip mono" style="font-size:12px">v0.1.1</span></div>

> [!NOTE]
> **Hosted plan coming soon.** app.virling.com is not open yet, so this guide covers self-hosting. Steps 4 to 6 will be the same on the hosted plan.

You need a Linux or macOS server with Docker or one of the release binaries, and a domain pointing at it. Virling listens on port `8080` and keeps everything in one data folder: SQLite for settings, DuckDB for events.

<div data-steps></div>

## Start the server

Pick one way to run it. Each starts the server with the app turned on, which serves sign-in, the dashboard and payment sync.

<div class="tabset" data-tabs="Docker|Binary|systemd" data-key="run" aria-label="How to run"></div>

```sh
docker run -d --name virling \
  -p 8080:8080 \
  -v virling-data:/data \
  -e VIRLING_BASE_URL=https://stats.example.com \
  ghcr.io/virling/virling:latest serve --app
```

```sh
curl -fsSL https://virling.com/install.sh | sh
VIRLING_BASE_URL=https://stats.example.com virling serve --app
```

<div class="cb-foot">The script picks the build for Linux x86_64 or arm64, or macOS on Intel or Apple silicon, and checks its checksum.</div>

```ini
[Unit]
Description=Virling
After=network-online.target
Wants=network-online.target

[Service]
User=virling
Environment=VIRLING_DATA=/var/lib/virling
Environment=VIRLING_BASE_URL=https://stats.example.com
ExecStart=/usr/local/bin/virling serve --app
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

<div class="cb-foot"><span class="tbd">EXAMPLE</span><span>The repo does not ship a unit file yet. Save this as <code>/etc/systemd/system/virling.service</code>, then run <code>sudo systemctl enable --now virling</code>.</span></div>

Check that it is up: `curl http://localhost:8080/health` prints `ok`.

> [!WARNING]
> **Keep the secret key.** Payment keys are encrypted with `secret.key` in the data folder, or with `VIRLING_SECRET_KEY` if you set it. Lose it and you have to reconnect every provider.

## Add your site

Create the site and its owner from the command line. The owner account is created if it does not exist. You can also add sites after signing in.

```sh
$ virling add-site yourdomain.com \
    --owner you@example.com --timezone Europe/Berlin
Added yourdomain.com. Script tag:
<script defer src="https://YOUR-VIRLING-HOST/v.js" data-site="yourdomain.com"></script>
```

With Docker, run it inside the container: `docker exec virling virling add-site yourdomain.com --owner you@example.com`. The timezone decides where your report days start and defaults to `UTC`.

## Sign in

Open `https://stats.example.com/login` and enter the owner email. Without an SMTP server, the sign-in link is printed to the server log instead of being emailed. With Docker, read it with `docker logs virling`.

> [!NOTE]
> **Send real emails.** Set `VIRLING_SMTP_URL`, for example `smtps://user:pass@smtp.example.com:465`, and `VIRLING_MAIL_FROM` for the sender. See [Sign-in email](self-host/smtp.md).

## Add the script

Paste this before `</head>` on every page. It loads with `defer` and never blocks rendering.

```html,file:index.html
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

Attributes the script reads:

| Attribute | What it does |
| --- | --- |
| `data-site` <span class="req">Required</span> | The domain you added in step 2. Pages on its subdomains count toward it. |
| `data-api` <span class="opt">Optional</span> | Send events to this URL instead of `/api/event` on the script host. |
| `data-cookieless` <span class="opt">Optional</span> | Never set the `virling_vid` cookie. Visitors are counted with a daily hash, and payments show as Unattributed. |
| `data-respect-gpc` <span class="opt">Optional</span> | Send nothing when the browser has Global Privacy Control turned on. |
| `data-dev` <span class="opt">Optional</span> | Also track on localhost. Off by default, so local testing stays out of your reports. |

Track a goal from your code with `virling("goal", "signup")`. Route changes in single-page apps are tracked on their own. See [Framework guides](track/frameworks.md) for Next.js, Astro, WordPress, Framer and Webflow.

## Connect payments

In the app, open your site's setup, then Connect payments, and paste a read-only key. Virling imports the last 90 days, converts amounts to USD and checks for new payments every minute.

<div class="prov"><a class="prow" href="payments/stripe.html"><span class="plogo"><svg viewBox="0 0 24 24" aria-hidden="true"><path fill="#635BFF" d="M13.976 9.15c-2.172-.806-3.356-1.426-3.356-2.409 0-.831.683-1.305 1.901-1.305 2.227 0 4.515.858 6.09 1.631l.89-5.494C18.252.975 15.697 0 12.165 0 9.667 0 7.589.654 6.104 1.872 4.56 3.147 3.757 4.992 3.757 7.218c0 4.039 2.467 5.76 6.476 7.219 2.585.92 3.445 1.574 3.445 2.583 0 .98-.84 1.545-2.354 1.545-1.875 0-4.965-.921-6.99-2.109l-.9 5.555C5.175 22.99 8.385 24 11.714 24c2.641 0 4.843-.624 6.328-1.813 1.664-1.305 2.525-3.236 2.525-5.732 0-4.128-2.524-5.851-6.594-7.305h.003z"/></svg></span><span class="pn-col"><span class="pn-name">Stripe</span><span class="k">Restricted key, rk_live_...</span></span><span class="pdesc">Read on Charges, Checkout Sessions, Customers, Events, Invoices, Subscriptions and PaymentIntents. Secret keys (sk_) are refused.</span><svg class="chev" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 6l6 6-6 6"/></svg></a><a class="prow" href="payments/dodo.html"><span class="plogo"><svg viewBox="0 0 30 30" fill="none" aria-hidden="true"><rect x="0.3" y="0.3" width="29.4" height="29.4" rx="14.7" fill="#C6FE1E"/><rect x="0.3" y="0.3" width="29.4" height="29.4" rx="14.7" stroke="#B3E910" stroke-width="0.6"/><mask id="mask0_845_82475" style="mask-type:luminance" maskUnits="userSpaceOnUse" x="5" y="8" width="20" height="15"><path d="M24.0038 8H5.99805V23H24.0038V8Z" fill="white"/></mask><g mask="url(#mask0_845_82475)"><path d="M12.9679 11.4606H12.9599C12.4637 11.3182 11.9387 11.603 11.7707 12.0734C11.585 12.579 11.8795 13.1582 12.3997 13.3182C13.6737 13.6798 14.2211 11.8542 12.9679 11.4606Z" fill="#00160D"/><path d="M23.8265 14.0425C22.1859 10.4505 16.9523 11.9369 16.4689 10.9961C15.0525 8.77368 12.4084 7.52568 9.54993 8.16888C9.10979 7.99288 7.59411 7.94488 6.6146 8.56088L7.20518 8.82168C7.25 8.84088 7.23719 8.83608 7.30281 8.86008C7.5701 8.96088 7.52368 8.93208 7.33162 9.03768C6.89148 9.29528 6.34251 9.59448 6 10.0441C6.0144 10.0649 6.75384 10.2425 6.75384 10.2425C6.76664 10.2457 6.90429 10.2601 6.87388 10.3209C4.45551 14.1385 8.5336 19.9001 10.5134 23.0009H16.2449C15.3598 21.4169 14.3483 19.2553 14.694 17.7881C14.7564 17.5225 14.8364 17.1849 15.1629 17.1401C15.952 17.0137 17.0083 17.0249 17.7605 16.9401C17.7605 16.9401 17.7643 16.9401 17.7717 16.9401C17.9318 16.9321 21.8498 16.4345 22.7877 18.8761C22.8678 19.1001 23.0518 18.9545 23.1254 18.8073C23.8249 17.4169 24.2762 15.2281 23.8297 14.0441L23.8265 14.0425ZM17.5013 12.8633C17.226 13.3545 17.0387 13.9929 16.9907 14.5497C16.9651 14.9033 17.0019 15.2521 17.0371 15.6057C17.0563 15.8009 17.0579 16.0473 16.9011 16.1753C16.765 16.2905 16.5393 16.3017 16.3121 16.3177C15.1965 16.3129 12.4757 16.3177 11.4273 15.6041L11.4209 15.5993C9.89564 14.6697 9.07938 12.5705 9.98527 10.9401C10.2782 10.3865 10.8175 10.0265 11.4321 9.89048C12.2228 9.70968 13.0935 9.84408 13.7993 10.2137C14.0874 10.3593 14.4315 10.5513 14.694 10.7721C15.2141 11.2265 15.6591 11.6985 16.3409 11.8441C16.6546 11.9337 16.9811 11.9209 17.2948 11.9849C17.8822 12.1257 17.7349 12.4601 17.4997 12.8601L17.5013 12.8633Z" fill="#00160D"/></g></svg></span><span class="pn-col"><span class="pn-name">Dodo Payments</span><span class="k">API key, live or test mode</span></span><span class="pdesc">Only reads payments, refunds and subscriptions. Live and test mode keys both work.</span><svg class="chev" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 6l6 6-6 6"/></svg></a><a class="prow" href="payments/polar.html"><span class="plogo"><svg viewBox="0 0 1510 1510" fill="none" aria-hidden="true"><mask id="mask0_545_3" style="mask-type:alpha" maskUnits="userSpaceOnUse" x="0" y="0" width="1510" height="1510"><circle cx="755" cy="755" r="755" transform="matrix(-1 0 0 1 1510 0)" fill="white"/></mask><g mask="url(#mask0_545_3)"><path d="M679.51 -81.6641V571.332L232.436 124.258L123.483 233.21L570.558 680.285H-449.15V834.366H570.515L159.337 1245.55L268.291 1354.5L679.51 943.277V1553.91H833.592V-81.6641H679.51Z" fill="currentColor"/><path d="M1189.51 757.326C1189.51 550.283 1290.75 361.31 1444.36 246.645L1342.97 110.824C1147.75 256.556 1020.02 495.271 1020.02 757.326C1020.02 985.231 1118.44 1184.81 1268.48 1329L1370.05 1432.88L1491.24 1314.39L1387.8 1208.6L1386.79 1207.63C1265.9 1091.86 1189.51 934.764 1189.51 757.326Z" fill="currentColor"/></g></svg></span><span class="pn-col"><span class="pn-name">Polar</span><span class="k">Organization token, polar_oat_...</span></span><span class="pdesc">Scopes orders:read and refunds:read, plus subscriptions:read for retention.</span><svg class="chev" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 6l6 6-6 6"/></svg></a><a class="prow" href="payments/lemonsqueezy.html"><span class="plogo"><svg viewBox="0 0 24 24" aria-hidden="true"><path fill="#FFC233" d="m7.4916 10.835 2.3748-6.5114a3.1497 3.1497 0 0 0-.065-2.3418C9.0315.183 6.9427-.398 5.2928.265 3.643.929 2.71 2.4348 3.512 4.3046l2.8197 6.5615c.219.509.97.489 1.16-.03m1.6798 1.0969 6.5334-2.7758c2.1699-.9219 2.7218-3.6907 1.022-5.2905l-.068-.063c-1.6669-1.5469-4.4217-1.002-5.3706 1.0359L8.3566 11.135c-.234.503.295 1.0199.8159.7979m.373.87 6.6454-2.5119c2.2078-.8349 4.6206.745 4.5886 3.0398l-.002.09c-.048 2.2358-2.3938 3.7376-4.5536 2.9467l-6.6724-2.4418a.595.595 0 0 1-.006-1.1229m-.386 1.9269 6.4375 2.9767a3.2997 3.2997 0 0 1 1.6658 1.6989c.769 1.7998-.283 3.6396-1.9328 4.3016-1.6499.662-3.4097.235-4.2097-1.6359l-2.8027-6.5694c-.217-.509.328-1.009.8419-.772"/></svg></span><span class="pn-col"><span class="pn-name">LemonSqueezy</span><span class="k">API key</span></span><span class="pdesc">Cannot be made read-only. Payments import, but checkout attribution waits on webhooks.</span><svg class="chev" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 6l6 6-6 6"/></svg></a></div>

## Link checkout to visits

Pass the visitor ID into checkout so each payment is credited to the visit that brought it. On your server, read the `virling_vid` cookie. In the browser, `window.virling?.vid?.()` returns the same ID.

```js,file:server.js
// Stripe Checkout (Node): pass the visitor ID on the session
const session = await stripe.checkout.sessions.create({
  client_reference_id: req.cookies.virling_vid,
  line_items: [{ price: "price_...", quantity: 1 }],
  mode: "subscription",
  success_url: "https://yourdomain.com/thanks",
});
```

> [!NOTE]
> **Payment links need nothing.** Stripe Payment Links, Polar checkout links and Dodo payment links are tagged by the script when a visitor clicks them. Payments without a visitor ID still count, as Unattributed.

See [Link checkout to visits](payments/checkout.md) for Dodo Payments, Polar and LemonSqueezy.

<div data-steps-end></div>

## Next steps

<div class="nx"><a href="attribution/index.html">How attribution works<span>The credited visit, models and the lookback window</span></a><a href="attribution/channels.html">Channels<span>How a visit becomes Paid, Email, AI, Social and more</span></a><a href="self-host/reverse-proxy.html">Reverse proxy<span>HTTPS, X-Forwarded-For and country headers</span></a><a href="reference/env.html">Environment variables<span>Every VIRLING_ setting with its default</span></a></div>
