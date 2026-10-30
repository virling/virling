# AI answers

The **Answers** tab of the AI card shows, per assistant, how often it uses and names your site, next to the visitors and revenue it already sent you. Click an assistant to see your buyer questions and its answers, with your name highlighted.

It has two layers:

| Column | What it counts | Needs |
| --- | --- | --- |
| Used in | Answers where the assistant opened one of your pages live, because someone asked it a question | The [server snippet](ai-crawlers.md#count-every-crawler-optional) |
| Mentioned | Of your buyer questions, asked on your schedule, how many answers named your site | A paid hosted plan; keys when self-hosting |
| Visitors, Revenue | Visits that came from the assistant, and the revenue credited to them | Nothing extra |

## Used in

ChatGPT, Claude and Perplexity fetch a page with a separate user agent when they read it to answer someone: ChatGPT-User, Claude-User and Perplexity-User. Virling counts each of these fetches as one answer that used your site, with the page it read. One answer that opens three of your pages counts three times.

These fetchers never run JavaScript, so the tracking script alone almost never sees them. Add the [server snippet](ai-crawlers.md#count-every-crawler-optional) to count them. Search crawlers such as OAI-SearchBot and Claude-SearchBot build an index ahead of time, so they are not counted here; they show on the Visits tab.

## Mentioned

Once a week, Virling asks each of your buyer questions to ChatGPT, Claude, Perplexity and Gemini with web search on, and stores the answers. For each answer it records:

- whether it names your site, by any of the names to look for,
- your position among the products it names, in order of first mention (best effort),
- which of your pages it cites,
- the answer text, cut to 4,000 characters, for the drill-down.

Answers change from run to run, so the dashboard shows a rate, such as "7 of 32", and the change from the period before.

### Nothing to set up

Once your site has visits, Virling writes 8 buyer questions from your homepage title, your top landing pages and search keywords, and picks the names to look for from your domain and title. The first run follows within the hour, then every week. In settings, **How often** switches it to every day or every month.

To change them, open **Site settings > AI answers**. Keep 5 to 20 questions, each what a buyer would ask without your name: "best invoicing app for freelancers", not "is Tallybird good". **Run now** asks them again right away, once a day. The runs happen in the background and never while a page loads.

### Hosted plan

Included on paid plans and founding spots. Virling pays the providers and no key is needed. Free and trial sites still see Used in.

### Self-hosted

Each provider bills the key it is asked with. Either:

- set server keys for every site with `VIRLING_OPENAI_KEY`, `VIRLING_ANTHROPIC_KEY`, `VIRLING_PERPLEXITY_KEY` and `VIRLING_GEMINI_KEY` (see [environment variables](../reference/env.md)), or
- add keys per site in **Site settings > AI answers**, for providers without a server key.

Any provider without a key is skipped. `VIRLING_AI_ANSWERS_BUDGET_USD` caps the estimated spend per month across all sites; runs pause when it is reached and start again next month. Each run logs its estimated cost and the month's total.

Per-site keys are stored like payment keys: encrypted with AES-256-GCM, shown only by their last four characters, never logged, and only sent to their own provider. When a provider refuses a key, settings show its error and that assistant is skipped until the next run.

When two sites on a server ask the same question on the same day, the provider is asked once and both use the reply.

### Models

| Assistant | API | Model | Search |
| --- | --- | --- | --- |
| ChatGPT | OpenAI Responses API | `gpt-6-luna` | `web_search` tool |
| Claude | Anthropic Messages API | `claude-haiku-5-5` | `web_search` tool, one search |
| Perplexity | Perplexity Agent API | `fast` preset | built-in web search |
| Gemini | Gemini `generateContent` | `gemini-2.5-flash-lite` | Google Search grounding |

These are each provider's lowest-cost model with search. The consumer apps may use bigger models, so treat the results as a signal of how often assistants find and recommend you, not a copy of what a given person sees.

Gemini only reports which sites it used, not which pages, so its cited pages show as your domain.

### Cost

At list prices, one question costs about:

| Assistant | Per question | 8 questions a week, per month |
| --- | --- | --- |
| ChatGPT | $0.011 | about $0.38 |
| Claude | $0.011 | about $0.38 |
| Perplexity | $0.002 | about $0.07 |
| Gemini | $0.0002 | under $0.01 |

That is about $0.84 a site a month with all four. Most of the ChatGPT and Claude cost is the search fee of $10 per 1,000 searches. Gemini's free tier includes Google Search grounding for up to 500 requests a day with `gemini-2.5-flash-lite`, and paid keys get 1,500 a day before grounding is billed at $35 per 1,000. Each **Run now** costs one more run.

## Google AI Overviews

AI Overviews in Google Search are not covered. Google has no API for them, and scraping search results breaks Google's terms. Gemini with Google Search grounding is the closest stand-in: it searches Google and answers from the results.

## Read API and MCP

The same numbers are in the [read API](../reference/api.md#get-apiv1sitesdomainai-answers) and the [MCP server](../reference/mcp.md) as `get_ai_answers`.
