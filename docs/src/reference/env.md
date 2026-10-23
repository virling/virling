# Environment variables

Every setting can be given as a flag or an environment variable. See [CLI](cli.md) for the flags.

| Variable | Default | What it does |
| --- | --- | --- |
| `VIRLING_DATA` | `./virling-data` | Folder for `virling.sqlite`, `events.duckdb` and `secret.key`. `/data` in the Docker image. |
| `VIRLING_ADDR` | `0.0.0.0:8080` | Address and port to listen on. |
| `VIRLING_APP` | `false` | Serve sign-in and the app, and run payment sync. |
| `VIRLING_HOSTED` | `false` | Run as the hosted service, where Agency teams can add seats past the 3 included. Off, a team is capped at 3 users. |
| `VIRLING_BASE_URL` | `http://localhost:8080` | Public URL, used in sign-in links and script snippets. No trailing slash needed. With `https://` and no path, such as `https://stats.example.com`, apps such as claude.ai can connect to the [MCP server](mcp.md) with OAuth sign-in. With a path, such as `https://example.com/virling`, sign-in is off and apps connect with an API key. |
| `VIRLING_SECRET_KEY` | unset | 64 hex characters that encrypt provider keys. Unset: `secret.key` in the data folder is used, and created if missing. |
| `VIRLING_TRUST_PROXY` | `false` | Read the client IP from `X-Forwarded-For`, and trust Cloudflare's `cf-*` headers from a proxy on this host or a private network, such as `cloudflared`. Only behind a proxy you control. |
| `VIRLING_EVENTS_PER_SECOND` | `200` | Events accepted per site per second. |
| `VIRLING_SMTP_URL` | unset | SMTP URL for sign-in emails, such as `smtps://user:pass@smtp.example.com:465`. Unset: emails are logged. |
| `VIRLING_MAIL_FROM` | `Virling <login@virling.com>` | Sender for sign-in emails. |
| `VIRLING_MARKETING` | `false` | Serve the virling.com landing pages and waitlist. |
| `VIRLING_WAITLIST_FROM` | `Rahul at Virling <hello@virling.com>` | Sender for waitlist emails. |
| `VIRLING_OPENAI_KEY` | unset | OpenAI key every site's [AI answers](../track/ai-answers.md) ask ChatGPT with. |
| `VIRLING_ANTHROPIC_KEY` | unset | Anthropic key for Claude in AI answers. |
| `VIRLING_PERPLEXITY_KEY` | unset | Perplexity key for AI answers. |
| `VIRLING_GEMINI_KEY` | unset | Google Gemini key for AI answers. |
| `VIRLING_AI_ANSWERS_BUDGET_USD` | unset | Estimated USD a month after which AI answer runs pause until the next month. Unset: no cap. |
| `RUST_LOG` | `info` | Log level and filter, such as `debug` or `virling=debug`. |

`VIRLING_MARKETING` and `VIRLING_WAITLIST_FROM` exist for virling.com itself. A self-hosted server does not need them.

## Minimal production set

```sh
VIRLING_DATA=/var/lib/virling
VIRLING_BASE_URL=https://stats.example.com
VIRLING_APP=true
VIRLING_TRUST_PROXY=true
VIRLING_SMTP_URL=smtps://user:pass@smtp.example.com:465
VIRLING_MAIL_FROM=Virling <stats@example.com>
```
