# Read your dashboard

The dashboard shows what each channel earns, not only how many visitors it sends. Open it at `/app/yourdomain.com` on your Virling server. It updates on its own while it is open, so you never need to reload to see a new visitor or payment.

## The range

The date picker at the top right sets the period for every page: Today, Yesterday, Last 7 days, Last 30 days, Last 90 days, This month, Last 12 months, or Since tracking started. Each number shows its change against the previous period of the same length. The range stays in the address, so a link keeps it.

## Overview

- **Four numbers:** Revenue, New customers, Revenue per visitor and Visitors. Revenue counts on the payment date, so some revenue in a range can come from visits before it.
- **Revenue chart:** bars for revenue per day, with a dot per day for visitors on the right axis. Hover a bar for the day's payments.
- **Insight line:** the biggest finding in the period, such as what an AI assistant visitor is worth compared with your average.

## The breakdown cards

Each card lists visitors, revenue and revenue per visitor for every row, with a chip that compares the row with your site average: green above it, red below it.

| Card | What it shows | Tabs |
| --- | --- | --- |
| Channels | Where visitors came from. Open a channel to see its sources. | Channel, Source, Campaign, Keyword |
| AI | Visits from AI assistants such as ChatGPT, Claude and Perplexity, and the pages AI crawlers read. See [AI crawlers](../track/ai-crawlers.md) and [AI answers](../track/ai-answers.md). | Visits, Answers |
| Pages | The first page of the visit, or the last page before paying. | Landing page, Before paying |
| Location | Where visitors are. | Country, City, Language |
| Devices | What they use. | Device, Browser, OS |

Click a row to filter the whole page by it. Revenue per visitor is the number to watch: a channel with many visitors and a low chip is loud but does not pay, and a small one with a high chip is worth more of your time. See [Find which channel pays](channel-revenue.md).

## Latest payments and visitors

Two lists sit at the top of the page. **Latest payments** shows each payment with its source and the visit before it. **Visitors** shows who is on the site now and what they did. Both update live.

## The other pages

- **Visitors:** every visitor with their first source, pages, goals and what they paid. Open one to see their full journey.
  A visitor page opens with their device, OS, browser and visitor ID (click the copy icon to copy it), a one-line summary, and stats for first source, visits, pageviews, time on site, goals, spent and last exit. Time on site adds up each visit from its first to its last event, and a visit ends after 30 minutes of no activity. A dash means nothing later is known, not zero. The timeline lists every visit with the time on each page, and each payment sits where it happened with its amount, provider and the source it was credited to: by visitor ID when the payment carried one, or through the customer when it did not.
- **Payments:** every payment with its source, status and time to complete. Open one to see the visits before it. Filter by goal, attributed, unattributed or refunded.
- **Retention:** customers who still pay, by the channel of their first payment. See [Retention by channel](../attribution/retention.md).
- **Client report:** a shareable report for one site. See below.
- **Site settings:** the script, payment connections, attribution model, goals and sharing.

## Share a site

In **Site settings**:

- **Public dashboard** turns on a read-only page anyone can open without signing in, for building in public. It never shows emails or customer IDs. Turn it off and the link stops working at once.
- **Client link** gives an agency's client a read-only report page, with your name, logo, accent colour and footer. Revoke the link to stop access at once. **Download PDF** saves the same report.
- **Weekly email** sends the report every week. Personal emails it to you with Virling branding; Agency adds white-label client reports.

## Account settings

Open it from the account menu at the bottom of the sidebar:

- **API keys** for the [Read API](../reference/api.md), the [CLI](../reference/cli.md) and the [MCP server](../reference/mcp.md).
- **Connected apps** that you allowed to read your data over MCP, each with a Revoke button.
- Your plan and sign-out.
