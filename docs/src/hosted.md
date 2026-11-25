# Hosted plan

Hosted Virling runs at app.virling.com. Every plan starts with a 14-day free trial: add a card, cancel before day 15 and nothing is charged.

It runs the same open-source code, so the [script](track/script.md), [payment connections](payments/index.md) and [checkout linking](payments/checkout.md) work the same way, without a server to run.

## Plans

- **Personal:** $19 a month. The same open-source app, run for you, with unlimited sites, 100,000 events a month and a weekly email report.
- **Agency:** $79 a month. Unlimited client sites, 1M events a month, white-label weekly client reports and a team of 3.

The first 50 customers of each plan keep a founding price for life: $9 a month for Personal and $29 a month for Agency. It is applied at checkout while spots last, after the same free trial.

Self-hosting stays free, with every feature.

## Included on hosted

Virling pays for the AI assistants that [AI answers](track/ai-answers.md) asks, so no API key is needed. This is included on every hosted plan.

## What is set up for you

- **The server.** There is nothing to install, and every site on your account uses `https://app.virling.com/v.js` as its script address.
- **Email.** Sign-in links, team invites and weekly reports are sent for you, with no SMTP server to set up.
- **Webhooks.** LemonSqueezy, Creem and RevenueCat need Virling on a public `https` address, which app.virling.com is. Virling adds the webhook with your key when you connect.

You still add the script to your site, create the payment key and pass the visitor ID into checkout.

## Which payment providers work

Nine: [Stripe](payments/stripe.md), [Dodo Payments](payments/dodo.md), [Polar](payments/polar.md), [LemonSqueezy](payments/lemonsqueezy.md), [Creem](payments/creem.md), [RevenueCat](payments/revenuecat.md), [Paddle](payments/paddle.md), [WooCommerce](payments/woocommerce.md) and [Shopify](payments/shopify.md). You can connect more than one to the same site.

The first sync imports the last 90 days. Shopify gives 60 days, and RevenueCat only one-time purchases from the last 90 days. Those earlier payments are usually [Unattributed](attribution/unattributed.md), because the script was not on your site yet.

## What a payment key can do

Virling uses the key to read payments, refunds and subscriptions. It never creates, changes or refunds a payment. [Which key to create](payments/index.md#which-key-to-create) lists the exact access per provider.

- **Read access is enough** for Stripe, Dodo Payments, Polar, Paddle, WooCommerce and Shopify. Stripe secret keys (`sk_`) are refused.
- **Creem and RevenueCat** keys also need write access for webhooks, so Virling can add one.
- **LemonSqueezy** keys cannot be limited: a key has full access to every store on the account. Virling reads orders, subscriptions and subscription invoices, and adds a webhook to each store.

Keys are encrypted with AES-256-GCM before they are stored, are never logged, and the app only shows their last characters. **Disconnect** deletes the stored key and stops syncing; payments already imported stay in your reports. To end access fully, also delete or roll the key at your provider. See [How keys are stored](payments/index.md#how-keys-are-stored).

## Who can see the data

- **You and your team.** The owner and, on Agency, each member can open every site on the account. Roles other than owner and member are <span class="na">Not available yet</span>.
- **People you share with.** A public dashboard, client links and report emails show a site's reports to whoever you give them to. You can turn off the public dashboard and revoke a client link. See [Share a site](guides/dashboard.md#share-a-site).
- **Apps you connect.** [API keys](reference/api.md) and the [MCP server](reference/mcp.md) read every site on the account. Only the owner can connect an app or create a key, and can revoke both.
- **Virling, as the operator.** Stacklance Infotech LLP runs the service and handles your visitors' data on your behalf. A Virling admin can open an account read only for 30 minutes, and each time is recorded. See [Admin panel](self-host/admin.md#what-it-does).
- **Service providers.** The app and its databases run on Oracle Cloud Infrastructure in Mumbai, India. The [privacy policy](https://virling.com/privacy) lists every provider and what it receives.

Virling does not store visitor IP addresses, or customer names, customer emails or card details from your payment provider. It does not sell personal data. See [Privacy and consent](track/privacy.md).

## First ten minutes

1. **Sign in.** Enter your email at [app.virling.com](https://app.virling.com/login) and open the link it sends. There is no password.
2. **Add the site.** Enter your domain and timezone at `/app/setup`. To start the trial, open **Account settings** and add a card.
3. **Add the script.** Paste the tag from the setup screen before `</head>` on every page. See [Add the script](track/script.md) and the [framework guides](track/frameworks.md).
4. **Connect payments.** Pick your provider, paste the key and click **Connect and backfill**. See [Connect a provider](payments/index.md#connect-a-provider).
5. **Pass the visitor ID at checkout.** [Some checkout links need no code](payments/checkout.md#checkout-links-need-no-code); other checkouts need a [short snippet](payments/checkout.md#per-provider).
6. **Test it.** Run [Test your setup](guides/test-your-setup.md), which takes about ten minutes itself, to see a tagged visit and a test payment arrive.

For the steps in your kind of business, see the guides for [SaaS](guides/saas.md), [online stores](guides/ecommerce.md) and [agencies](guides/agencies.md).

## Hosted or self-hosted

| | Hosted | Self-hosted |
| --- | --- | --- |
| Where it runs | Run for you at app.virling.com | Your own Linux or macOS server, or [Docker](install/docker.md) |
| Code | The same open-source code | The same open-source code, with every feature |
| Price | [Plans](#plans), after a 14-day free trial | Free |
| Where the data is | Mumbai, India, on Oracle Cloud Infrastructure | The [data folder](self-host/data.md#the-data-folder) on your server |
| Sign-in and report emails | Sent for you | Your [SMTP server](self-host/smtp.md), or sign-in links in the server log |
| AI answers | Runs on [paid plans and founding spots](track/ai-answers.md#hosted-plan), with no API key | Your own [API keys](track/ai-answers.md#self-hosted) |
| Agency team | 3 users included, with paid seats past that | Up to 3 users |
| Import history from another analytics tool | Not offered | From CSV exports, on the [command line](reference/cli.md#import-ga) |
| Export of the account | Not offered. [Moving to your own server](faq.md#can-i-move-from-hosted-to-self-hosted) leaves past visits behind | No export command, but you can open copies of the [database files](self-host/data.md#retention-and-export) |
| Delete a site | No button. Email hello@virling.com and it is deleted within 30 days | In the [admin panel](self-host/admin.md#what-it-does) |
| Uptime guarantee | Not offered, see the [terms](https://virling.com/terms) | You run the server |

Sign in with your email at [app.virling.com](https://app.virling.com/login) to start your free trial. You can also [self-host Virling](quickstart.md).
