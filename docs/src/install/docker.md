# Docker

The image is published to the GitHub Container Registry as `ghcr.io/virling/virling` on every release.

```sh
docker pull ghcr.io/virling/virling:latest
```

## Tags

| Tag | Example | Points to |
| --- | --- | --- |
| `X.Y.Z` | `VIRLING_VERSION` | One exact release |
| `X.Y` | `0.1` | The newest patch release of that minor version |
| `latest` | `latest` | The newest release |

Pin `X.Y.Z` in production so an upgrade only happens when you change the tag. Read the [changelog](https://github.com/virling/virling/blob/main/CHANGELOG.md) first: Virling is in early development and releases can include breaking changes.

The image is built for `linux/amd64` and `linux/arm64`. Docker pulls the right one for your server.

## What is in the image

| Setting | Value |
| --- | --- |
| Entrypoint | `virling` |
| Default command | `serve` |
| Data folder | `/data`, declared as a volume, set with `VIRLING_DATA=/data` |
| Listen address | `0.0.0.0:8080`, set with `VIRLING_ADDR`, port `8080` exposed |
| User | `virling`, uid `10001`, home `/data` |
| Base | `debian:bookworm-slim` with CA certificates |

Without arguments, the container runs `virling serve`, which only serves the tracking script and the event endpoint. Pass `serve --app` to also serve sign-in, the dashboard and payment sync:

```sh
docker run -d --name virling \
  -p 8080:8080 \
  -v virling-data:/data \
  -e VIRLING_BASE_URL=https://stats.example.com \
  ghcr.io/virling/virling:VIRLING_VERSION serve --app
```

> [!WARNING]
> **Mount a volume on /data.** Events, settings and `secret.key` live there. Without a volume they are lost when the container is removed.

If you bind-mount a host folder instead of a named volume, make it writable by uid `10001`:

```sh
sudo mkdir -p /srv/virling && sudo chown 10001 /srv/virling
docker run -d --name virling -p 8080:8080 -v /srv/virling:/data \
  ghcr.io/virling/virling:VIRLING_VERSION serve --app
```

## Run CLI commands

The image's entrypoint is the `virling` binary, so other commands run in the same container:

```sh
docker exec virling virling add-site yourdomain.com --owner you@example.com
docker exec virling virling --version
```

Next: [run it in production with Docker](../self-host/docker.md).
