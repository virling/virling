# Virling plugin for Claude

Ask Claude which channels, referrers, campaigns, pages and countries bring paying customers, using your own Virling server. Virling is open-source revenue analytics: it joins every payment to the visit before it.

## Install

```sh
claude plugin marketplace add virling/virling
claude plugin install virling@virling
```

Claude asks for your Virling server URL. Hosted users enter https://beta.virling.com. Self-hosters enter their own HTTPS address. Claude then opens the sign-in page, so no API key is needed.

## What it does

The plugin bundles a skill and the read-only Virling MCP server. Ask in plain words, for example "which channel made the most money last month" or "what did my last paying customer do before paying".

Docs: https://docs.virling.com/reference/mcp.html
Privacy: https://virling.com/privacy
License: AGPL-3.0
