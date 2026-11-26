# Run with Docker

This page covers running the [Docker image](../install/docker.md) in production.

## docker run

```sh
docker run -d --name virling --restart unless-stopped \
  -p 127.0.0.1:8080:8080 \
  -v virling-data:/data \
  -e VIRLING_BASE_URL=https://stats.example.com \
  -e VIRLING_TRUST_PROXY=true \
  -e VIRLING_SMTP_URL='smtps://user:pass@smtp.example.com:465' \
  -e VIRLING_MAIL_FROM='Virling <stats@example.com>' \
  ghcr.io/virling/virling:VIRLING_VERSION serve --app
```

- Port `8080` is bound to localhost only, so it is reachable through your [reverse proxy](reverse-proxy.md) and not directly. Drop `127.0.0.1:` if nothing sits in front.
- `VIRLING_TRUST_PROXY=true` makes Virling read the client IP from `X-Forwarded-For`. Set it only behind a proxy.
- Without the SMTP variables, sign-in links are printed to `docker logs virling`.

## Docker Compose

The repo does not ship a Compose file yet <span class="na">Not available yet</span>. This example does the same as the command above:

```yaml,file:compose.yaml
services:
  virling:
    image: ghcr.io/virling/virling:VIRLING_VERSION
    command: serve --app
    restart: unless-stopped
    ports:
      - "127.0.0.1:8080:8080"
    volumes:
      - virling-data:/data
    environment:
      VIRLING_BASE_URL: https://stats.example.com
      VIRLING_TRUST_PROXY: "true"
      VIRLING_SMTP_URL: smtps://user:pass@smtp.example.com:465
      VIRLING_MAIL_FROM: Virling <stats@example.com>

volumes:
  virling-data:
```

```sh
docker compose up -d
docker compose exec virling virling add-site yourdomain.com --owner you@example.com
```

## Add sites and owners

```sh
docker exec virling virling add-site yourdomain.com --owner you@example.com --timezone Europe/Berlin
```

Signed-in users can also add sites in the app at `/app/setup`.

## Check that it works

1. **The server answers.** The [health check](#health-check) below prints `ok`, on the host and at your public address:

   ```sh
   curl -fsS https://stats.example.com/health
   ```

2. **You can sign in.** Open `https://stats.example.com/login` and enter the owner email. With SMTP the link arrives by email. Without it, read it from the log, open it and confirm:

   ```sh
   docker logs virling 2>&1 | grep -A4 "email not sent"
   ```

3. **The first pageview arrives.** Put the [script](../track/script.md) on your site and open a page in a browser. In the network tab, the request to `/api/event` is answered with `202`, and **Script installed** in the dashboard's setup checklist turns green. A request from `curl` does not count: it is treated as a bot. A `202` alone is not proof: events for a site you have not added are answered the same way and dropped.

Then [run a test visit and a test payment](../guides/test-your-setup.md).

## Health check

`GET /health` returns `200` with the body `ok`. The image has no `curl`, so check it from the host or your proxy:

```sh
curl -fsS http://127.0.0.1:8080/health
```

## Logs

Virling logs to stdout. Set `RUST_LOG` to change the level, for example `-e RUST_LOG=debug`. The default is `info`.

## Secret key

The key that encrypts provider keys is created as `/data/secret.key` on first start. To manage it yourself, pass `-e VIRLING_SECRET_KEY=<64 hex characters>` instead, for example from `openssl rand -hex 32`. See [Data, backups, upgrades](data.md).

## Upgrade

```sh
docker pull ghcr.io/virling/virling:VIRLING_VERSION
docker rm -f virling
# then the same docker run command with the new tag
```

Migrations run on start. Read the [changelog](https://github.com/virling/virling/blob/main/CHANGELOG.md) before upgrading.

## Common questions

### Why can I not sign in?

| What you see | Cause and fix |
| --- | --- |
| `/login` is a 404 | The container runs plain `serve`. Run `serve --app`. |
| No email and no link in the log | With SMTP set, look for "sending login email failed" in `docker logs virling`. Without SMTP, the link is logged at `info`, so a `RUST_LOG` of `warn` or `error` hides it. |
| "Check your email", but no new link | More than 5 links in an hour for one email, 5 in 10 minutes or 20 in an hour from one IP address. The log says "login link rate limit reached". Behind a proxy without `VIRLING_TRUST_PROXY=true`, everyone shares the proxy's address. |
| The link points to `http://localhost:8080` | `VIRLING_BASE_URL` is not set. |
| Confirming the link gives a 403, or returns to the sign-in page | The address in the browser does not match `VIRLING_BASE_URL`, for example `http` on one side and `https` on the other. Set it to the exact public address and recreate the container. |
| "This link has expired or was already used." | A link works once and for 15 minutes. Request a new one. |
| You are in, but your site is missing | Any email can sign in, and a new one gets its own empty account. Use the email from `--owner`. |

### Why are all visitors from one country?

Virling does not look up the country from the IP address. It takes the [country header](reverse-proxy.md#three-settings-matter) your proxy sends and, without one, the browser's time zone. One country for everyone means a header with the same value on every request: the proxy that sets it sees another proxy's address, or your own server forwards `/api/event` and the CDN in front of Virling reports where that server is. Set the header at the outermost proxy and pass it on, or remove it to use time zones.

Behind Cloudflare, `cf-ipcountry` is ignored unless the request comes from Cloudflare's addresses, or from a private address with `VIRLING_TRUST_PROXY=true`. With a proxy on the same host or network you need that setting. Without a header, a browser whose time zone is `UTC` has no country.

### How do I change the port?

Change the host side of `-p` and leave the container on `8080`. For port `9000` on the host, use `-p 127.0.0.1:9000:8080`, or `"127.0.0.1:9000:8080"` under `ports` in Compose. Recreate the container with `docker rm -f virling` and the new `docker run`, or `docker compose up -d`, then point your proxy at the new port. Change `VIRLING_BASE_URL` too if the port is part of your public address.

### How do I move to another server?

1. Back up the volume on the old server as in [Docker volume](data.md#docker-volume), without the last `docker start`, so the old container stays stopped.
2. Copy the archive to the new server and [restore it](data.md#restore-a-docker-volume) into a volume there.
3. Start the container with the same image tag, the same settings and, if you set one, the same `VIRLING_SECRET_KEY`.
4. Check `/health`, then point the DNS of `stats.example.com` at the new server.

If the address stays the same, the script tags, sign-in sessions, provider keys and webhooks keep working. Events sent between step 1 and the DNS change taking effect are lost. Payments that polling finds are picked up after the start.

If the address changes, set the new `VIRLING_BASE_URL` and update the script tag on your sites. Sign in again at the new address, and use **Replace key** for LemonSqueezy, Creem and RevenueCat, whose webhooks point at the old one.

### How do I import payments again?

In the site's settings at `/app/yourdomain.com/settings`, use **Replace key** on the provider and paste a key, which can be the same one. Virling starts over from the [backfill](../payments/index.md#replace-or-disconnect-a-key): 90 days for most providers, 60 for Shopify, and only one-time purchases for RevenueCat. Payments already stored are updated, not duplicated, and older ones stay. Restarting the container does not import again, it continues where the sync stopped.
