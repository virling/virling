# Run Virling for client sites

This guide sets up one Virling account for an agency: a site per client, each with its own script and its own payment connection, a branded report your client can open, and the same numbers through the API or an AI assistant. For what the Agency plan includes, see [virling.com/agency](https://virling.com/agency).

## What you need

- A Virling account. Sign in at [app.virling.com](https://app.virling.com/login) for the hosted plan, or [self-host it](#self-host-for-your-clients).
- The Agency plan. In **Account settings**, pick **Agency** as the plan. It adds client grouping, white-label client reports and team members. See [Hosted plan](../hosted.md) for prices.
- Access to each client's site, where you can add one tag to the `<head>`.
- A read-only key from each client's payment provider.

## 1. Add each client site

1. Click **+ Add a site** in the sidebar, or open `/app/setup`.
2. Enter the client's domain, such as `clientshop.com`, and its timezone, then continue.
3. Open the new site's **Site settings** at `/app/clientshop.com/settings`, type the client's name under **Client** and click **Save client**.

Sites with the same client name are grouped in the site switcher and on **All clients** at `/app/clientshop.com/clients`, which shows each site's visitors and revenue over the last 30 days and a total per client. A client with more than one domain gets one site per domain under the same name.

On a self-hosted server you can also add a site from the command line with [`virling add-site`](../reference/cli.md#add-site).

## 2. Install the script on each client domain

1. Open `/app/clientshop.com/setup/script`. It shows the exact tag for that site.
2. Paste it before `</head>` on every page of the client's site:

```html
<script defer src="https://app.virling.com/v.js"
  data-site="clientshop.com"></script>
```

On a self-hosted server, the `src` is your own server's address. `data-site` must match the domain you added, so each client site gets its own tag. The setup screen waits for the first pageview and shows when it arrives. See [Add the script](../track/script.md) and the [framework guides](../track/frameworks.md).

## 3. Connect the client's own payment provider

Each site has its own payment connections, so every client's revenue stays on their site.

1. Ask the client for a read-only key from their provider, such as a Stripe restricted key with read access. [Which key to create](../payments/index.md#which-key-to-create) lists what each provider needs.
2. Open `/app/clientshop.com/setup/payments`, pick the provider, paste the key and click **Connect and backfill**.
3. Pass the visitor ID into the client's checkout so payments link to visits. See [Link checkout to visits](../payments/checkout.md).

Virling imports the last 90 days, then checks for new payments every minute. It never creates, changes or refunds anything. When the engagement ends, **Disconnect** in site settings deletes the stored key, and the client should also delete the key in their provider's dashboard.

## 4. Give the client access to their numbers

Clients do not need a Virling login. In the site's **Site settings**:

1. Under **Client report**, add your agency name, logo URL, accent colour and footer, and click **Save branding**. Turn off **Show Powered by Virling on reports** if you want only your brand.
2. Under **Client link**, click **Create link** and send it to the client. Anyone with the link sees that site's report without signing in. **Revoke link** turns it off at once, and **New link** replaces it.
3. To send the report every week, open **Client report** at `/app/clientshop.com/report`, click **Change schedule**, turn on **Weekly email** and put the client's addresses under **Send to**, up to 10. The email uses your branding. Sending it needs [sign-in email](../self-host/smtp.md) set up when you self-host.

**Download PDF** on the report saves the same page to attach to your own deck.

Team members are for your own staff. Invite them in **Account settings > Team**. A member sees and manages every site on the account, so do not invite a client as a member. The Agency plan includes 3 users, the owner included.

## Report to clients

The question clients ask is which channel paid. These answer it, for any range you pick on the dashboard:

- **Channels** card: visitors, revenue and revenue per visitor for each channel. Open a channel, or use the Source and Campaign tabs, to see the posts, sites and UTM campaigns behind it.
- **Channel verdicts**: one plain line per channel against the period before. The same lines are in the client report and the weekly email.
- **Payments**: every payment with the channel it was credited to. Open one to show its journey.
- **Retention**: which channels bring customers who keep paying.
- **Profit by channel**: when you enter the client's monthly spend per channel in site settings, each channel's spend, revenue, profit and return per dollar.

See [Read your dashboard](dashboard.md) for every card, and [Model and lookback window](../attribution/model.md) for how a payment is credited.

### Pull the same numbers by API or MCP

1. In **Account settings > API keys**, create a key. Only the account owner can. The key reads every site on the account and cannot change anything except add chart notes.
2. List your client sites with `GET /api/v1/sites`, then read each one:

```sh
curl -H "Authorization: Bearer $VIRLING_API_KEY" \
  "https://app.virling.com/api/v1/sites/clientshop.com/breakdown/channel?range=30d"
```

3. To ask an AI assistant instead, connect it to the [MCP server](../reference/mcp.md) with the same key or OAuth sign-in. `list_sites` returns every client site, and `get_breakdown` with `dimension` `channel` and `get_channel_verdicts` answer which channel paid.

All endpoints and fields are in [Read API](../reference/api.md).

## Self-host for your clients

Self-hosting is free, with every feature, on your own server. Start with the [Quickstart](../quickstart.md), then [Docker](../self-host/docker.md) or a [release binary](../self-host/binary.md) behind a [reverse proxy](../self-host/reverse-proxy.md). Set up [sign-in email](../self-host/smtp.md) so invites and weekly client emails go out, and keep [backups](../self-host/data.md) of the data folder. On a self-hosted server, an Agency account includes up to 3 users. Teams larger than that use the hosted Agency plan.

## Questions agencies ask

### How many client sites can one account hold?

Virling sets no limit on the number of sites in an account. Add one per client domain, as in [step 1](#1-add-each-client-site). Hosted plans include a monthly event allowance, listed on [virling.com/pricing](https://virling.com/pricing). A self-hosted server has no plan limits.

### Can a client see other clients?

No. A client link, its PDF and the weekly email each show one site's report. The client name only groups sites for you, in the site switcher and on **All clients**, which need a Virling login. Team members, API keys and the MCP server read every site on the account, so keep those for your own staff. See [Who can see the data](../hosted.md#who-can-see-the-data).

### Does the client have to share a payment key?

Yes. Virling reads payments with a key from the client's own provider account, and never creates, changes or refunds anything. For most providers the key can be read-only. Creem and RevenueCat keys also need write access to add a webhook, and [LemonSqueezy](../payments/lemonsqueezy.md) keys cannot be limited. Keys are encrypted before they are stored, are never logged, and the app shows only their last characters. See [Which key to create](../payments/index.md#which-key-to-create) and [How keys are stored](../payments/index.md#how-keys-are-stored).

### What happens to the data when a client leaves?

1. **Revoke link** in **Site settings** stops the client report at once. Remove their addresses from the weekly email too.
2. **Disconnect** deletes the stored payment key and stops syncing. Payments already imported stay in the reports. The client should also delete the key at their provider. See [Replace or disconnect a key](../payments/index.md#replace-or-disconnect-a-key).
3. To delete the site and its data on the hosted plan, email hello@virling.com and it is deleted within 30 days. On a self-hosted server, the [admin panel](../self-host/admin.md#what-it-does) deletes a site with its stored events at once.

A hosted site cannot be exported or moved to another account, so past visits stay behind if the client sets up their own Virling. See [Can I move from hosted to self-hosted?](../faq.md#can-i-move-from-hosted-to-self-hosted).

### How are reports branded?

Branding is set per site under **Client report** in **Site settings**: your agency name, a logo URL, an accent colour and a footer, with **Show Powered by Virling on reports** to turn off. It applies to the client link, **Download PDF** and the weekly email. White-label client reports come with the Agency plan. See [step 4](#4-give-the-client-access-to-their-numbers) and [Share a site](dashboard.md#share-a-site).

### Can I self-host Virling for my clients?

Yes. One self-hosted server holds every client site, with every feature and no plan to buy, and an Agency account on it includes up to 3 users. A client can also run their own server from the same open-source code. Payments are credited the same way on either, as in [How attribution works](../attribution/index.md). See [Self-host for your clients](#self-host-for-your-clients).
