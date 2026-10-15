# FAQ

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

Only the [event endpoint](reference/event-api.md) for sending events. A read API is <span class="na">Not available yet</span>.

## How do I report a security problem?

Email security@virling.com. Do not open a public issue.
