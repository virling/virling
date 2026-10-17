# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

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


