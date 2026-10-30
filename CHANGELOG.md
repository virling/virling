# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.3.0] - 2026-10-30

### Added

- Add path to purchase to the Payments page
- Add lifetime value by first source to Retention
- Add email alerts for sales and a daily digest
- Add Timing card and default to last 7 days
- Add AI to cash card
- Add Almost paid card to the dashboard
- Add HSTS, CSP and permissions headers to the site
- Add Creem payment provider
- Add see visitors link, chart day filter, row underline
- Add cookieless-until-consent mode
- Add last-touch attribution model
- Add RevenueCat payment provider
- Add a rule-based verdict per channel
- Add the DataBull comparison page
- Add Paperclip setup and a weekly review to the skill

### Changed

- Move hosted to app.virling.com with a 14-day trial

### Other

- Compute Timing in one cached pass
- Record reading time and scroll depth per page
- Hide the scrollbar on the dashboard tab strip
- Start dashboard card work early and set 20 ms budget
- Split Direct traffic by how visitors arrived
- Build direct_kind for the server only
- Give Other sources a globe icon on Retention
- Trim the heavy index tests outside CI
- Time tests with nextest
- Run server tests in parallel shards
- Optimize bundled duckdb in dev and test builds
- Reorder dashboard cards and merge the AI cards
- Fill the AI card height with its revenue share chart
- Write docs sidebar links relative to each page
- Link docs pages without .html so they skip the redirect
- Let the docs link check follow clean URLs
- Keep the app out of search results
- Fill the last row of extra dashboard cards
- Let sites pick how often AI answers run
- Accept another tool's script API as a drop-in switch
- Install and start the service with one command
- Plain homepage headline, switch FAQ, price rows
- Use the app menu for the AI answers frequency
- Give the first 50 per plan the founding price
- Import daily traffic history from a CSV export
- Lead pricing with the founding price
- Show bot names once in the AI crawls table
- Load the docs tracker from app.virling.com
- Expect bot names once in the AI card test
- Match the AI answers schedule wording on main
- List Creem before LemonSqueezy in outbound hosts
- Read the Creem visitor ID through the shared key list
- Describe the switch page for any previous provider
- Mark launches and notes on the revenue chart
- Let the API docs test read POST routes
- Pass the day link in the chart notes test
- Check last touch after the first-touch assertions
- Compare the dashboard against any date range
- Pass the day link to the chart notes test
- Take payment fees off channel revenue in profit card
- Pass the test archive through the cache, not artifacts
- Say LemonSqueezy checkouts match through webhooks
- Point the Claude plugin at app.virling.com
- Track our own hosted sales by channel
- Pass the visitor ID to hosted checkout links

## [0.2.1] - 2026-10-27

### Added

- Add an AI crawlers card
- Add an Account settings page for plan and API keys
- Add search meta, structured data and llms.txt
- Add an answer FAQ and lead with outcomes on the home page
- Add an answer FAQ to the agency page
- Add a guide to finding which channel pays
- Add a dark theme with an Auto, Light or Dark choice
- Add pricing, self-host, demo and changelog pages
- Add Personal founding pre-order to the worker
- Add Personal founding spot button on home and pricing
- Add a dashboard screenshot and Docker quickstart to the README
- Add integration pages and a blog to virling.com
- Add comparison pages to virling.com
- Add AI answers settings with keys and questions
- Add AI answers to the read API and MCP
- Add OAuth sign-in for the MCP server
- Add a Claude Code plugin marketplace
- Add a dashboard guide and refresh the hosted plan page
- Add Copy prompt to the script and checkout setup steps
- Add optional Turnstile to the sign-in form
- Add privacy policy page
- Add terms and refund pages
- Add detail to the visitor page
- Add a README to the Claude plugin
- Add try with sample data for empty sites
- Add MCP tool titles, reword list_sites
- Add optional dedicated MCP host
- Add live globe page with payment bursts
- Add operator admin panel
- Add Telegram alerts for new payments
- Add full screen and live arrivals to the globe
- Add a refunded column to the Channels table
- Add the hosted 14-day trial and subscriptions via Dodo
- Add comparison pages and alternative-style titles
- Add a launch ledger page
- Add a public live globe for shared sites
- Add an About page and link it from the footer
- Add four SEO articles with FAQ markup
- Add four more SEO articles

### Changed

- Move the Claude plugin into its own folder
- Update the per-IP limit test for the burst limit

### Fixed

- Fix the onboarding Payments step copy and key form
- Fix the Next.js snippet for App and Pages Router
- Fix lookback and quickstart link on self-host page
- Fix admin panel on current main, add admin bench routes
- Fix skills path in site build

### Removed

- Drop the compare phrase from the line chart title when off
- Drop the count from the payments goal label
- Drop the compromised eMudhra root from bundled certs
- Drop repeated logos from the empty Answers tab
- Drop impossible Macs and let owners ignore their visits
- Remove the Bots card from the dashboard

### Other

- Send virling.com analytics to beta.virling.com
- Split community sites into a Communities channel
- Track key virling.com actions as goals
- Fall back to the virling_vid cookie at checkout
- Shrink the tracker and allow 1.2 KB in CI
- Count a repeat customer's later payments as renewals
- Price MRR from what each subscription is charged
- Pin weekly report test to a fixed time in the week
- Round each payment to cents so totals match rows
- Label new customers with the range start
- Give chart visitors their own axis
- Share one date range control across app pages
- Keep the range menu on screen on phones
- Keep the not-tracking hatch off days with data
- Set the visitor cookie on the registrable domain
- Log pageviews on replaceState route changes
- Treat www and same-visit domains as one site
- Show payment states by provider connection
- WIP: return visits, leave on pagehide, bfcache returns
- Treat Back to another page as a move, not a return
- Show Left to and Came back rows on the timeline
- Document leaves, returns and the k field
- Track Viewed pricing on any page under /pricing
- Expose settings switches and radio groups as controls
- Use the plural helper for every count string
- Label the return-visit radios as a radio group
- Default site timezone to the browser and allow changing it
- Show zero revenue in neutral gray
- Trust Cloudflare headers only from Cloudflare
- Explain missing cities when visits exist
- Space hourly chart ticks evenly
- Link done setup steps and read done state from data
- Give docs pages canonical URLs, descriptions and a sitemap
- Answer channel and API questions in the docs FAQ
- Show the dashboard's visit on the setup Script step
- Flag automated browsers in the tracker
- Keep flagged bot visits out of every metric
- Describe what ships today in the README
- Note that LemonSqueezy payments are not attributed yet
- Keep the demo pages out of search indexes
- Say LemonSqueezy payments are not attributed yet
- Put API doc paths in code so parameters show
- Lay out the setup checklist steps side by side
- Keep each setup step to a single line
- Say LemonSqueezy payments are not attributed yet
- Track docs.virling.com as part of virling.com with goals
- Keep card height fixed when switching tabs
- Detect a site's platform and payments from its homepage
- Give Before paying its own empty message
- Ping every 30 s from visible tabs
- Refresh the dashboard live as visitors arrive
- Let Agency accounts invite team members
- Draw app icons as thin line glyphs
- Shorten the Link checkout setup step
- Calm the dashboard cards
- Use full-color brand marks for source logos
- Draw DuckDuckGo, Tux and Copilot in their colors
- Keep every route under 10 ms at 100x data
- Write first pageviews after the events, in batches
- Keep Visitors columns readable at every width
- Hide the source type label below 1180 px
- Show KPIs 2 by 2 when narrow and align card headers
- Size the index budget from memory, warn near it
- Wait for an in-flight touch flush before reading
- Rework the Visitors list
- Store session and visitor IDs without heap strings
- Hold every bench route to 10 ms in CI
- Line up Payments rows and keep the tabs by the Goal filter
- Stretch the last dashboard cards to fill the row
- Give the pricing cards one height and aligned rows
- Regenerate the demo with the current UI
- Regenerate the demo on every site deploy
- Count funnel steps in one pass over the range
- Write chart tooltips in place instead of piecemeal
- Let a failing payment key be replaced from settings
- Read the UTC offset from the cached zone table
- Skip the touch flush for reads of events and bots
- Schedule a touch flush only when rows were left pending
- Write pending first pageviews once a second
- Sort the visitor list before building its rows
- Show a readable error when Stripe rejects a stored key
- Keep the report chart legend inside the page
- Refresh every live number on app pages without a reload
- Turn the waitlist into a beta application
- Pin the hero card and rename the nav button
- Close beta applications on November 8
- Use Apply for beta in the nav on every page
- Fall back to the browser time zone for the country
- Use tight, current screenshots on the demo page
- Find Stripe invoices through invoice payments
- Detect Dodo renewals from the first paid payment
- Attach LemonSqueezy visitor IDs from a signed webhook
- Say LemonSqueezy payments are attributed over https
- Credit charges after a free trial to the trial's visit
- Recompute attribution only for changed payments
- Ask buyer questions to AI assistants each week
- Show AI answers in the AI card
- Document AI answers setup, cost and API
- Run AI answers weekly with server keys and a budget
- Publish agent discovery files on virling.com
- Run hosted AI mention checks only on paid plans
- Rate limit OAuth registration and token calls per IP
- Name the /pricing funnel step by its path
- Support the 2026-07-28 MCP revision with server/discover
- Accept Client ID Metadata Documents for MCP sign-in
- Turn off MCP sign-in when the base URL has a path
- Open the beta to everyone from the site
- Limit sign-in link requests per IP
- Show the sign-in check only when needed
- Show average visit time on Overview and Channels
- State hosting, deletion and sign-in data in privacy
- Name the operator in the privacy policy
- Make the Claude plugin keyless with OAuth sign-in
- Let the directory find the plugin icon by path
- Point the Claude plugin at hosted Virling
- Wrap the payment credit line on small screens
- Check Turnstile action and hostname on sign-in
- Enable the sign-in button once the check is ready
- Document sample data
- Show how much revenue is matched to a visit
- Name in-app browsers and messengers as sources
- Point the Claude plugin at mcp.virling.com
- Make two MCP tool descriptions purely descriptive
- Link the Claude directory listing in the docs and README
- Draw the globe with real ocean and land colors
- Preload globe assets, time the globe, move the idle note
- Calmer Visitors list: one Filters menu, name alone
- Polish Visitors: header, direct icon, avatars, account name
- Document the Visitors polish
- Grant admin from the database only, send invites by SMTP
- Test the missing-admin case against the admins table
- Pin the sample data notice while scrolling
- Say the Personal plan has no event limit
- Serve public pages as Markdown to AI crawlers
- Make payment pulses bigger
- Build a Markdown copy of every site page
- Pin visitors at their city
- Make the idle globe honest and readable
- Give the full screen globe an Earth look
- Show top countries on the globe
- Serve Markdown to Accept: text/markdown
- Block temp mail sign-in and add a burst limit
- Keep the worker out of this change
- Publish OAuth metadata, auth.md and WebMCP tools
- Publish the Poppy discovery file
- Use the full-color Google logo in the hero
- Offer the AI crawler snippet and a test on setup
- Send a Telegram digest and traffic spike message
- Hint at the server snippet on the AI card
- Name the site as its own protected resource
- Document the Dodo billing settings
- Link founding pre-orders to hosted accounts by email
- Document the founders lookup settings
- Make pricing, size and provider claims agree
- Count every bot and show them in a Bots card
- Count AI fetches of the site's own pages
- Forward every bot fetch, not only AI ones
- Name browser-like bots by their real tool
- Skip platform words and numbers in bot names
- Style the AI snippet block like the script block
- Show profit per channel from monthly spend
- Show how long each channel takes to get paid
- Keep the first card row and fit channel names
- Say why revenue moved on the dashboard
- Tidy the site footer and update its status
- Trim titles and add visible FAQs with markup
- Keep generated demo pages out of search
- Give dashboard cards in a row equal height
- Lead the landing page with a concrete outcome
- Link articles from product and comparison pages
- Name Devendra Variya as author on every article
- Give every dashboard card the same width and height
- Count DeepSeekBot as AI and hide probe paths in Bots

## [0.2.0] - 2026-10-17

### Added

- Add end-to-end tests for renewal attribution
- Add account menu to the app sidebar
- Add static demo dashboard at /demo
- Add Visitors page with per-visitor exit timeline
- Add read API with per-account API keys
- Add a WordPress plugin
- Add virling query CLI for the read API
- Add agent skill for revenue attribution questions
- Add read-only MCP server at /mcp
- Add retention by first channel
- Add tracked short links
- Add a Language tab to the Location card
- Add a Personal or Agency plan setting
- Add a hidden bench command that times every route
- Add a public shareable dashboard

### Changed

- Update docs for v0.1.1 and the install script
- Format the font table
- Move tracked links into the Campaign tab
- Format the worker module list

### Fixed

- Fix ok badges, retention tabs and link copy styles

### Removed

- Drop more automation traffic and referrer spam
- Remove the site switcher from the dashboard top bar
- Drop the links redirect from the bench routes

### Other

- Key the release build cache by runner image
- Replace agency report doodle with a real report mock
- Skip failed payments when finding a customer's first
- Credit plan changes to the customer's first channel
- Count the same customer ID on two providers as two customers
- Track outbound clicks and page leaves
- Store exits for virling.com and show exit pages
- Bundle logos for ChatGPT, Bing, Copilot, Slack and more
- Test that every known source has a bundled logo
- Fetch and cache favicons for unknown referrers
- List favicon fetches under outbound connections
- Keep the logo coverage test out of the worker build
- Use the two-step checkout so payment methods load
- Send pre-orders to the hosted checkout page
- Revert "Send pre-orders to the hosted checkout page"
- Ask for the billing address so payment methods load
- Show payments as customer rows with time to pay
- Give demo visitors an operating system
- Tab the dashboard breakdowns and add a world map
- Give demo visits a region, city, host and exits
- Mark the static demo as generated for GitHub
- Serve fonts from our own domain
- Cache landing CSS, JS and fonts for a year
- Serve landing art as sized WebP
- Defer site.js and fix low-contrast landing text
- Inline the landing stylesheet
- Pin the demo to the styles it was built with
- Load the home page diagrams on scroll
- Serve /favicon.ico
- Rebuild the dashboard to the v3 design
- Document retention by channel
- Leave empty exit link and goal cells blank
- Retry a failed site icon fetch after ten minutes
- Count new customers in period totals
- Serve source logos and glyphs as image files
- Let mail go out under a display name
- Rebuild the client report as a weekly emailed report
- Give every visitor a generated face
- Give every visitor a unique animal name and face
- Refresh demo seed and export visitors and icons
- Seed subscriptions and export the retention page
- Use the email glyph for a lowercase newsletter source
- Seed richer demo visits, goals and tracked links
- Regenerate the demo with the current UI
- Let the demo seed a data dir to run the app on
- Say per visitor in report insight cards
- Build the current dashboard design
- Pick custom ranges on an in-app calendar
- Store subscription prices and show MRR
- Save named funnels per site
- Keep first pageviews in tables and drop correlated queries
- Cache store reads per site until its data changes
- Read known alias slots in one query and slim breakdowns
- Serve dashboard aggregates from a per-site session index
- Keep language in touches and the session index
- Share per-range work across index reads
- Count every dimension in one pass over the range
- Read MRR from pooled connections and pass it through the cache
- Switch revenue chart types without a reload
- Regenerate the demo with the current UI
- Serve payment, visitor and goal rows from the index
- Wait for the site index before timing routes
- Version app.css and app.js so updates load at once
- Share the asset version with the site worker
- Keep expanded cards at a fixed height
- Serve the goal table and funnels from the index
- Serve retention and MRR from the index
- Count each AI visitor once in the AI share
- Time new-visitor and concurrent ingest in the bench
- Share commits across concurrent ingests, skip no-op upserts
- Count events written during an index load once
- Time fresh 12m and all-time dashboards in the bench
- Keep long-range counts and day totals across appends
- Run the bench in CI against checked-in budgets

## [0.1.1] - 2026-10-15

### Added

- Add contributing guide and issue templates
- Add founding pre-order link
- Add pre-order API and founding email webhook
- Add inline pre-order checkout to /agency
- Add payments table and model
- Add encrypted storage for provider keys
- Add USD conversion for payments
- Add provider interface and sync loop
- Add Stripe API fixtures
- Add Stripe payment sync
- Add Dodo Payments sync
- Add LemonSqueezy payment sync
- Add Polar payment sync
- Add attribution job
- Add dashboard queries
- Add per-site sync and live visitor queries
- Add dashboard, payments page and journey drawer
- Add payments and checkout setup steps
- Add site settings
- Add uptime check for virling.com
- Add stats page
- Add report, payment summary and first-touch queries
- Add server-side SVG charts
- Add product mock to hero and vary section layouts
- Add logos for AI, social, chat, writing and search sources
- Add the mdBook theme for the docs site
- Add a docs feedback issue form
- Add robots.txt and sitemap.xml to virling.com
- Add a one-command installer script
- Add a per-site client report page
- Add white-label branding for client reports
- Add revocable read-only links and print styles for client reports

### Changed

- Format key vault

### Fixed

- Fix landing page claims and install steps
- Fix clippy lint in key vault
- Fix overlapping labels in landing diagrams
- Fix client report build after the favicon change

### Other

- Require Rust 1.99
- Restore source_name for referrer display
- Read visitor ID from Stripe PaymentIntent metadata
- Tag Polar checkout links with the visitor ID
- Load ICU up front and keep DuckDB in UTC
- Mount app pages and add handler tests
- Install the DuckDB ICU extension on first start
- Use Dodo checkout SDK 2.0.0
- Serve the tracker from virling.com
- Store virling.com events in D1
- Attribute waitlist signups and pre-orders to channels
- Redeploy the site when shared modules change
- Allow unused shared channel helpers in the worker
- Report unreadable Dodo responses as sync errors
- Collapse connected providers on the payments step
- Correct checkout snippets
- Set founding agency price to $29 with 30 sites and 1M pageviews
- Clarify pre-order charge wording
- Allow discount codes on the pre-order checkout
- Ask only for what checkout needs
- Charge $29 once to hold a founding spot
- Serve landing page art from the site worker
- Redesign the main and agency landing pages
- Ship source logos, flags and empty-state art
- Redesign the dashboard
- Redesign the payments page and journey drawer
- Show the $19 hosted price and founding discount
- Sign the site and emails from Deven and Rahul
- Match agency hero chip and link to main page
- Count only tracked customers in conversion rate
- Send app email as Deven and Rahul at Virling
- Wait for the tracker before creating a checkout
- Shrink the tracker to under 1 KB gzipped
- Show referrer rows as canonical domains
- Use the site domain in app URLs
- Use the Dodo Payments and Polar logos
- Make dashboard numbers consistent and polish the UI
- Tag Dodo payment links with the visitor ID
- Classify more AI, social, chat, writing and search sources
- Use a tagged source host when the referrer is missing
- Use the full width on settings and setup pages
- Write the docs pages
- Build and deploy the docs to docs.virling.com
- Point the install link and README to the docs
- Put the founders signature above the link buttons
- Let the install tag bar span the full section width
- Show each site's own favicon in the switcher
- Send waitlist emails from Rahul at Virling
- Build Intel macOS binaries and an arm64 Docker image
- Build Linux binaries on Ubuntu 22.04 for older glibc
- Serve install.sh from virling.com
- Restyle the client report with the dashboard tiles and tables

## [0.1.0] - 2026-10-13

### Added

- Add AGPL-3.0 license
- Add Rust server, landing pages and waitlist
- Add Cloudflare Worker for virling.com
- Add README
- Add CI workflow
- Add site deploy workflow
- Add changelog config
- Add release workflows
- Add Dockerfile

### Changed

- Format code with rustfmt

### Fixed

- Fix clippy warnings
- Fix clippy warnings in worker
- Fix release artifact download

### Other

- Disable debug info in CI builds


