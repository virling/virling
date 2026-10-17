# Tracked links

A tracked link is a short URL on your Virling server, like `https://stats.example.com/l/35b6cac`, that sends people to a page on your site with UTM tags added. Share it instead of your plain URL in a post, newsletter, bio or partner page, and Links shows the clicks, visitors and revenue it brought.

## Create one

In the app, open **Links** and fill in:

| Field | What it does |
| --- | --- |
| Name | Only for you, such as "Launch post on X". |
| Destination | A page on the site or one of its subdomains. |
| Source | Where it is shared: `x.com`, `linkedin.com`, `newsletter`. It becomes `utm_source`. |
| Channel | Social, Communities, Email, Paid or Referral. It becomes `utm_medium` (`social`, `community`, `email`, `cpc`, `referral`). |

The link's id becomes `utm_campaign`, so its visits also show under the Campaign tab.

## What happens on a click

The server redirects to the destination with `utm_source`, `utm_medium` and `utm_campaign` added. Tags already on the destination are kept. Clicks from bots are not counted. Visits are attributed like any other tagged visit, so revenue follows your attribution model. A known source such as `x.com` lands in its channel; an unknown source tagged Social counts as Referral.

Deleting a link makes its short URL return 404 right away. Visits it already brought stay in your reports.
