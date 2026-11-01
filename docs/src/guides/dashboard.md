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

## Live globe

Open **Live globe** in the sidebar (`/app/yourdomain.com/globe`) to watch visitors arrive on a 3D globe. It is drawn in your browser from one small bundled outline of the world, so it loads nothing from other sites and needs no map key.

- **Visitors** appear as their animal face at the centre of their country. Visitors who have paid have an orange ring. Hover a face for the page they are on.
- **Payments** make a ring pulse at the payer's country with the amount and the channel that brought them, the moment the payment is synced. Open the page and the two latest payments pulse once.
- **Replay a journey:** click a paying customer, on the globe, in the list or in Latest payments. The globe turns to them and a panel steps through where they came from, the pages they viewed, their goals and the payment. Close it to go back.
- **Shading:** the **Visits** and **Revenue per visitor** switch colours each country for the date range at the top right. Revenue per visitor shows which countries are worth more than their traffic suggests.
- **On the site now** lists the people on the globe, with their countries, sources and devices below it. It shows visitors with a tab open and anyone active in the last 5 minutes, up to 60.

Drag to rotate, use the plus and minus buttons (or the arrow keys, `+` and `-` with the globe focused) to turn and zoom. The globe turns slowly on its own when you leave it alone. With reduced motion turned on in your system, it stays still, the rings do not pulse, and the latest payment is marked with a fixed ring and label instead. It follows your light or dark theme and works on a phone.

Places are shown at the country level, so visitors from the same country sit close together around its centre. Visitors with no known country are listed but not drawn.

## The other pages

- **Live globe:** visitors and payments on a globe, described above.
- **Visitors:** every visitor with their first source, pages, goals and what they paid. Open one to see their full journey.
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
