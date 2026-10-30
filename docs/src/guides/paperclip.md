# Use Virling with Paperclip

[Paperclip](https://github.com/paperclipai/paperclip) runs teams of AI agents toward business goals. Connect Virling so its agents can see which channels bring paying customers, and plan around revenue instead of traffic.

## Connect

1. In Paperclip, add an MCP server connection with the URL `https://mcp.virling.com/mcp`. If you self-host Virling, use `https://your-server/mcp`. See [MCP server](../reference/mcp.md).
2. Sign in with your Virling account when asked, or create a key in **Account settings > API keys** and send it as `Authorization: Bearer vk_...`. Both are read-only.
3. Install the Virling skill from [plugins/virling/skills/virling](https://github.com/virling/virling/tree/main/plugins/virling/skills/virling) and give it, with the connection, to the agents that report on growth.

## A weekly routine

Schedule a routine that asks an agent for a weekly channel review. The skill tells it to read the week's overview, revenue by channel, and retention, and to report the best channel, the biggest drop, the Unattributed share and one next step.
