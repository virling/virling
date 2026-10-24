# FAQ

Short answers about attribution, payments, privacy and running Virling.

## How do I see which channel brings paying customers?

Add the [script](track/script.md) to your site, connect your payment provider with a read-only key, and [pass the visitor ID into checkout](payments/checkout.md). Virling then credits each payment to the visit that led to it and shows revenue and revenue per visitor for every channel. The [Quickstart](quickstart.md) walks through all three.

## Is Virling free and open source?

Yes. Virling is licensed under AGPL-3.0 and self-hosting is free. A hosted beta is open at beta.virling.com, paid hosted plans are <span class="na">Not available yet</span>, and you can hold a founding spot on the [Hosted plan](hosted.md) page.

## Do ad blockers block Virling?

Some block requests to hosts or paths they know as analytics. Serving the script and `/api/event` from your own domain through a proxy helps, see [Reverse proxy](self-host/reverse-proxy.md#serving-the-script-from-your-own-domain). A tested first-party proxy setup per platform is <span class="na">Not available yet</span>. Payments are imported from your provider either way, so blocked visitors still count toward revenue, as Unattributed.

## Does it run on an Intel Mac or Windows?

Intel Macs, yes: releases cover Linux x86_64 and arm64 and macOS on Intel and Apple silicon, and the Docker image is linux/amd64 and linux/arm64. Windows builds are <span class="na">Not available yet</span>. On Windows, use Docker or try [building from source](install/source.md).

## Can I track several sites?

Yes. Add each domain as its own site, with `add-site` or in the app, and use its domain in `data-site`. Each site has its own settings, payment connections and reports. Subdomains of a site are counted as that site.

## Can I invite team members?

Not yet. Each site has one owner account. Team members and roles are <span class="na">Not available yet</span>.

## Can I import data from another analytics tool?

Not yet. Migrating from other tools is <span class="na">Not available yet</span>. Payments are backfilled 90 days from your provider, but past visits only exist from when the script was added.

## Why is my revenue Unattributed?

See [Unattributed revenue](attribution/unattributed.md). Most often checkout does not carry the `virling_vid` visitor ID.

## Do I need a cookie banner?

That depends on your jurisdiction. Virling sets one first-party cookie, `virling_vid`. [Cookieless mode](track/cookieless.md) sets none, at the cost of revenue attribution. See [Privacy and consent](track/privacy.md).

## Why does the script not track on my laptop?

It skips `localhost`, `127.*` and `[::1]`. Add `data-dev` to the tag to test locally, and add a site for the domain your pages run on.

## Is there an API?

Yes. The [Read API](reference/api.md) returns the dashboard's numbers as JSON with a read-only key, the [MCP server](reference/mcp.md) lets AI assistants read the same data, and the [event endpoint](reference/event-api.md) accepts events from your server.

## How do I report a security problem?

Email security@virling.com. Do not open a public issue.
