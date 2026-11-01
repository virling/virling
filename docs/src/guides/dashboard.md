# Read your dashboard

The dashboard shows what each channel earns, not only how many visitors it sends. Open it at `/app/yourdomain.com` on your Virling server. It updates on its own while it is open, so you never need to reload to see a new visitor or payment.

## The range

The date picker at the top right sets the period for every page: Today, Yesterday, Last 7 days, Last 30 days, Last 90 days, This month, Last 12 months, or Since tracking started. Each number shows its change against the previous period of the same length. The range stays in the address, so a link keeps it. **Custom range** picks exact dates, and **Compare to other dates** compares the range against any other period you choose instead of the one just before it.

## Overview

- **Five numbers:** Revenue, New customers, Revenue per visitor, Visitors and Avg visit time. Revenue counts on the payment date, so some revenue in a range can come from visits before it.
- **Avg visit time:** the average time from a visit's first event to its last. A visit ends after 30 minutes without activity. Only visits with two or more events count, so a visitor who views one page and leaves is not averaged in as zero. It shows a dash when no visit in the range has two events.
- **Why revenue moved:** when revenue changes by at least 20% and $20 against the previous period, one line under the numbers names the channel that moved it most, such as "Revenue is up 41% ($2,000) on the previous period, mostly from Organic search (+$1,300)." It appears only when that one channel explains at least 40% of the move, and not while a filter is on.
- **Matched revenue:** under the five numbers, one line gives the share of revenue matched to a visit and why the rest was not, such as payments with no visitor ID or with no visit inside the lookback window. It disappears when every payment is matched. See [Link checkout to visits](../payments/checkout.md).
- **Revenue chart:** bars for revenue per day, with a dot per day for visitors on the right axis. Hover a bar for the day's payments, and click it to filter the page to that day. Launches you log, and notes added through the [API](../reference/api.md), show as markers on their day.
- **Filters:** every filter you click shows as a chip at the top. **See visitors** opens the Visitors page with the same filters, so you can see the people behind a number.
- **Insight line:** the biggest finding in the period, such as what an AI assistant visitor is worth compared with your average.

## The breakdown cards

Each card lists visitors, revenue and revenue per visitor for every row, with a chip that compares the row with your site average: green above it, red below it.

| Card | What it shows | Tabs |
| --- | --- | --- |
| Channels | Where visitors came from. The Channel tab also shows each channel's average visit time. Open a channel to see its sources. Direct is split by how visitors arrived: typed or bookmarked, a shared link, or a return from checkout. | Channel, Source, Campaign, Keyword |
| Pages | The first page of the visit, or the last page before paying. | Landing page, Before paying |
| Goals | Funnel: Visit, Signup, Checkout started and Paid, with the share that reached each step and the drop between steps. Goals: every custom goal with its count. | Funnel, Goals |
| AI | Crawls: which AI companies' bots read your pages, and how many. Answers: how often assistants name your site, see [AI answers](../track/ai-answers.md). Funnel: crawled, named, visited and paid, with revenue per assistant. | Crawls, Answers, Funnel |
| Recent customers | The last 10 paying customers, newest first: amount, first source, visits before paying, time to convert and a short path. Click one for their timeline. | |
| Location | Where visitors are. | Country, City, Language |
| Devices | What they use. | Device, Browser, OS |
| Timing | Visitors and revenue by weekday or by hour, in your site's timezone, with the best times to post. | Weekday, Hour |

The Channels card also shows **Refunded**, the share of a row's payments that were refunded in whole or part. A dash means none. Virling does not store disputes apart from refunds: a lost dispute counts as a refund where the provider reports it, and a dispute still open does not count.

Click a row to filter the whole page by it. Revenue per visitor is the number to watch: a channel with many visitors and a low chip is loud but does not pay, and a small one with a high chip is worth more of your time. See [Find which channel pays](channel-revenue.md).

## More cards

These appear below the main grid when they have something to show.

- **Channel verdicts:** one plain line per channel against the period before, such as what earned most, what to do more of, and what earned nothing. The same lines are in the weekly email and the client report.
- **Reading:** for each page, how long it stays visible and how far it is scrolled, for every visitor or only for buyers.
- **Almost paid:** visitors who reached pricing or checkout and did not pay, by channel or by their last page, with the revenue left on the table.
- **Profit by channel** and **Time to pay**, described below.

## Profit by channel

In **Site settings**, under **Monthly spend per channel**, put one channel per line with what you spend on it each month in dollars, such as `Paid 300` or `Social 50`. The channels are Paid, Email, AI, Organic search, Communities, Social, Referral and Direct. When any channel has a spend, a **Profit by channel** card appears on the dashboard. For the range you pick it shows each channel's spend (the monthly amount scaled to the range), the revenue credited to it, the profit, and the return per dollar spent. Nothing connects to an ad platform, and the numbers are only as good as the amounts you type.

Revenue is already net of refunds and lost disputes. To take payment fees off too, add a line such as `Fees 2.9% + 0.30`: the percent of each channel's revenue plus the fixed amount for each of its payments. The card then lists every channel with revenue, its costs are spend plus fees, and profit is what is left. A channel with no spend entered shows only its fees as costs, and its return shows No data rather than an endless return.

## Time to pay

The **Time to pay** card shows, for each channel that brought at least three first payments in the range, the median time from the customer's first visit to their first payment, and the share who paid within 1, 7 and 30 days. A channel with a long median is not failing, so give it the time its customers need before you cut it. Renewals do not count, and it reads the 500 newest payments in the range.

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

When nobody is on the site, the globe keeps turning slowly with a few soft grey pulses and says it is idle. The pulses are decoration, not visitors. The globe uses WebGL through a bundled copy of three.js that loads only on this page; if the browser has no WebGL, the page shows the same lists without the globe.

With **Public dashboard** on in Site settings, the globe is also public at `/share/yourdomain.com/globe`. Anyone with the link sees visitors and payment pulses for the last 30 days, with animal names only: no emails, no customer journeys, and no amounts when you hide revenue on the public dashboard. The page can be embedded in an iframe. Turn the public dashboard off and the globe stops working with it.

Places are shown at the country level, so visitors from the same country sit close together around its centre. Visitors with no known country are listed but not drawn.

## The other pages

- **Live globe:** visitors and payments on a globe, described above.
- **Visitors:** every visitor with their first source, pages, goals and what they paid. Open one to see their full journey. Filter by source and goal, search pages, or use **Filters** to pick visitors who left through a link or without one, and who paid or did not. The name stands alone, with country, browser and system in a muted column beside it, and goals, exit links and intents all show as the same kind of chip. Direct visits show a neutral no-referrer icon. The account menu shows a name made from your email (maya.chen@example.com reads Maya Chen); hover it for the full address.
  A visitor page opens with their device, OS, browser and visitor ID (click the copy icon to copy it), a one-line summary, and stats for first source, visits, pageviews, time on site, goals, spent and last exit. Time on site adds up each visit from its first to its last event, and a visit ends after 30 minutes of no activity. A dash means nothing later is known, not zero. The timeline lists every visit with the time on each page, and each payment sits where it happened with its amount, provider and the source it was credited to: by visitor ID when the payment carried one, or through the customer when it did not.
- **Payments:** every payment with its source, status and time to complete. Open one to see the visits before it. Filter by goal, attributed, unattributed or refunded.
- **Retention:** customers who still pay, by the channel of their first payment, with lifetime value by first source and the path to purchase: how many days and visits customers took before paying. See [Retention by channel](../attribution/retention.md).
- **Launches:** a ledger of the posts, threads and launch days you log by hand, each with a name, an optional link, a channel and a day. Next to each one Virling shows the visitors, new customers and revenue of that day and the day after, in your site's timezone, so you can see whether a launch made money or only traffic. The figures are all traffic on those two days, not only the visitors the launch sent, so they show what the days brought and not proof of cause.
- **Client report:** a shareable report for one site. See below.
- **All clients** (Agency): every site of the account grouped by client, with each site's visitors and revenue over the last 30 days and a total per client. Set a site's client under **Site settings**, **Client**; the site switcher groups sites the same way.
- **Site settings:** the script, payment connections, attribution model, goals and sharing.

## Sample data

On a site with no events yet, the setup step and the empty dashboard offer **Try with sample data**. It fills that site with generated visitors, payments and goals, so every page and every [MCP tool](../reference/mcp.md) shows real-looking results. A **Sample data** notice stays on the site until you click **Remove sample data**, which deletes exactly the generated rows.

Sample data is never counted anywhere that matters: the public dashboard, client report links and weekly emails stay off while it is present, and no payment provider is contacted. It is never mixed with real data: the first real pageview, or connecting a payment provider, removes it automatically. Only the account owner can add or remove it.

## Telegram alerts

In **Site settings > Telegram alerts**, paste a bot token from @BotFather and your chat ID. Virling sends a test message, then one message per new paid payment, with its amount and source, within a minute. The token is stored encrypted, and only the account owner can set it. You also get one message on a day when visitors reach at least three times the daily average of the week before (and at least 20), and a summary of yesterday after 9 in the morning in the site's timezone. Alerts stay off while a site holds sample data.

## Share a site

In **Site settings**, unless noted:

- **Public dashboard** turns on a read-only page anyone can open without signing in, for building in public. It never shows emails or customer IDs. Turn it off and the link stops working at once.
- **Client link** gives an agency's client a read-only report page, with your name, logo, accent colour and footer. Revoke the link to stop access at once. **Download PDF** saves the same report.
- **Share card:** on the dashboard's Channels card, **Share card** opens an image of revenue by channel for the range you picked, for posting on social. Pick whether it shows revenue amounts (off by default, so only each channel's share shows), the site name, and a dark look. **Download PNG** saves it. The card is drawn by Virling and holds only channel names, shares and the amounts you chose, never customers or emails.
- **Weekly email** sends the report every week. Personal emails it to you with Virling branding; Agency adds white-label client reports. Paste a Slack incoming webhook URL under **Change schedule** to also post a short summary (the headline, the week's numbers and the top three sources, with a link to the full report) to a Slack channel at the same time; Slack alone also works. The webhook URL is stored encrypted.

## Account settings

Open it from the account menu at the bottom of the sidebar:

- **API keys** for the [Read API](../reference/api.md), the [CLI](../reference/cli.md) and the [MCP server](../reference/mcp.md).
- **Connected apps** that you allowed to read your data over MCP, each with a Revoke button.
- Your plan and sign-out.
