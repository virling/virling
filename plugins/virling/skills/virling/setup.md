# Setup

1. Add the plugin.
2. Sign in to Virling when Claude asks, and click **Allow**. No API key is needed.

The plugin connects to hosted Virling at https://app.virling.com. If you self-host, add your own server in Claude as a custom connector with the URL `https://your-server/mcp` instead; it needs a public `https` address for sign-in. The CLI and HTTP API are documented at https://docs.virling.com/reference/api.html.

## Paperclip

1. In Paperclip, add an MCP server connection with the URL `https://mcp.virling.com/mcp`, or `https://your-server/mcp` if you self-host.
2. Sign in with your Virling account when asked, or send an API key from **Account settings > API keys** as `Authorization: Bearer vk_...`.
3. Give the connection and this skill to the agents that report on growth, such as a marketing or CEO agent.
