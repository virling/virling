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
  ghcr.io/virling/virling:0.1.1 serve --app
```

- Port `8080` is bound to localhost only, so it is reachable through your [reverse proxy](reverse-proxy.md) and not directly. Drop `127.0.0.1:` if nothing sits in front.
- `VIRLING_TRUST_PROXY=true` makes Virling read the client IP from `X-Forwarded-For`. Set it only behind a proxy.
- Without the SMTP variables, sign-in links are printed to `docker logs virling`.

## Docker Compose

The repo does not ship a Compose file yet <span class="na">Not available yet</span>. This example does the same as the command above:

```yaml,file:compose.yaml
services:
  virling:
    image: ghcr.io/virling/virling:0.1.1
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
docker pull ghcr.io/virling/virling:0.1.1
docker rm -f virling
# then the same docker run command with the new tag
```

Migrations run on start. Read the [changelog](https://github.com/virling/virling/blob/main/CHANGELOG.md) before upgrading.
