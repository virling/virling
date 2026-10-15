# CLI

```sh
virling [--data-dir <DIR>] [COMMAND]
```

| Option and env | Default | What it does |
| --- | --- | --- |
| `--data-dir <DIR>`<br>`VIRLING_DATA` | `./virling-data` | Folder for the SQLite and DuckDB files. Works before or after the command. |
| `-h`, `--help` | | Print help. |
| `-V`, `--version` | | Print the version. |

Without a command, Virling runs `serve` with its defaults and environment variables. To pass flags, name the command: `virling serve --app`.

## serve

Runs the server. This is the default command.

```sh
virling serve [OPTIONS]
```

| Flag and env | Default | What it does |
| --- | --- | --- |
| `--addr <ADDR>`<br>`VIRLING_ADDR` | `0.0.0.0:8080` | Address and port to listen on. |
| `--app`<br>`VIRLING_APP` | off | Serve sign-in and the app, and run payment sync. |
| `--base-url <URL>`<br>`VIRLING_BASE_URL` | `http://localhost:8080` | Public URL of this server, used in sign-in links and script snippets. |
| `--secret-key <HEX>`<br>`VIRLING_SECRET_KEY` | unset | 64 hex characters used to encrypt provider keys. Unset: `secret.key` in the data folder. |
| `--trust-proxy`<br>`VIRLING_TRUST_PROXY` | off | Read the client IP from `X-Forwarded-For`. |
| `--events-per-second <N>`<br>`VIRLING_EVENTS_PER_SECOND` | `200` | Ingest limit per site. Events over it are dropped. |
| `--smtp-url <URL>`<br>`VIRLING_SMTP_URL` | unset | SMTP server for sign-in emails. Unset: emails are logged. |
| `--mail-from <ADDR>`<br>`VIRLING_MAIL_FROM` | `Virling <login@virling.com>` | Sender for sign-in emails. |
| `--marketing`<br>`VIRLING_MARKETING` | off | Serve the virling.com landing pages and waitlist. Not needed when self-hosting. |
| `--waitlist-from <ADDR>`<br>`VIRLING_WAITLIST_FROM` | `Rahul at Virling <hello@virling.com>` | Sender for waitlist emails. Only used with `--marketing`. |

A flag on the command line wins over its environment variable. Boolean variables take `true` or `false`.

## add-site

Adds a site to track, and its owner account if it does not exist.

```sh
virling add-site <DOMAIN> --owner <EMAIL> [--timezone <TZ>]
```

| Argument | Default | What it does |
| --- | --- | --- |
| `<DOMAIN>` | | The site's domain. `https://`, a path and a leading `www.` are stripped, so `https://www.example.com/pricing` becomes `example.com`. |
| `--owner <EMAIL>` | | Owner email, lowercased. The account is created if it does not exist. Sign in with this address. |
| `--timezone <TZ>` | `UTC` | IANA timezone such as `Europe/Berlin`. Report days start at midnight in it. The CLI does not check the name, so spell it exactly. |

```sh
$ virling add-site example.com --owner you@example.com
Added example.com. Script tag:
<script defer src="https://YOUR-VIRLING-HOST/v.js" data-site="example.com"></script>
```

It fails if the domain is not valid, the email is not valid, or the site already exists. It works while the server is running. New sites are picked up within a minute.

## Not available yet

- Removing a site or a user from the CLI <span class="na">Not available yet</span>
- Listing sites from the CLI <span class="na">Not available yet</span>
