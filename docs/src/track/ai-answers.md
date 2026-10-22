# AI answers

The **Answers** tab of the AI card shows, per assistant, how often it uses and names your site, next to the visitors and revenue it already sent you. Click an assistant to see your questions and its answers, with your name highlighted.

It has two layers:

| Column | What it counts | Needs |
| --- | --- | --- |
| Used in | Answers where the assistant opened one of your pages live, because someone asked it a question | The [server snippet](ai-crawlers.md#count-every-crawler-optional) |
| Mentioned | Of your buyer questions, asked once a day, how many answers named your site | Your own API keys |
| Visitors, Revenue | Visits that came from the assistant, and the revenue credited to them | Nothing extra |

## Used in: no keys needed

ChatGPT, Claude and Perplexity fetch a page with a separate user agent when they read it to answer someone: ChatGPT-User, Claude-User and Perplexity-User. Virling counts each of these fetches as one answer that used your site, with the page it read. One answer that opens three of your pages counts three times.

These fetchers never run JavaScript, so the tracking script alone almost never sees them. Add the [server snippet](ai-crawlers.md#count-every-crawler-optional) to count them. Search crawlers such as OAI-SearchBot and Claude-SearchBot build an index ahead of time, so they are not counted here; they show on the Visits tab.

## Mentioned: your buyer questions

Once a day, Virling asks each of your questions to each assistant you added a key for, with web search on, and stores the answer. For each answer it records:

- whether it names your site, by any of the names you set or your domain,
- your position among the products it names, in order of first mention (best effort),
- which of your pages it cites,
- the answer text, cut to 4,000 characters, for the drill-down.

Answers change from run to run, so the dashboard shows a rate, such as "7 of 20", and the change from the period before.

### Set it up

1. Open **Site settings** and go to **AI answers**.
2. Add a key for any of OpenAI, Anthropic, Perplexity and Google Gemini. Links next to each one go to the provider's key page.
3. Write 5 to 20 buyer questions, one per line. Ask what a buyer would type, without your name: "best invoicing app for freelancers", not "is Tallybird good". Virling suggests three from your homepage title to start.
4. Set the names to look for, such as your product name and its other spellings. Your domain always counts.
5. Save. The first run starts within the hour, or click **Run now**.

The daily run happens in the background and never while a page loads. **Run now** works three times a day per site, and a manual run replaces that day's answers.

### Models

| Assistant | API | Model | Search |
| --- | --- | --- | --- |
| ChatGPT | OpenAI Responses API | `gpt-6-luna` | `web_search` tool |
| Claude | Anthropic Messages API | `claude-haiku-5-5` | `web_search` tool, one search |
| Perplexity | Perplexity Agent API | `fast` preset | built-in web search |
| Gemini | Gemini `generateContent` | `gemini-2.5-flash-lite` | Google Search grounding |

These are the providers' low-cost models with search. The consumer apps may use bigger models, so treat the results as a signal of how often assistants find and recommend you, not a copy of what a given person sees.

Gemini only reports which sites it used, not which pages, so its cited pages show as your domain.

### Cost

Each provider bills your key directly. Virling shows an estimate in settings from your number of questions and keys. At list prices, one question costs about:

| Assistant | Per question | 20 questions a day, per month |
| --- | --- | --- |
| ChatGPT | $0.011 | about $6.60 |
| Claude | $0.011 | about $6.60 |
| Perplexity | $0.002 | about $1.20 |
| Gemini | $0.0002 | about $0.12 |

Most of the ChatGPT and Claude cost is the search fee of $10 per 1,000 searches. Gemini's free tier includes Google Search grounding for up to 500 requests a day with `gemini-2.5-flash-lite`, so 20 questions a day fit in it; paid keys get 1,500 a day before grounding is billed. Each **Run now** costs one more day of questions.

### Keys

Keys are stored like payment keys: encrypted with AES-256-GCM with the server's secret key, shown only by their last four characters, never logged, and only sent to their own provider. Remove a key to stop asking that assistant; past answers stay.

When a provider refuses a key or a request, settings show its error next to the key and that assistant is skipped until the next day.

## Google AI Overviews

AI Overviews in Google Search are not covered. Google has no API for them, and scraping search results breaks Google's terms. Gemini with Google Search grounding is the closest stand-in: it searches Google and answers from the results.

## Read API and MCP

The same numbers are in the [read API](../reference/api.md#get-apiv1sitesdomainai-answers) and the [MCP server](../reference/mcp.md) as `get_ai_answers`.
