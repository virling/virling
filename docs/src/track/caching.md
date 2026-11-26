# Page caching and CDNs

Page caching and a CDN do not break tracking. The tag is static, and everything that differs per visitor happens in the browser after the page loads, so a cached copy of the HTML works for every visitor. What can break it are optimization settings that rewrite scripts. This applies to WordPress and WooCommerce caching plugins, Shopify's own page cache, and sites behind Cloudflare or a similar CDN.

## What happens when HTML is cached

- **The script.** A cached page carries the same tag for everyone: `v.js` with `data-site`. The browser loads `v.js` from your Virling server, which serves it with `Cache-Control: public, max-age=3600`, so a CDN may cache it as well.
- **The visitor cookie.** The `virling_vid` cookie is created by the script in the browser, not sent by your server with the page. A cached page cannot hand one visitor's ID to another, and each visitor keeps their own ID on later visits. See [Visitor ID](script.md#visitor-id).
- **UTM tags.** The script reads `utm_source`, `ref` and the other tags from the address in the browser, not from the HTML. A cache that ignores the query string when it picks a cached copy is fine, as long as the visitor's address still has it. See [UTM tags and ref](script.md#utm-tags-and-ref).
- **Checkout links.** The visitor ID is added to hosted checkout links in the browser when the page loads and again on click, so links in cached HTML get it too. See [Checkout links need no code](../payments/checkout.md#checkout-links-need-no-code).

## Reading the visitor cookie in server code

The script runs in the browser, so it works the same on every kind of page: it creates the cookie and reads the UTM tags from the address there, never from your HTML. What changes is when your own server code runs, and so whether it can see the cookie. This matters when your server creates the checkout session, for example a Stripe Checkout Session with `client_reference_id`.

One rule covers every case: read the cookie in the request that creates the session, not when the page is rendered to be viewed.

- **First visit.** On a first visit the server renders the page before the script has set the cookie, so code that runs while the page renders finds none. By the time the buyer presses Buy, the browser has the cookie and sends it with that request.
- **Prerendered and cached pages.** A page made at build time, or one copy of the HTML served to many visitors from a cache, is not rendered for this visitor, so there is no cookie of this visitor at all. The tag is the same for everyone, so the copy itself is safe. Create the session in a server route that runs on request, which a form on the prerendered page can post to, and keep `POST` requests to it out of the cache.
- **Hidden fields.** The same goes for a hidden form field filled in while the page renders: from a shared copy, every buyer would send the same value. Read the cookie on the server when the form is posted instead.
- **No server.** A fully static site has no server code to read the cookie. The script adds the visitor ID to hosted checkout links in the browser, so use those. See [Checkout links need no code](../payments/checkout.md#checkout-links-need-no-code).
- **Client-side route changes.** Routers that navigate with the History API have their route changes counted on their own. Each change of path or query is one pageview, and a change of only the `#hash` is none. Later routes carry no referrer, so they stay in the channel of the landing page. The cookie stays in the browser through all of it, so it is sent from whichever page the buyer presses Buy on. Checkout links that appear after a route change get the visitor ID when they are clicked. See [Single-page apps](spa.md).

## What can break it

- **Combining or inlining scripts.** The script finds its own tag to read `data-site` and to know which server to send events to. Merged into a combined file or inlined into the page, it no longer has its tag, and it sends nothing.
- **Hosting the script on your own domain.** A copy of `v.js` served from your site sends events to `/api/event` on your site, where there is no Virling server. Load it from your Virling server, or add `data-api` with the full URL of your server's `/api/event`. See [Script attributes and API](../reference/script.md).
- **Delaying scripts until interaction.** The script then runs only after a scroll, tap or key press, so visitors who leave without one are never counted.
- **Redirects that drop the query string.** A redirect from `http` to `https`, from `www.` to the bare domain, or to a "clean" URL that loses `?utm_source=...` loses the tags before the script can read them.
- **Cached checkout.** The WooCommerce snippet reads the `virling_vid` cookie on your server when the order is created. If that request never reaches WordPress with its cookies, the order has no visitor ID.

Minifying the page and adding `async` to the tag are fine. `v.js` is already minified.

## Safe settings

1. **Exclude `v.js` from combining, inlining, delaying and local hosting.** Most optimization plugins have an exclusion list for each; add `v.js`. With Cloudflare's Rocket Loader on, add `data-cfasync="false"` to the tag:

   ```html
   <script defer data-cfasync="false" src="https://stats.example.com/v.js"
     data-site="yourdomain.com"></script>
   ```

2. **Keep query strings.** Leave tags on page addresses through every redirect. Options that remove query strings from static files such as CSS and JS are fine.
3. **Do not cache checkout.** Keep cart, checkout and account pages out of the page cache, and let checkout requests reach your server with their cookies. Most WooCommerce caching plugins do this by default; check it if you added your own rules. On Shopify, checkout is hosted by Shopify and the [theme snippet](../payments/checkout.md) runs in the browser, so there is nothing to change.
4. **Clear the cache after you add or change the tag**, so cached pages carry the new one.

## Check that it works

Open your site in a private window with `?utm_source=test&utm_medium=test` on the address, then open the network tab:

- `v.js` loads from your Virling server, as its own file, right after the page loads.
- A `POST` to `/api/event` is answered with `202`, without scrolling or clicking first.
- `window.virling.vid()` in the console returns an ID, the same on every page.

Then run the full [Test your setup](../guides/test-your-setup.md) checklist with a test payment.

## Troubleshooting

- **No visits.** View the page source and check that the tag is still there with `data-site`, and that `v.js` loads as its own file from your Virling server. If visits show only after you scroll or click, a delay setting is holding the script. Clear the cache after changing the exclusions. See [No visitors show up](../troubleshooting.md#no-visitors-show-up).
- **Visits but payments unattributed.** Open the payment: if it lists no visitor ID, checkout did not carry `virling_vid`. Check that checkout is not cached and that the cookie reaches your server there. See [Checkout is not linked](../troubleshooting.md#checkout-is-not-linked) and [Payments show as Unattributed](../troubleshooting.md#payments-show-as-unattributed).
- **UTMs missing.** Open a tagged link and watch the address bar: if the tags are gone once the page loads, a redirect dropped them. Fix the redirect rule to keep the query string, and link to the final address in your campaigns.
