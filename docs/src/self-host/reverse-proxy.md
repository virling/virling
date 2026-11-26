# Reverse proxy

Virling serves plain HTTP. Put a reverse proxy in front for HTTPS. Built-in TLS is <span class="na">Not available yet</span>.

## Three settings matter

| Setting | Why |
| --- | --- |
| `VIRLING_BASE_URL` | The public URL, such as `https://stats.example.com`. Used in sign-in links and the script snippets the app shows. Without a trailing slash. |
| `VIRLING_TRUST_PROXY=true` | Read the client IP from `X-Forwarded-For` instead of the TCP peer. See [Real client IPs](#real-client-ips). |
| Country header | Virling reads the visitor's country from `cf-ipcountry`, `x-vercel-ip-country` or `x-country-code`, in that order. Without one, the country comes from the browser's time zone, using a built-in table of IANA time zones. |
| Region and city | Read from `cf-region` and `cf-ipcity` (Cloudflare visitor location headers) or `x-vercel-ip-country-region` and `x-vercel-ip-city`. They appear only when the proxy sends them. |

Cloudflare's `cf-*` headers count only when the request really came through Cloudflare: the TCP peer is in [Cloudflare's IP ranges](https://www.cloudflare.com/ips/), or `VIRLING_TRUST_PROXY=true` is set and the peer is on this host or a private network, such as `cloudflared` or a proxy in front of a Docker container. Otherwise they are ignored, so nobody can fake a location by sending them.

## Real client IPs

Virling needs the visitor's IP to hash cookieless visitors apart. It never stores the IP. Behind a proxy, the TCP peer is the proxy, so every visit looks like it comes from one address unless you set `VIRLING_TRUST_PROXY=true`.

With it set, Virling takes the first address in `X-Forwarded-For`. The proxy that faces the internet must therefore replace that header with the address it saw, not append to whatever the browser sent, or anyone could pick their own IP. The configs below do that: Caddy and Traefik drop `X-Forwarded-For` from clients they do not trust by default, and the nginx config sets it to `$remote_addr`.

When Cloudflare is in front, Virling uses `CF-Connecting-IP` instead, under the rules above.

Pass the `Host` header through unchanged. All three proxies below do.

## Caddy

Caddy gets a certificate on its own, redirects HTTP to HTTPS and sets `X-Forwarded-For`, `X-Forwarded-Proto` and `Host`.

```caddy,file:/etc/caddy/Caddyfile
stats.example.com {
	reverse_proxy 127.0.0.1:8080
}
```

```sh
sudo systemctl reload caddy
```

To pass a country code, add it from a header your CDN sets, or from a Caddy GeoIP plugin if you use one, as `X-Country-Code`.

## nginx

Get a certificate first, for example with `certbot certonly --nginx -d stats.example.com`, then:

```nginx,file:/etc/nginx/sites-available/virling
server {
    listen 80;
    listen [::]:80;
    server_name stats.example.com;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl;
    listen [::]:443 ssl;
    http2 on;
    server_name stats.example.com;

    ssl_certificate     /etc/letsencrypt/live/stats.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/stats.example.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $remote_addr;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

```sh
sudo ln -s /etc/nginx/sites-available/virling /etc/nginx/sites-enabled/
sudo nginx -t && sudo systemctl reload nginx
```

If nginx itself sits behind another proxy or load balancer you control, use `$proxy_add_x_forwarded_for` instead of `$remote_addr`, and make sure the outer proxy replaces the header.

If the nginx GeoIP2 module is set up, add `proxy_set_header X-Country-Code $geoip2_country_code;` with your variable name.

## Traefik

This Compose file runs Traefik v3 and Virling together. Traefik gets a Let's Encrypt certificate and redirects HTTP to HTTPS. Virling publishes no port: only Traefik reaches it, over the Compose network.

```yaml,file:compose.yaml
services:
  traefik:
    image: traefik:v3
    restart: unless-stopped
    command:
      - --providers.docker=true
      - --providers.docker.exposedbydefault=false
      - --entrypoints.web.address=:80
      - --entrypoints.web.http.redirections.entrypoint.to=websecure
      - --entrypoints.web.http.redirections.entrypoint.scheme=https
      - --entrypoints.websecure.address=:443
      - --certificatesresolvers.le.acme.tlschallenge=true
      - --certificatesresolvers.le.acme.email=you@example.com
      - --certificatesresolvers.le.acme.storage=/letsencrypt/acme.json
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
      - letsencrypt:/letsencrypt

  virling:
    image: ghcr.io/virling/virling:VIRLING_VERSION
    command: serve --app
    restart: unless-stopped
    volumes:
      - virling-data:/data
    environment:
      VIRLING_BASE_URL: https://stats.example.com
      VIRLING_TRUST_PROXY: "true"
    labels:
      - traefik.enable=true
      - traefik.http.routers.virling.rule=Host(`stats.example.com`)
      - traefik.http.routers.virling.entrypoints=websecure
      - traefik.http.routers.virling.tls.certresolver=le
      - traefik.http.services.virling.loadbalancer.server.port=8080

volumes:
  letsencrypt:
  virling-data:
```

```sh
docker compose up -d
```

Add the SMTP variables from [Run with Docker](docker.md) for sign-in emails. If Traefik sits behind another proxy, list that proxy's addresses in `--entrypoints.websecure.forwardedheaders.trustedips` so Traefik keeps its `X-Forwarded-For`.

## Cloudflare

With the site proxied through Cloudflare, `cf-ipcountry` arrives on every request and the country works without extra setup. Cloudflare also sets `CF-Connecting-IP`, which Virling uses as the visitor's IP for the cookieless hash and never stores.

Region and city need one switch: in the Cloudflare dashboard, open **Rules**, then **Settings**, then **Managed Transforms**, and turn on **Add visitor location headers**. It is free on every plan. Without it, the City tab says there are no cities for these visits.

Behind a Cloudflare Tunnel, `cloudflared` connects from the same host, so set `VIRLING_TRUST_PROXY=true` for the `cf-*` headers to count.

## Serving the script from your own domain

Some ad blockers block third-party analytics hosts. You can serve the script and the event endpoint from your site's own domain by proxying two paths to Virling:

| Path on your site | Proxies to |
| --- | --- |
| `/v.js` | `https://stats.example.com/v.js` |
| `/api/event` | `https://stats.example.com/api/event` |

Then load the script from your domain. It sends events to `/api/event` on the host it was loaded from. To send them somewhere else, set `data-api` to the full URL. Make sure the proxy forwards the visitor's IP in `X-Forwarded-For`, the user agent and the country header.

A documented, tested proxy recipe for each platform is <span class="na">Not available yet</span>.

## Troubleshooting

| Symptom | Cause and fix |
| --- | --- |
| `502 Bad Gateway` | The proxy cannot reach Virling. Run `curl -fsS http://127.0.0.1:8080/health` on the proxy host; it should print `ok`. If the proxy runs in a container, `127.0.0.1` is the container itself: use the Compose service name, such as `virling:8080`. |
| Far fewer visitors than expected, all from one IP | `VIRLING_TRUST_PROXY` is not set, so every visit hashes from the proxy's address. Set it and restart. |
| Sign-in links point to `http://localhost:8080` | `VIRLING_BASE_URL` is not set. Set it to the public URL without a trailing slash. |
| Sign-in returns to the sign-in page | The app was opened over `http` while `VIRLING_BASE_URL` starts with `https`. The session cookie is then `Secure` and the browser drops it. Open the `https` URL. |
| `ERR_TOO_MANY_REDIRECTS` behind Cloudflare | Cloudflare's SSL mode is **Flexible**, so it talks HTTP to a proxy that redirects to HTTPS. Set it to **Full (strict)**. |
| Country comes from time zones, no cities | No country header reaches Virling. See [Cloudflare](#cloudflare) or the GeoIP notes above. |
| The proxy itself fails to start | Check its config: `caddy validate --config /etc/caddy/Caddyfile`, `sudo nginx -t`, or `docker compose logs traefik`. |
