# AI crawlers

The **AI** card on the dashboard has one row per AI company, with the pages its crawlers read next to the visitors and revenue its assistants sent. Hover a company's pages read to see each bot's hits, pages, most-read page and when it was last seen. Crawler hits are stored apart from visits, so they never count as visitors.

The **Answers** tab of the same card counts the answer fetchers per assistant and, with your own API keys, how often assistants name you. See [AI answers](ai-answers.md).

Each bot has a type:

| Type | What the bot does | Bots |
| --- | --- | --- |
| Training | Collects pages to train models | GPTBot, ClaudeBot, anthropic-ai, Google-Extended, GoogleOther, Applebot-Extended, meta-externalagent, CCBot, Bytespider |
| Search | Builds an AI search index | OAI-SearchBot, Claude-SearchBot, PerplexityBot, Amazonbot |
| Answer | Fetches a page live because someone asked an assistant | ChatGPT-User, Claude-User, Perplexity-User, meta-externalfetcher, DuckAssistBot |

Google-Extended and Applebot-Extended are mostly `robots.txt` tokens. Google and Apple crawl with their normal bots, so these two rarely show up.

## What the script sees

The tracking script runs in browsers, and most crawlers never run JavaScript. Out of the box, Virling only counts the crawlers that fetch `/v.js` with a `Referer` or run the script. That is a small share of them.

## Count every crawler (optional)

To see them all, forward AI crawler requests from your server or edge to `POST /api/crawl` on your Virling server. Send JSON with the site, the full page URL and the crawler's user agent:

```json
{ "s": "example.com", "u": "https://example.com/pricing", "ua": "Mozilla/5.0 (compatible; GPTBot/1.2; +https://openai.com/gptbot)" }
```

Virling keeps only known AI crawlers, on pages of that site, and always answers `202`. Send it without waiting, so your page is never slower. The snippets below forward only user agents that look like AI crawlers. Replace `https://stats.example.com` with your Virling server and `example.com` with your site.

### Next.js middleware

```js
// middleware.js
import { NextResponse } from "next/server";

const AI = /gptbot|oai-searchbot|chatgpt-user|claudebot|claude-|anthropic-ai|perplexity|google-extended|googleother|applebot-extended|amazonbot|meta-external|ccbot|bytespider|duckassistbot/i;

export function middleware(req, event) {
  const ua = req.headers.get("user-agent") || "";
  if (AI.test(ua)) {
    event.waitUntil(fetch("https://stats.example.com/api/crawl", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ s: "example.com", u: req.url, ua }),
    }).catch(() => {}));
  }
  return NextResponse.next();
}
```

### Express

```js
const AI = /gptbot|oai-searchbot|chatgpt-user|claudebot|claude-|anthropic-ai|perplexity|google-extended|googleother|applebot-extended|amazonbot|meta-external|ccbot|bytespider|duckassistbot/i;

app.use((req, res, next) => {
  const ua = req.get("user-agent") || "";
  if (AI.test(ua)) {
    fetch("https://stats.example.com/api/crawl", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ s: "example.com", u: `https://${req.hostname}${req.originalUrl}`, ua }),
    }).catch(() => {});
  }
  next();
});
```

### Cloudflare Worker

Put it on the route that serves your site:

```js
const AI = /gptbot|oai-searchbot|chatgpt-user|claudebot|claude-|anthropic-ai|perplexity|google-extended|googleother|applebot-extended|amazonbot|meta-external|ccbot|bytespider|duckassistbot/i;

export default {
  async fetch(request, env, ctx) {
    const ua = request.headers.get("user-agent") || "";
    if (AI.test(ua)) {
      ctx.waitUntil(fetch("https://stats.example.com/api/crawl", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ s: "example.com", u: request.url, ua }),
      }));
    }
    return fetch(request);
  },
};
```

### Rails

```ruby
# app/controllers/application_controller.rb
AI_CRAWLER = /gptbot|oai-searchbot|chatgpt-user|claudebot|claude-|anthropic-ai|perplexity|google-extended|googleother|applebot-extended|amazonbot|meta-external|ccbot|bytespider|duckassistbot/i

before_action do
  ua = request.user_agent.to_s
  if ua.match?(AI_CRAWLER)
    url = request.original_url
    Thread.new do
      Net::HTTP.post(URI("https://stats.example.com/api/crawl"),
        { s: "example.com", u: url, ua: ua }.to_json, "Content-Type" => "application/json")
    rescue StandardError
      nil
    end
  end
end
```

Crawler hits have their own per-site rate limit, so a crawler flood never drops real visits.
