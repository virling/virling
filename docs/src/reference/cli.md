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
| `--hosted`<br>`VIRLING_HOSTED` | off | Run as the hosted service, where Agency teams can add seats past the 3 included. Off, a team is capped at 3 users. |
| `--base-url <URL>`<br>`VIRLING_BASE_URL` | `http://localhost:8080` | Public URL of this server, used in sign-in links and script snippets. |
| `--secret-key <HEX>`<br>`VIRLING_SECRET_KEY` | unset | 64 hex characters used to encrypt provider keys. Unset: `secret.key` in the data folder. |
| `--trust-proxy`<br>`VIRLING_TRUST_PROXY` | off | Read the client IP from `X-Forwarded-For`. |
| `--events-per-second <N>`<br>`VIRLING_EVENTS_PER_SECOND` | `200` | Ingest limit per site. Events over it are dropped. |
| `--smtp-url <URL>`<br>`VIRLING_SMTP_URL` | unset | SMTP server for sign-in emails. Unset: emails are logged. |
| `--mail-from <ADDR>`<br>`VIRLING_MAIL_FROM` | `Virling <login@virling.com>` | Sender for sign-in emails. |
| `--openai-key <KEY>`<br>`VIRLING_OPENAI_KEY` | unset | OpenAI key for every site's [AI answers](../track/ai-answers.md). |
| `--anthropic-key <KEY>`<br>`VIRLING_ANTHROPIC_KEY` | unset | Anthropic key for AI answers. |
| `--perplexity-key <KEY>`<br>`VIRLING_PERPLEXITY_KEY` | unset | Perplexity key for AI answers. |
| `--gemini-key <KEY>`<br>`VIRLING_GEMINI_KEY` | unset | Google Gemini key for AI answers. |
| `--ai-answers-budget-usd <USD>`<br>`VIRLING_AI_ANSWERS_BUDGET_USD` | unset | Estimated monthly spend after which AI answer runs pause. |
| `--marketing`<br>`VIRLING_MARKETING` | off | Serve the virling.com landing pages and waitlist. Not needed when self-hosting. |
| `--waitlist-from <ADDR>`<br>`VIRLING_WAITLIST_FROM` | `Rahul at Virling <hello@virling.com>` | Sender for waitlist emails. Only used with `--marketing`. |
| `--dodo-api-key <KEY>`<br>`VIRLING_DODO_API_KEY` | unset | Bill the hosted plans through Dodo Payments. Needs `--hosted` and the other `--dodo-*` flags. Not needed when self-hosting. |
| `--dodo-api-url <URL>`<br>`VIRLING_DODO_API_URL` | `https://live.dodopayments.com` | Dodo API, or `https://test.dodopayments.com` for test mode. |
| `--dodo-webhook-secret <SECRET>`<br>`VIRLING_DODO_WEBHOOK_SECRET` | unset | Signing secret of the Dodo webhook at `/api/webhooks/dodo`. |
| `--dodo-personal-product <ID>`<br>`VIRLING_DODO_PERSONAL_PRODUCT` | unset | Dodo product for Personal. |
| `--dodo-agency-product <ID>`<br>`VIRLING_DODO_AGENCY_PRODUCT` | unset | Dodo product for Agency. |
| `--dodo-seat-addon <ID>`<br>`VIRLING_DODO_SEAT_ADDON` | unset | Dodo add-on for each Agency user past the 3 included. |
| `--dodo-personal-founding-product <ID>`<br>`VIRLING_DODO_PERSONAL_FOUNDING_PRODUCT` | unset | Dodo product for Personal at the founding price. |
| `--dodo-agency-founding-product <ID>`<br>`VIRLING_DODO_AGENCY_FOUNDING_PRODUCT` | unset | Dodo product for Agency at the founding price. |
| `--founders-url <URL>`<br>`VIRLING_FOUNDERS_URL` | `https://virling.com/api/founders` | Lookup of founding pre-orders by email. |
| `--founders-token <TOKEN>`<br>`VIRLING_FOUNDERS_TOKEN` | unset | Bearer token for the founders lookup. |

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

## query

Reads reports from a running Virling server through the [Read API](api.md): totals, revenue by channel, referrer, campaign or page, payments and visitor timelines. It runs anywhere, not only on the server, and does not touch the data folder.

```sh
virling query [OPTIONS] <COMMAND>
```

| Flag and env | Default | What it does |
| --- | --- | --- |
| `--url <URL>`<br>`VIRLING_URL` | `http://localhost:8080` | The Virling server. |
| `--key <KEY>`<br>`VIRLING_API_KEY` | | An API key from **Account settings > API keys** in the app. |
| `--site <DOMAIN>`<br>`VIRLING_SITE` | the only site | Which site to read. Needed when the account has more than one. |
| `--json` | off | Print the API response as JSON instead of a table. Use it from scripts and AI agents. |

Commands that read a range take `--range today|7d|30d|90d|12m|all`, or `--from YYYY-MM-DD --to YYYY-MM-DD` for exact local dates. Without them the range is the last 30 days, or since tracking started for a site tracked for 30 days or less.

| Command | What it prints |
| --- | --- |
| `sites` | The sites the key can read. |
| `overview` | Visitors, revenue, payments, conversion and revenue per visitor, with the previous period and the change. |
| `channels` | Revenue by channel. |
| `referrers` | Revenue by referring domain. |
| `campaigns` | Revenue by `utm_campaign`. |
| `landing-pages` | Revenue by the first page of each visit. |
| `pages` | Revenue by every page viewed. |
| `countries`, `devices` | Revenue by country and by device. |
| `breakdown <DIMENSION>` | Any dimension of the API, including `keyword`, `browser` and `os`. |
| `retention` | Customers acquired, still paying, net and average LTV, refund rate and 30-day churn by the first payment's channel. Takes `--by channel\|referrer\|campaign` and `--limit`. See [Retention by channel](../attribution/retention.md). |
| `payments` | Payments, newest first, with the source and campaign credited and the visitor ID. Takes `--filter all\|attributed\|unattributed\|refunded`, `--limit` (up to 200) and `--offset`. |
| `visitor <ID>` | One visitor's visits, pages, goals, exits and payments, oldest first. |

Breakdowns take `--limit` (default 20) and are sorted by revenue, then visitors. Every command has examples in `virling query <COMMAND> --help`.

```sh
$ export VIRLING_URL=https://stats.example.com VIRLING_API_KEY=vk_...
$ virling query channels --range 30d
example.com  2026-09-17 to 2026-10-16 (UTC)

Channel       Visitors  Payments    Revenue  Per visitor
Search           1,020        11    $539.00        $0.53
AI                 210         9    $441.00        $2.10
Unattributed         0         7    $343.00            -
Direct             610         4    $196.00        $0.32
```

Errors go to standard error with a non-zero exit code, such as a wrong key (`401`) or a site of another account (`404`).

## Not available yet

- Removing a site or a user from the CLI <span class="na">Not available yet</span>
