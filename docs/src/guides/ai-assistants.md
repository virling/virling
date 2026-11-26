# Ask your AI assistant which channel pays

Once payments are linked to visits, you can ask Claude, ChatGPT or another MCP client questions such as "which channel brought paying customers last month?" and get answers from your own numbers. This guide connects an assistant to Virling's [MCP server](../reference/mcp.md) and gives you questions to start with. It takes about five minutes.

The server only reads. It cannot add or change sites, goals, keys or payments, and it never returns names, emails or IP addresses.

## What you need

- A Virling account with at least one site. To start, [Start free](https://app.virling.com/login) on the hosted plan, or [self-host it](../quickstart.md).
- Payments connected and linked to visits, so answers are about revenue and not only traffic. See [Find which channel pays](channel-revenue.md). To try the questions before any traffic arrives, open a new site and choose **Try with sample data**.
- The MCP URL. For hosted Virling it is `https://mcp.virling.com/mcp`. For a self-hosted server it is `/mcp` on your `VIRLING_BASE_URL`, such as `https://stats.example.com/mcp`, and needs the app on (`--app`).

## 1. Choose how to sign in

Both ways read every site of your account and can't change anything.

- **OAuth sign-in.** You add the URL in your assistant, it opens Virling, and you click **Allow**. This is what claude.ai and ChatGPT use. A self-hosted server needs `VIRLING_BASE_URL` to be a public `https://` URL at the root of its host. Only the account owner can connect an app, and connected apps are listed in **Account settings > Connected apps**, where you can revoke them.
- **API key.** Create a key in **Account settings > API keys** and send it as `Authorization: Bearer vk_...`. Use it for Claude Code, scripts, other clients, or a server on plain `http`.

## 2. Add the connector

The examples use `https://stats.example.com/mcp`. Replace it with your MCP URL.

### claude.ai

For hosted Virling, open the [Claude directory listing](https://claude.ai/directory/virling), click **Connect**, and click **Allow** when Virling opens. For a self-hosted server, add a custom connector:

1. Open **Customize > Connectors** and click **Add custom connector**. On Team and Enterprise plans, an Owner adds it under **Organization settings > Connectors**.
2. Enter a name, such as Virling, and the URL `https://stats.example.com/mcp`. Leave the advanced settings empty.
3. Click **Add**, then **Connect**. Virling opens: sign in if asked, then click **Allow**.
4. Turn the connector on in a chat from the **+** menu.

On a server without `https`, choose **No sign-in** and, under **Request headers**, add `authorization` with the value `Bearer vk_...`.

### ChatGPT

With developer mode on, create a custom app with the URL `https://stats.example.com/mcp` and OAuth authentication. ChatGPT opens Virling: sign in if asked, then click **Allow**.

### Claude Code

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

For hosted Virling, the `virling` plugin bundles the MCP server and the agent skill. Install it with `claude plugin marketplace add virling/virling` and `claude plugin install virling@virling`, then run `/mcp`, choose **virling** and sign in. See [Claude Code plugin](../reference/mcp.md#claude-code-plugin).

### Other clients

Any client that speaks the Streamable HTTP transport and can send a header works. Point it at the MCP URL with the `Authorization: Bearer vk_...` header. See [Other clients](../reference/mcp.md#other-clients).

## 3. Ask

Start with "Which sites can you read?" to check the connection. Then ask in plain words, and name a range and, when your account has more than one site, the site. The tools take today, the last 7, 30 or 90 days, the last 12 months, all time, or two dates.

## Example questions

Each one is answered by the tools in brackets. More are on the [AI revenue questions](https://virling.com/tools/ai-revenue-questions) page.

**SaaS founder**

1. Which channel brought paying customers last month, and how does that compare with the month before? (`get_channel_verdicts`)
2. Which channel brings the most revenue per visitor over the last 90 days? (`get_breakdown`)
3. Which channel's customers have the highest 30-day churn? (`get_retention`)
4. Show me the path of the last 5 customers who paid, from first visit to payment. (`list_payments`, `get_visitor_timeline`)

**Store owner**

5. How did visitors, revenue and conversion in the last 7 days compare with the 7 days before? (`get_overview`)
6. Which countries brought the most revenue in the last 12 months? (`get_breakdown`)
7. Which landing pages got visitors but no revenue in the last 30 days? (`get_breakdown`)

**Agency**

8. List the sites I can read and give me revenue for each over the last 30 days. (`list_sites`, `get_overview`)
9. For client.com, should we keep, fix or cut each channel, by revenue, value per visitor and retention? (`get_breakdown`, `get_retention`)
10. Which AI assistants sent client.com visitors who paid, and which pages did they cite? (`get_ai_answers`)

Clients that support MCP prompts also list `weekly_brief`, `channel_review` and `trust_check`, which ask these kinds of questions for you.

## What it can't answer

- Ad spend, ROAS or cost per click. The tools return visitors, payments and revenue, not spend.
- Who a customer is. Visitors are IDs and generated names.
- Anything that changes data, such as adding a site or a goal.
- Raw events or bulk exports. Payments come at most 200 per call and breakdowns at most 1,000 rows.

For the full list of tools, arguments and limits, see [MCP server](../reference/mcp.md). For the story behind this setup, read [Ask Claude which marketing channel pays](https://virling.com/blog/ask-claude-which-channel-pays).

[Start free](https://app.virling.com/login) and connect your assistant.
