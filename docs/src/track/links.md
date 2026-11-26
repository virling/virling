# Tracked links

A tracked link is a short URL on your Virling server, like `https://stats.example.com/l/35b6cac`, that sends people to a page on your site with UTM tags added. Share it instead of your plain URL in a post, newsletter, bio or printed page, and the dashboard's **Campaign** tab shows the clicks, visitors and revenue it brought.

## Create one

On the dashboard, open the **Campaign** tab of the Channels card and click **+ Tracked link**:

| Field | What it does |
| --- | --- |
| Where will you share it? | X, LinkedIn, Newsletter, YouTube, Reddit, Discord, Product Hunt, Podcast, QR code or Other. It becomes `utm_source`. **Other** asks for a source of your own, such as `indiehackers.com`. |
| Page | A path such as `/pricing`, or a full URL on the site or one of its subdomains. |
| Name | Optional, up to 80 characters, such as "Launch post on X". Left empty, it is the place, such as "X link". |

The short URL appears under the list as **Link ready**. Copy it then: the link gets its own row, with a copy button, only once it has a visit in the range you are viewing.

The link's id, the last part of the short URL, becomes `utm_campaign`. The Campaign tab shows it by the link's name, with its place and clicks.

A link's page, source and name cannot be changed later, and the app has no button to delete it. To change one, create a new link and share that instead.

For creators, affiliates and sponsors, use **Partners** instead, which makes the same kind of link with a report per partner. See [Read your dashboard](../guides/dashboard.md#the-other-pages).

## Name the source and channel

You pick the place; Virling works out the channel from its source and sets `utm_medium` to match:

| Place | `utm_source` | `utm_medium` | Channel with no referrer |
| --- | --- | --- | --- |
| X | `x.com` | `social` | Social |
| LinkedIn | `linkedin.com` | `social` | Social |
| Newsletter | `newsletter` | `referral` | Referral |
| YouTube | `youtube.com` | `social` | Social |
| Reddit | `reddit.com` | `community` | Communities |
| Discord | `discord.com` | `social` | Social |
| Product Hunt | `producthunt.com` | `community` | Communities |
| Podcast | `podcast` | `referral` | Referral |
| QR code | `qr` | `referral` | Referral |
| Other, a known social source | What you typed | `social` | Social |
| Other, a known community source | What you typed | `community` | Communities |
| Other, a known AI or search source | What you typed | `referral` | AI or Organic search |
| Other, anything else | What you typed | `referral` | Referral |

For **Other**, type the site's domain, such as `indiehackers.com`, not a bare name. Both usually land in the same channel, but with no referrer only a [known domain](../attribution/channels.md#known-hosts) shows as itself on the Source tab. The source is lowercased and cut to 80 characters.

To land a newsletter in Email, add the medium to the page: `/pricing?utm_medium=email`. For an ad, use `/pricing?utm_medium=cpc` to land in Paid. Tags already on the page are kept, so these win over the link's own.

### What each channel becomes

Every link has one of five channels. Older links, made when the form asked for a channel, keep the one you picked, including Email and Paid.

| Channel of the link | `utm_medium` | Where the visit lands |
| --- | --- | --- |
| Social | `social` | The channel of the referrer, or of the source when there is no referrer. An unknown source tagged Social counts as Referral. |
| Communities | `community` | The same: the referrer, then the source. An unknown source counts as Referral. |
| Email | `email` | Email, whatever the referrer or source. |
| Paid | `cpc` | Paid, whatever the referrer or source. |
| Referral | `referral` | The referrer, then the source. An unknown source counts as Referral. |

Only `email` and `cpc` decide the channel by themselves. See [Channels](../attribution/channels.md#rule-order) for the full order.

## One link per placement

Make one link for each place you want to compare: two posts on X, a bio and a newsletter are four links. Each link has its own `utm_campaign`, so each gets its own row with its own revenue, even when they share a source and a page.

Put the placement in the name, such as "X bio" or "Newsletter 12 footer", because the name is what the Campaign tab shows.

The Campaign tab lists the top 6 campaigns in the range. With more links than that, shorten the range, or read the rest by `utm_campaign` with the [API](../reference/api.md).

## Share one link in several places

The same link works anywhere, but all its clicks, visitors and revenue share one row, and every visit carries the source you picked:

- **With a referrer**, the referrer decides the channel and the source, unless the medium is `email` or `cpc`. A link made for X and opened from a Reddit page counts as Communities, from `reddit.com`.
- **With no referrer**, as from many apps and email clients, the visit is credited to the place you picked, wherever it was opened.

Click the link's row to filter the dashboard by it, then open the Source tab to see the places that sent a referrer. When you need each place on its own, make one link per place.

## QR codes and printed pages

Choose **QR code** as the place, or **Other** with your own source such as `flyer`. Scans usually have no referrer, so they count as Referral, and the Campaign tab shows each code by name.

- Make the QR code from the short URL with any generator. Virling does not draw it.
- The short URL is short enough to print next to the code for people who type it.
- Make one link per printed item, such as "Booth poster" and "Business card".
- Print it only once the page is settled, because the link's page cannot be changed. The code also stops working if your Virling server's address changes or the site is deleted.
- Point it at a page that runs the script, not at a payment link. A link's page must be on your site, and a checkout link opened straight from a code carries no visitor ID.

## What happens on a click

The server redirects to the page with `utm_source`, `utm_medium` and `utm_campaign` added. Tags already on the page are kept. Clicks from bots are not counted. The visit itself is recorded by the script on your page and attributed like any other tagged visit, so revenue follows your [attribution model](../attribution/model.md).

## Read the numbers

| Number | What it counts |
| --- | --- |
| Clicks | Every time the short URL was opened since you made it, whatever the range. The same person twice counts as two. Bots that name themselves are left out. |
| Visitors | People whose visit began with the link's tags, in the range you picked. |
| Revenue | Payments in the range credited to a visit from the link, after refunds. |

Clicks and visitors rarely match: one is counted by the server at the redirect, for all time, the other by the script on your page, for the range.

## Troubleshooting

### No clicks

- **The link is not in the list.** It shows only when it has visitors or revenue in the range, and is among the top 6. Choose a longer range.
- **People got the plain URL.** Copying the address bar after a click shares the tagged page, not the short URL. Those visits still count for the link, but they are not clicks.
- **The click looked like a bot.** Link previews, uptime checks and tools such as `curl` are redirected but not counted. Test in a normal browser.
- **The short URL returns 404.** The id is mistyped, or it was a partner link that has been removed. Visits it already brought stay in your reports.

### Clicks without visits

- **The page does not run the script**, or a blocker stopped it. A click needs only the server; a visit needs the script on the page it opens. See [Add the script](script.md#when-it-does-not-track).
- **Your site drops the tags.** If the page redirects, such as `/` to `/en`, and loses the query string, the visit is recorded without the link's campaign. Link straight to the final page.
- **The page already had a `utm_campaign`.** It is kept, so the visits show under that campaign instead of the link.
- **The visits are outside the range.** Clicks cover all time, visitors only the range.
- **You were the visitor.** A browser opted out with `#virling-ignore`, or an ignored IP address, counts clicks but no visits.
- **An app opened the link for a preview** without naming itself as a bot. That is a click with no visit.

### Visits without revenue

- **Checkout did not carry the visitor ID.** The payment is there but Unattributed. This is the first thing to check: see [Link checkout to visits](../payments/checkout.md).
- **The site is cookieless.** There is no visitor ID to pass, so payments stay Unattributed. See [Cookieless mode](cookieless.md#what-you-give-up).
- **Another visit got the credit.** With last non-direct, the default, a later visit from search or another link takes it. Open the payment to see its journey.
- **The payment is outside the range or the window.** Revenue counts on the payment date, and only visits inside the [lookback window](../attribution/model.md#lookback-window) before it are considered.
- **The buyer changed browser or device.** The visitor ID belongs to one browser, so a click on a phone does not match a payment on a laptop by itself. See [Recover payments by email](../attribution/unattributed.md#recover-payments-by-email).

To find which one it is, open the link in a private window, make a test payment and follow [Test your setup](../guides/test-your-setup.md).
