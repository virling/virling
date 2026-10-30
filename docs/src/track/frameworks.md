# Framework guides

The same tag works everywhere. These are the snippets from the app's setup screen. Replace `stats.example.com` with your Virling server and `yourdomain.com` with your site.

## HTML

Before `</head>` on every page:

```html
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

## Next.js

Use `next/script` with `strategy="afterInteractive"` and load it once, so it runs on every route. `defer` does nothing on `next/script`, so leave it out. Route changes are tracked through `history.pushState` and `replaceState`, so client-side navigation is counted.

### App Router

In the root layout:

```jsx,file:app/layout.tsx
import Script from "next/script";

export default function RootLayout({ children }) {
  return (
    <html lang="en">
      <body>
        {children}
        <Script src="https://stats.example.com/v.js"
          data-site="yourdomain.com" strategy="afterInteractive" />
      </body>
    </html>
  );
}
```

### Pages Router

In the custom App:

```jsx,file:pages/_app.tsx
import Script from "next/script";

export default function App({ Component, pageProps }) {
  return (
    <>
      <Script src="https://stats.example.com/v.js"
        data-site="yourdomain.com" strategy="afterInteractive" />
      <Component {...pageProps} />
    </>
  );
}
```

## Astro

Use `is:inline` so Astro leaves the tag as it is instead of bundling it. The script reads its own attributes, which only works on the original tag.

```html,file:src/layouts/Layout.astro
<script is:inline defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

## WordPress

The Virling plugin adds the tag for you and survives theme updates. Download the `integrations/wordpress/virling` folder from the [repository](https://github.com/virling/virling/tree/main/integrations/wordpress/virling), upload it to `wp-content/plugins/`, activate it, then enter your Virling server and site domain in Settings, Virling. It skips logged-in administrators by default.

Without the plugin, open Appearance, Theme File Editor, open `header.php` and paste the tag before `</head>`.

```html,file:header.php
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

## Framer

1. In Framer, open **Site settings**, **General**, then **Custom code**.
2. Paste the tag into **End of `<head>` tag** so it loads on every page.
3. Publish the site. Custom code only runs on the published site, not in the editor preview.

```html
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

Framer moves between pages without a full reload; the script counts each page change on its own. For a payment button that links to Stripe, Polar, Dodo Payments or LemonSqueezy checkout, nothing else is needed: see [Checkout links need no code](../payments/checkout.md#checkout-links-need-no-code).

## Webflow

1. In Webflow, open **Site settings**, then **Custom code**.
2. Paste the tag into **Head code**. Custom code needs a paid site plan.
3. Publish the site to your domain.

```html
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

## Google Tag Manager

Import the ready-made container, or add one tag by hand.

**Import.** Download [`integrations/gtm/virling-container.json`](https://github.com/virling/virling/blob/main/integrations/gtm/virling-container.json). In Tag Manager, open **Admin**, **Import Container**, choose the file, pick your workspace and **Merge**. It adds a **Virling** tag that fires on all pages and two constants, **Virling server** and **Virling site**. Set those two to your Virling address and site domain, then **Submit** to publish.

**By hand.** Add a tag of type **Custom HTML** with the tag below, set its trigger to **All Pages**, and publish.

```html
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

Tag Manager keeps the tag's attributes when it injects it, which is what the script reads `data-site` from. Ad blockers that block Tag Manager block the script with it; adding the tag directly to the page avoids that.

## Other setups

- **Other tag managers.** The script reads `data-site` and the other attributes from its own `<script>` tag. If a tag manager injects it, make sure the injected tag keeps those attributes.
- **Several domains.** Add each domain as its own site and use its own `data-site`. Subdomains of a site are included automatically.
