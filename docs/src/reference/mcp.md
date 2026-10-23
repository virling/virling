# MCP server

Virling serves a [Model Context Protocol](https://modelcontextprotocol.io) endpoint, so AI assistants such as Claude can read your reports and answer questions like "which channel made the most money last month?". It is read-only. Apps sign in with OAuth, or use the same API keys as the [Read API](api.md).

```text
https://stats.example.com/mcp
```

It is served when the app is on (`--app`). Use your server's public URL, as in `VIRLING_BASE_URL`. Assistants on the web, such as claude.ai, reach it from the internet, so it needs a public HTTPS address. Claude Code on your machine can also use a local one.

## Authentication

There are two ways to sign in. Both read every site of your account and can't change anything.

**OAuth sign-in.** Apps such as claude.ai and ChatGPT sign in with your Virling account: you add the URL, the app opens Virling, and you click **Allow**. It needs `VIRLING_BASE_URL` to be a public `https://` URL; on `http://` sign-in is off and only API keys work. Connected apps are listed in **Account settings > Connected apps**, where you can revoke them. Only the account owner can connect an app.

**API key.** Create a key in **Account settings > API keys** in the app and send it on every request:

```http
Authorization: Bearer vk_...
```

Requests without a valid key or token get `401` with a `WWW-Authenticate` header that points at the sign-in metadata.

### For client developers

Virling follows the [MCP authorization spec](https://modelcontextprotocol.io/specification/latest/basic/authorization) and is its own authorization server.

- Protected resource metadata (RFC 9728) at `/.well-known/oauth-protected-resource/mcp` and `/.well-known/oauth-protected-resource`. The resource is `https://stats.example.com/mcp`.
- Authorization server metadata (RFC 8414) at `/.well-known/oauth-authorization-server`.
- Dynamic client registration (RFC 7591) at `/oauth/register`. Clients are public: no secret, `token_endpoint_auth_method` is `none`.
- Client ID Metadata Documents: a `client_id` that is an `https` URL is read from that URL when the app signs in. The document must name the same `client_id`, a `client_name` and its `redirect_uris`, and be at most 5 KB. Virling fetches it only from public addresses, does not accept a redirect, waits at most 3 seconds and keeps it for its `Cache-Control` `max-age`, an hour by default and a day at most.
- Authorization code flow with PKCE, `S256` only, at `/oauth/authorize` and `/oauth/token`. Redirect URIs must be `https`, or `http` on `localhost`, and must match a registered one exactly; a loopback URI may use any port. The response carries `iss` (RFC 9207).
- The `resource` parameter (RFC 8707), when sent, must be the MCP URL. Tokens only work on `/mcp`, not on the Read API.
- One scope, `read`. Access tokens last 1 hour. Refresh tokens last 30 days and rotate on every use; reusing an old one revokes the sign-in.
- Tokens and codes are stored hashed, like API keys.

## Claude Code

On an `https` server, add the URL and run `/mcp` in a session to sign in:

```sh
claude mcp add --transport http virling https://stats.example.com/mcp
```

Or send an API key, which also works on a local `http` server:

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
2. Enter a name, such as Virling, and the URL `https://stats.example.com/mcp`. Leave the advanced settings empty.
3. Click **Add**, then **Connect**. Virling opens: sign in if asked, then click **Allow**.
4. Turn the connector on in a chat from the **+** menu.

On a server without `https`, use an API key instead: choose **No sign-in** and, under **Request headers**, add `authorization` with the value `Bearer vk_...`.

## ChatGPT

With developer mode on, create a custom app with the URL `https://stats.example.com/mcp` and OAuth authentication. ChatGPT opens Virling: sign in if asked, then click **Allow**.

## Claude Code plugin

This repository is also a Claude Code plugin marketplace. The `virling` plugin bundles the [agent skill](https://github.com/virling/virling/tree/main/skills/virling) and the MCP server:

```sh
claude plugin marketplace add virling/virling
claude plugin install virling@virling
```

In a session, `/plugin marketplace add virling/virling` and `/plugin install virling@virling` do the same. Claude Code asks for your server's URL, such as `https://stats.example.com`. Then run `/mcp`, choose **virling** and sign in. The plugin uses OAuth sign-in, so the server needs `https`. On `http`, add the server with an API key as above.

## Other clients

Any client that speaks the Streamable HTTP transport and can send a header works. Point it at `/mcp` with the `Authorization` header above. Virling answers each request with one JSON response; it does not open event streams or keep sessions, and `GET /mcp` returns `405`. It speaks MCP 2026-07-28, which has no `initialize` and starts with `server/discover`, and the earlier revisions from 2025-03-26 to 2025-11-25.

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

- Tools that change data <span class="na">Not available yet</span>
