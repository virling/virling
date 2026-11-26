# Channels

Every session gets one channel, decided by its first pageview: Paid, Email, AI, Organic search, Communities, Social, Referral or Direct.

## Rule order

The rules run in this order and the first match wins:

1. **`utm_medium`** (case-insensitive):
   - `cpc`, `ppc`, `paid`, `paidsearch`, `paid_social` or `paidsocial` is **Paid**.
   - `email`, `newsletter` or `e-mail` is **Email**.
2. **Referrer host**, when the visitor came from another site:
   - A known AI host is **AI**.
   - A known search host is **Organic search**.
   - A known community host is **Communities**.
   - A known social host is **Social**.
   - Any other host is **Referral**.
3. **`utm_source`**, or `ref` when `utm_source` is missing, when there is no referrer:
   - A known AI source is **AI**, a known search source is **Organic search**, a known community source is **Communities**, a known social source is **Social**.
   - Any other value is **Referral**.
4. A visit with no referrer and no source whose user agent shows a known [in-app browser](#in-app-browsers) is credited to that app.
5. Any other `utm_medium` with no referrer and no source is **Referral**.
6. Everything else is **Direct**.

Referrers from your own domain and its subdomains are ignored, so moving between your pages never changes the channel.

## Known hosts

A leading `www.` is ignored, and subdomains match: `old.reddit.com` matches `reddit.com`. Entries ending in a dot match any domain ending, so `google.` covers `google.com` and `google.de`.

| Channel | Hosts |
| --- | --- |
| AI | `chatgpt.com`, `chat.openai.com`, `perplexity.ai`, `claude.ai`, `gemini.google.com`, `copilot.microsoft.com`, `chat.deepseek.com` |
| Organic search | `google.`, `bing.com`, `duckduckgo.com`, `search.yahoo.`, `yahoo.`, `ecosia.org`, `yandex.`, `baidu.com`, `search.brave.com`, `startpage.com`, `kagi.com`, `qwant.com` |
| Communities | `news.ycombinator.com`, `reddit.com`, `producthunt.com`, `indiehackers.com`, `dev.to`, `hashnode.com`, `hashnode.dev`, `lobste.rs` |
| Social | `t.co`, `x.com`, `twitter.com`, `facebook.com`, `instagram.com`, `linkedin.com`, `lnkd.in`, `youtube.com`, `youtu.be`, `threads.net`, `threads.com`, `bsky.app`, `mastodon.social`, `mastodon.online`, `mstdn.social`, `pinterest.com`, `pin.it`, `tiktok.com`, `discord.com`, `discordapp.com`, `discord.gg`, `slack.com`, `t.me`, `telegram.org`, `whatsapp.com`, `wa.me`, `substack.com`, `medium.com` |

AI is checked before search, so `gemini.google.com` is AI, not Organic search.

## In-app browsers

Links opened inside an app often arrive with no referrer. When the user agent names the app, the visit is credited to it instead of Direct. A referrer or a `utm_source` always wins.

| App | Source | Channel |
| --- | --- | --- |
| Instagram | `instagram.com` | Social |
| Facebook and Messenger | `facebook.com` | Social |
| TikTok | `tiktok.com` | Social |
| LinkedIn | `linkedin.com` | Social |
| Telegram | `t.me` | Social |
| WhatsApp | `whatsapp.com` | Social |
| Slack | `slack.com` | Social |
| Discord | `discord.com` | Social |

Some apps open links in a plain system browser that does not name them. Those visits stay Direct.

## Known sources

These `utm_source` or `ref` values are matched exactly, lowercased:

| Channel | Sources |
| --- | --- |
| AI | `chatgpt`, `openai`, `perplexity`, `claude`, `gemini`, `copilot` |
| Organic search | `google`, `bing`, `duckduckgo`, `yahoo`, `ecosia`, `brave`, `yandex`, `baidu` |
| Communities | `hackernews`, `hn`, `ycombinator`, `reddit`, `producthunt`, `indiehackers`, `devto`, `hashnode`, `lobsters` |
| Social | `twitter`, `x`, `facebook`, `fb`, `instagram`, `ig`, `linkedin`, `youtube`, `threads`, `bluesky`, `mastodon`, `tiktok`, `pinterest`, `discord`, `slack`, `telegram`, `whatsapp`, `substack`, `medium` |

A [known host](#known-hosts), such as `reddit.com`, works as a source too.

## Which tags to use

| Link in | `utm_source` | `utm_medium` | Channel |
| --- | --- | --- | --- |
| Ads | The ad platform, such as `google` or `facebook` | `cpc` | Paid, whatever the referrer |
| Email | Your list, such as `newsletter` | `email` | Email, whatever the referrer |
| Social posts and profiles | The site's domain, such as `linkedin.com` | `social` | Social |
| Communities | The site's domain, such as `reddit.com` | `community` | Communities |
| Partners | The partner's domain, such as `partner.com` | `referral` | Referral |
| AI tools | The tool's domain, such as `chatgpt.com` | `referral` | AI |

In the last four rows the source decides the channel, and the medium does not. A referrer from another site still wins over the source. An unknown source counts as Referral, even with `utm_medium=social`.

## Examples

| Landing | Channel |
| --- | --- |
| `/?utm_source=google&utm_medium=cpc` from google.com | Paid, medium wins over the referrer |
| `/?utm_medium=newsletter` | Email |
| From `news.ycombinator.com` | Communities |
| From `chatgpt.com` | AI |
| `/?ref=producthunt` with no referrer | Communities |
| From `someblog.dev` | Referral |
| Link opened in the Instagram app, no referrer | Social, from Instagram |
| Typed URL or bookmark | Direct |

## Referrer names

In breakdowns, aliases share one row: a leading `www.` is dropped, and these hosts show as another:

| Host | Shows as |
| --- | --- |
| `t.co`, `twitter.com` | `x.com` |
| `lnkd.in` | `linkedin.com` |
| `old.reddit.com` | `reddit.com` |
| `chat.openai.com` | `chatgpt.com` |
| `l.facebook.com` | `facebook.com` |
| `threads.net`, `l.threads.com` | `threads.com` |
| `go.bsky.app` | `bsky.app` |
| `pin.it` | `pinterest.com` |
| `discordapp.com`, `discord.gg` | `discord.com` |
| `app.slack.com` | `slack.com` |
| `web.telegram.org` | `t.me` |
| `web.whatsapp.com`, `wa.me` | `whatsapp.com` |
| Every Google domain except `gemini.google.com` | `google.com` |

Custom channel rules are <span class="na">Not available yet</span>.
