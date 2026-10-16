# Framework guides

The same tag works everywhere. These are the snippets from the app's setup screen. Replace `stats.example.com` with your Virling server and `yourdomain.com` with your site.

## HTML

Before `</head>` on every page:

```html
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

## Next.js

In the App Router, put it in the root layout so it loads on every route. Route changes are tracked through `history.pushState`, so client-side navigation is counted.

```jsx,file:app/layout.tsx
import Script from "next/script";

export default function RootLayout({ children }) {
  return (
    <html lang="en">
      <head>
        <Script defer src="https://stats.example.com/v.js"
          data-site="yourdomain.com" />
      </head>
      <body>{children}</body>
    </html>
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

Site settings, General, Custom code, then "End of `<head>` tag". Publish the site afterwards.

## Webflow

Site settings, Custom code, then Head code. Publish the site afterwards.

```html
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

## Other setups

- **Tag managers.** The script reads `data-site` and the other attributes from its own `<script>` tag. If a tag manager injects it, make sure the injected tag keeps those attributes.
- **Several domains.** Add each domain as its own site and use its own `data-site`. Subdomains of a site are included automatically.
