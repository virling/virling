=== Virling Analytics ===
Tags: analytics, revenue, attribution, privacy, stripe
Requires at least: 6.0
Tested up to: 6.8
Requires PHP: 7.4
Stable tag: 0.1.0
License: GPLv2 or later
License URI: https://www.gnu.org/licenses/gpl-2.0.html

See which channels bring paying customers. Adds the Virling tracking script to every page.

== Description ==

Virling is open-source revenue analytics. It ties every visit to the payment that followed, so you can see which channels, pages and campaigns bring paying customers, not just visitors.

This plugin adds the Virling tracking script (under 1 KB) to your site. You need a Virling server: self-host it for free or use the hosted version.

* Settings > Virling: your server address and site domain
* Skips logged-in administrators by default
* Optional cookieless mode and Global Privacy Control support

== Installation ==

1. Upload the `virling` folder to `/wp-content/plugins/`, or install it from the Plugins screen.
2. Activate the plugin.
3. Open Settings > Virling and enter your Virling server address and site domain.

== External services ==

The plugin loads the tracking script from the Virling server you enter and sends page views to it. No data goes anywhere else. See https://docs.virling.com/track/privacy.html for what is collected.

== Changelog ==

= 0.1.0 =
* First release.
