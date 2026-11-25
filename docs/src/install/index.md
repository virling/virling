# Install

Virling ships as a Docker image and as release binaries. You can also build it from source with Rust.

| Option | Platforms | Pick it when |
| --- | --- | --- |
| [Install script](binary.md#install-script) | Linux x86_64 and arm64, macOS on Intel and Apple silicon | You want one command. On Linux with systemd it also sets up the user, data folder and service. On macOS it only installs the binary |
| [Docker](docker.md) | linux/amd64, linux/arm64 | Your server already runs containers, or you want to pin and change versions by image tag |
| [Release binary](binary.md#download-and-verify) | Same as the install script | You manage the service yourself, use another supervisor, or want to verify the checksum by hand |
| [Build from source](source.md) | Anywhere Rust 1.99 builds | There is no release for your platform, or you want to change the code |

The quickest way to get the binary is `curl -fsSL https://virling.com/install.sh | sh`, see [Install script](binary.md#install-script).

Every option gives you the same `virling` binary. It runs `serve` by default and keeps its files in one [data folder](../self-host/data.md).

## Server size

One vCPU and 1 GB of RAM is enough for 100,000 pageviews a month. Measured on 2026-10-31 with `scripts/load-1cpu.sh`, in a container limited to 1 vCPU and 1 GB on a GitHub Actions runner:

| Test | Result |
| --- | --- |
| 100,000 pageviews sent to `/api/event`, 16 at a time | 2,829 a second, none failed, p50 4 ms, p99 34 ms |
| Memory after the ingest | 157 MB |
| Every dashboard route on the demo data, no result cache | p50 0.6 to 7.6 ms, p95 up to 64 ms on long date ranges |

Run the script on your own server to measure it there.

There is no release build for Windows <span class="na">Not available yet</span>. On Windows, use [Docker](docker.md) or [build from source](source.md).

## After installing

Do these in order. Each links to the page with the details.

| Step | What to do | Details |
| --- | --- | --- |
| 1. Start it | Run `virling serve --app`. Without `--app` there is no sign-in, dashboard or payment sync. The service from the install script already runs it. | [Docker](../self-host/docker.md), [binary](../self-host/binary.md), [systemd](../self-host/systemd.md) |
| 2. Set the domain | Point a subdomain such as `stats.example.com` at the server and set `VIRLING_BASE_URL` to it, with `https://` and no trailing slash. Sign-in links use it. | [Environment variables](../reference/env.md) |
| 3. Add a reverse proxy | Virling serves plain HTTP. Put Caddy, nginx or Traefik in front for HTTPS, and set `VIRLING_TRUST_PROXY=true`. The install script listens on every address, so set `VIRLING_ADDR=127.0.0.1:8080` too when the proxy is on the same server. | [Reverse proxy](../self-host/reverse-proxy.md) |
| 4. Create the first account | Add your site with its owner's email, using the data folder the server uses. The account is created if it does not exist. | [CLI](../reference/cli.md#add-site) |
| 5. Sign in | Open `/login` and enter that email. Without SMTP, the link is in the server log. | [Sign-in email](../self-host/smtp.md) |
| 6. Set up backups | Back up the whole data folder, with `secret.key`. Try a restore once. | [Data, backups, upgrades](../self-host/data.md#backups) |
| 7. Add the script and payments | Put the tag on your site, connect a provider and make a test payment. | [Quickstart](../quickstart.md), [Test your setup](../guides/test-your-setup.md) |

For step 4, pick the command for your install:

```sh
# Install script or systemd
sudo -u virling env VIRLING_DATA=/var/lib/virling \
  virling add-site yourdomain.com --owner you@example.com

# Docker
docker exec virling virling add-site yourdomain.com --owner you@example.com
```

To check the server before you point a domain at it, run `curl -fsS http://127.0.0.1:8080/health` on it. It prints `ok`.

Outgoing connections, such as the extension DuckDB downloads on the first start, are listed in [Outbound connections](../reference/outbound.md).
