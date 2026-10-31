# Virling plugin for Claude

Ask Claude which channels, referrers, campaigns, pages and countries bring paying customers, using your own Virling server. Virling is open-source revenue analytics: it joins every payment to the visit before it.

## Install

```sh
claude plugin marketplace add virling/virling
claude plugin install virling@virling
```

The plugin connects to hosted Virling at https://app.virling.com. Claude opens the sign-in page, so no API key is needed. If you self-host, add your own server in Claude as a custom connector with the URL https://your-server/mcp instead.

## What it does

The plugin bundles a skill and the read-only Virling MCP server. Ask in plain words, for example "which channel made the most money last month" or "what did my last paying customer do before paying".

Docs: https://docs.virling.com/reference/mcp
Privacy: https://virling.com/privacy
License: AGPL-3.0
