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
4. Any other `utm_medium` with no referrer and no source is **Referral**.
5. A visit with no referrer and no source whose user agent shows a known [in-app browser](#in-app-browsers) is credited to that app.
6. Everything else is **Direct**.

Referrers from your own domain and its subdomains are ignored, so moving between your pages never changes the channel.

## Known hosts

A leading `www.` is ignored, and subdomains match: `old.reddit.com` matches `reddit.com`. Entries ending in a dot match any domain ending, so `google.` covers `google.com` and `google.de`.

| Channel | Hosts |
| --- | --- |
| AI | `chatgpt.com`, `chat.openai.com`, `perplexity.ai`, `claude.ai`, `gemini.google.com`, `copilot.microsoft.com`, `chat.deepseek.com` |
| Organic search | `google.`, `bing.com`, `duckduckgo.com`, `search.yahoo.`, `yahoo.`, `ecosia.org`, `yandex.`, `baidu.com`, `search.brave.com`, `startpage.com`, `kagi.com`, `qwant.com` |
| Communities | `news.ycombinator.com`, `reddit.com`, `producthunt.com`, `indiehackers.com`, `dev.to`, `hashnode.com`, `hashnode.dev`, `lobste.rs` |
| Social | `t.co`, `x.com`, `twitter.com`, `facebook.com`, `instagram.com`, `linkedin.com`, `lnkd.in`, `youtube.com`, `youtu.be`, `threads.net`, `bsky.app`, `mastodon.social`, `pinterest.com`, `tiktok.com` |

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
| Organic search | `google`, `bing`, `duckduckgo`, `yahoo`, `ecosia`, `brave` |
| Communities | `hackernews`, `hn`, `ycombinator`, `reddit`, `producthunt`, `indiehackers`, `devto`, `hashnode`, `lobsters` |
| Social | `twitter`, `x`, `facebook`, `fb`, `instagram`, `ig`, `linkedin`, `youtube`, `threads`, `bluesky`, `tiktok` |

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

In breakdowns, aliases share one row: `t.co` and `twitter.com` show as `x.com`, `lnkd.in` as `linkedin.com`, `chat.openai.com` as `chatgpt.com`, and every Google country domain as `google.com`.

Custom channel rules are <span class="na">Not available yet</span>.
