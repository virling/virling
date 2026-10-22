# MCP server

Virling serves a [Model Context Protocol](https://modelcontextprotocol.io) endpoint, so AI assistants such as Claude can read your reports and answer questions like "which channel made the most money last month?". It is read-only and uses the same API keys as the [Read API](api.md).

```text
https://stats.example.com/mcp
```

It is served when the app is on (`--app`). Use your server's public URL, as in `VIRLING_BASE_URL`. Assistants on the web, such as claude.ai, reach it from the internet, so it needs a public HTTPS address. Claude Code on your machine can also use a local one.

## Authentication

Create a key in **Account settings > API keys** in the app and send it on every request:

```http
Authorization: Bearer vk_...
```

Requests without a valid key get `401`. The key reads every site of your account.

## Claude Code

```sh
claude mcp add --transport http virling https://stats.example.com/mcp \
  --header "Authorization: Bearer vk_..."
```

Or in `.mcp.json`, with the key read from your environment:

```json
{
  "mcpServers": {
    "virling": {
      "type": "http",
      "url": "https://stats.example.com/mcp",
      "headers": { "Authorization": "Bearer ${VIRLING_API_KEY}" }
    }
  }
}
```

For Claude Code, the [CLI](cli.md) with `--json` works too, and uses less context per call.

## claude.ai

1. Open **Customize > Connectors** and click **Add custom connector**. On Team and Enterprise plans, an Owner adds it under **Organization settings > Connectors**.
2. Enter a name, such as Virling, and the URL `https://stats.example.com/mcp`.
3. For **Authentication**, choose **No sign-in**.
4. Under **Request headers**, pick `authorization` and enter `Bearer vk_...` as the value, with the word `Bearer` and a space before the key. Mark it **Required**.
5. Click **Add**, then turn the connector on in a chat from the **+** menu.

Request headers are in beta in claude.ai and not every account has them yet. Virling does not offer OAuth sign-in yet, so without request headers the connector cannot authenticate.

## Claude Code plugin

This repository is also a Claude Code plugin marketplace. The `virling` plugin bundles the [agent skill](https://github.com/virling/virling/tree/main/skills/virling) and the MCP server:

```sh
claude plugin marketplace add virling/virling
claude plugin install virling@virling
```

In a session, `/plugin marketplace add virling/virling` and `/plugin install virling@virling` do the same. Claude Code asks for your server's URL, such as `https://stats.example.com`. Then run `/mcp`, choose **virling** and sign in. The plugin uses OAuth sign-in, so the server needs `https`. On `http`, add the server with an API key as above.

## Other clients

Any client that speaks the Streamable HTTP transport and can send a header works. Point it at `/mcp` with the `Authorization` header above. Virling answers each request with one JSON response; it does not open event streams or keep sessions, and `GET /mcp` returns `405`.

## Tools

| Tool | What it returns | Arguments |
| --- | --- | --- |
| `list_sites` | The sites the key can read. | none |
| `get_overview` | Visitors, payments, revenue, conversion and revenue per visitor, the previous period and the change. | `site`, range |
| `get_breakdown` | Visitors and credited revenue per channel, referrer, campaign, keyword, landing page, page, country, device, browser or OS. | `site`, `dimension` (required), range, `limit` |
| `get_retention` | Customers acquired, still paying, net and average LTV, refund rate and 30-day churn per first-payment channel, referrer and campaign, with a headline. | `site`, range |
| `get_ai_answers` | Per assistant: answers that opened the site's pages, how often its answers to the site's buyer questions name the site, cited pages, and the visitors and revenue it sent, with each question's latest answer. | `site`, range |
| `list_payments` | Payments, newest first, with the credited visit and the visitor ID. | `site`, range, `filter`, `limit`, `offset` |
| `get_visitor_timeline` | One visitor's visits, pages, goals, exits and payments. | `site`, `visitor_id` (required) |

`site` is a domain from `list_sites`. It can be left out when the account has one site. A range is `range` (`today`, `7d`, `30d`, `90d`, `12m` or `all`) or `from` and `to` as `YYYY-MM-DD`. The results have the same fields as the [Read API](api.md) responses, as structured content and as JSON text.

Errors such as an unknown site come back as a tool result with `isError: true` and a message, so the assistant can correct itself.

## Not available yet

- OAuth sign-in for connectors <span class="na">Not available yet</span>
- Tools that change data <span class="na">Not available yet</span>
