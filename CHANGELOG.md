# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

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


