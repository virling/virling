# Track a launch

This guide is for launching on Product Hunt, Hacker News, Reddit, X or a newsletter and finding out which of them brought paying customers, not only visitors. Do the first two steps the day before you launch. To start, sign in at [app.virling.com](https://app.virling.com/login) for the hosted plan, or [self-host it](../quickstart.md). For a printable list of the same steps, use the [launch tracking checklist](https://virling.com/tools/launch-tracking-checklist).

## 1. Tag every launch link

Make one [tracked link](../track/links.md) per place you post. In the app, open **Links** and fill in a name, the page to open, a source and a channel:

| Where you post | Source | Channel |
| --- | --- | --- |
| Product Hunt | `producthunt` | Communities |
| Hacker News | `hackernews` | Communities |
| Reddit | `reddit` | Communities |
| X | `x` | Social |
| Your newsletter | `newsletter` | Email |

Each link gets its own short URL and its id becomes `utm_campaign`, so every post shows by name under the Campaign tab. Visits from `producthunt.com`, `news.ycombinator.com`, `reddit.com` and `x.com` land in their channel even without tags, but a tag tells two posts on the same site apart and keeps visits that arrive with no referrer, such as from an app, out of Direct. See [Channels](../attribution/channels.md).

## 2. Check the script and checkout

A launch day is the wrong time to find out that checkout drops the visitor. Run one tagged visit and one test payment through the whole path, as described in [Test your setup](test-your-setup.md): the script loads, the visit shows its source, the payment shows that source in the payments list instead of Unattributed, and its journey starts at the tagged visit.

## 3. Watch the day

- **Log the launch.** Open **Launches** in the sidebar and fill in **Log a launch**: a name such as "Show HN post", the link to the post, its channel and the day. It shows as a marker on the revenue chart, and the Launches list gives the visitors, new customers and revenue of that day and the day after. Those figures count all traffic on the two days, not only the visitors the launch sent.
- **Watch it live.** On the dashboard, set the range to Today. **Latest payments** and **Visitors** at the top update on their own, and the Channels card shows which source the visitors come from. **Live globe** shows visitors arriving and a pulse for each payment with the channel that brought it.
- **Compare posts.** **Links** shows the clicks, visitors and revenue of each tracked link.

See [Read your dashboard](dashboard.md) for every card.

## 4. Read revenue by channel a week later

Many launch visitors sign up first and pay days later. About a week after launch, set the range to Last 7 days, or a **Custom range** from launch day to today, and open the Channels card:

- The **Source** tab splits Communities by source, such as Product Hunt, Hacker News and Reddit, and the **Campaign** tab shows each tracked link.
- Revenue per visitor shows which post brought buyers rather than only traffic.
- **Payments** lists every payment with its source. Open one to see the visits before it and which one got the credit.

Revenue counts on the payment date, and each payment is credited under your site's [model and lookback window](../attribution/model.md). With the default, last non-direct over 90 days, a visitor who found you on launch day and came back later by typing your URL is still credited to the launch post, as long as they pay within 90 days of that visit. A payment with no visit inside the window shows as Unattributed. Check the numbers again a month later: launch revenue keeps arriving after the first week.

## Read more

- [Product Hunt launch revenue: how to measure it](https://virling.com/blog/product-hunt-launch-revenue)
- [Hacker News launch revenue](https://virling.com/blog/hacker-news-revenue-tracking)
- [Find which channel pays](channel-revenue.md)

[Start free](https://app.virling.com/login) and tag your launch links before you post.
