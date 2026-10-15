# Channels

Every session gets one channel, decided by its first pageview: Paid, Email, AI, Organic search, Social, Referral or Direct.

## Rule order

The rules run in this order and the first match wins:

1. **`utm_medium`** (case-insensitive):
   - `cpc`, `ppc`, `paid`, `paidsearch`, `paid_social` or `paidsocial` is **Paid**.
   - `email`, `newsletter` or `e-mail` is **Email**.
2. **Referrer host**, when the visitor came from another site:
   - A known AI host is **AI**.
   - A known search host is **Organic search**.
   - A known social host is **Social**.
   - Any other host is **Referral**.
3. **`utm_source`**, or `ref` when `utm_source` is missing, when there is no referrer:
   - A known AI source is **AI**, a known search source is **Organic search**, a known social source is **Social**.
   - Any other value is **Referral**.
4. Any other `utm_medium` with no referrer and no source is **Referral**.
5. Everything else is **Direct**.

Referrers from your own domain and its subdomains are ignored, so moving between your pages never changes the channel.

## Known hosts

A leading `www.` is ignored, and subdomains match: `old.reddit.com` matches `reddit.com`. Entries ending in a dot match any domain ending, so `google.` covers `google.com` and `google.de`.

| Channel | Hosts |
| --- | --- |
| AI | `chatgpt.com`, `chat.openai.com`, `perplexity.ai`, `claude.ai`, `gemini.google.com`, `copilot.microsoft.com`, `chat.deepseek.com` |
| Organic search | `google.`, `bing.com`, `duckduckgo.com`, `search.yahoo.`, `yahoo.`, `ecosia.org`, `yandex.`, `baidu.com`, `search.brave.com`, `startpage.com`, `kagi.com`, `qwant.com` |
| Social | `t.co`, `x.com`, `twitter.com`, `facebook.com`, `instagram.com`, `linkedin.com`, `lnkd.in`, `reddit.com`, `news.ycombinator.com`, `youtube.com`, `youtu.be`, `threads.net`, `bsky.app`, `mastodon.social`, `pinterest.com`, `tiktok.com`, `producthunt.com`, `indiehackers.com` |

AI is checked before search, so `gemini.google.com` is AI, not Organic search.

## Known sources

These `utm_source` or `ref` values are matched exactly, lowercased:

| Channel | Sources |
| --- | --- |
| AI | `chatgpt`, `openai`, `perplexity`, `claude`, `gemini`, `copilot` |
| Organic search | `google`, `bing`, `duckduckgo`, `yahoo`, `ecosia`, `brave` |
| Social | `twitter`, `x`, `facebook`, `fb`, `instagram`, `ig`, `linkedin`, `reddit`, `hackernews`, `hn`, `youtube`, `threads`, `bluesky`, `tiktok`, `producthunt` |

## Examples

| Landing | Channel |
| --- | --- |
| `/?utm_source=google&utm_medium=cpc` from google.com | Paid, medium wins over the referrer |
| `/?utm_medium=newsletter` | Email |
| From `news.ycombinator.com` | Social |
| From `chatgpt.com` | AI |
| `/?ref=producthunt` with no referrer | Social |
| From `someblog.dev` | Referral |
| Typed URL or bookmark | Direct |

## Referrer names

In breakdowns, aliases share one row: `t.co` and `twitter.com` show as `x.com`, `lnkd.in` as `linkedin.com`, `chat.openai.com` as `chatgpt.com`, and every Google country domain as `google.com`.

Custom channel rules are <span class="na">Not available yet</span>.
