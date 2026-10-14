# Reverse proxy

Virling serves plain HTTP. Put a reverse proxy in front for HTTPS. Built-in TLS is <span class="na">Not available yet</span>.

## Three settings matter

| Setting | Why |
| --- | --- |
| `VIRLING_BASE_URL` | The public URL, such as `https://stats.example.com`. Used in sign-in links and the script snippets the app shows. Without a trailing slash. |
| `VIRLING_TRUST_PROXY=true` | Read the client IP from the first address in `X-Forwarded-For` instead of the TCP peer. Needed for the cookieless hash to tell visitors apart. Set it only when a proxy you control sets that header. |
| Country header | Virling reads the visitor's country from `cf-ipcountry`, `x-vercel-ip-country` or `x-country-code`, in that order. Without one, country is empty. |

Country from a built-in GeoIP database, without a proxy header, is <span class="na">Not available yet</span>.

## Caddy

Caddy gets a certificate on its own and sets `X-Forwarded-For`.

```caddy,file:Caddyfile
stats.example.com {
	reverse_proxy 127.0.0.1:8080
}
```

To pass a country code, add it from a header your CDN sets, or from a Caddy GeoIP plugin if you use one, as `X-Country-Code`.

## nginx

```nginx,file:/etc/nginx/sites-available/virling
server {
    listen 443 ssl;
    http2 on;
    server_name stats.example.com;

    ssl_certificate     /etc/letsencrypt/live/stats.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/stats.example.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

If the nginx GeoIP2 module is set up, add `proxy_set_header X-Country-Code $geoip2_country_code;` with your variable name.

## Cloudflare

With the site proxied through Cloudflare, `cf-ipcountry` arrives on every request and the country works without extra setup. Cloudflare also sets `X-Forwarded-For`.

## Serving the script from your own domain

Some ad blockers block third-party analytics hosts. You can serve the script and the event endpoint from your site's own domain by proxying two paths to Virling:

| Path on your site | Proxies to |
| --- | --- |
| `/v.js` | `https://stats.example.com/v.js` |
| `/api/event` | `https://stats.example.com/api/event` |

Then load the script from your domain. It sends events to `/api/event` on the host it was loaded from. To send them somewhere else, set `data-api` to the full URL. Make sure the proxy forwards the visitor's IP in `X-Forwarded-For`, the user agent and the country header.

A documented, tested proxy recipe for each platform is <span class="na">Not available yet</span>.
