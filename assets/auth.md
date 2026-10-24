# Virling auth.md

How an AI agent gets access to a person's Virling data.

Virling's API for agents is its MCP server at `https://mcp.virling.com/mcp`. It is read-only and sits behind OAuth 2.1 with PKCE. An agent never gets an account of its own: it acts for a person who already has one, and that person approves it.

## Authorization server

- Issuer: `https://beta.virling.com`
- Metadata: `https://beta.virling.com/.well-known/oauth-authorization-server`
- Protected resource metadata: `https://mcp.virling.com/.well-known/oauth-protected-resource`
- Scope: `read`
- Flow: authorization code with PKCE (`S256`), public clients (`token_endpoint_auth_method: none`)

## Registering a client

An agent can identify itself in either way. No credentials are issued ahead of time.

1. **Client ID Metadata Document.** Use an https URL that serves your client metadata as the `client_id`. The server fetches it.
2. **Dynamic client registration.** `POST https://beta.virling.com/oauth/register` with `client_name`, `redirect_uris` and `token_endpoint_auth_method: none`.

## Getting a token

1. Send the person to `https://beta.virling.com/oauth/authorize` with your `client_id`, `redirect_uri`, `code_challenge` and `scope=read`.
2. The person signs in with an email link, if they are not already, and approves the access on the consent screen.
3. Exchange the code at `https://beta.virling.com/oauth/token`.
4. Call the MCP server with `Authorization: Bearer <access_token>`.

The person can revoke any connected app under Account, Connected apps.

## Not offered

- Anonymous agent credentials and self-service agent signup.
- Write access. Every tool only reads.

Self-hosted servers use API keys from Account, API keys instead. See https://docs.virling.com/reference/mcp.html.

Skill for agents: https://virling.com/.well-known/agent-skills/virling/SKILL.md
