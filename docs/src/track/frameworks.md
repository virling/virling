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

## Nuxt

Add the tag to the app head in the Nuxt config. Nuxt renders it into the HTML of every page, so the script can read its own attributes. Route changes are tracked through `history.pushState`, so client-side navigation is counted.

```ts,file:nuxt.config.ts
export default defineNuxtConfig({
  app: {
    head: {
      script: [
        { src: "https://stats.example.com/v.js", defer: true,
          "data-site": "yourdomain.com" },
      ],
    },
  },
});
```

Then [check that it works](script.md#check-that-it-works): open the published site and watch for the first pageview.

## SvelteKit

Paste the tag into `src/app.html`, before `%sveltekit.head%`. That file is the shell of every page, and client-side navigation is counted on its own.

```html,file:src/app.html
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

Then [check that it works](script.md#check-that-it-works): open the published site and watch for the first pageview.

## React (Vite)

Paste the tag into `index.html` at the project root, before `</head>`. Vite uses that file as the shell of the app and leaves an external script without `type="module"` as it is. Route changes from React Router are tracked through `history.pushState`, so client-side navigation is counted; see [Single-page apps](spa.md).

```html,file:index.html
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

To pass the visitor ID to a checkout you open from the browser, read it with `window.virling?.vid?.()`; see [Link checkout to visits](../payments/checkout.md#read-the-visitor-id).

## Vue (Vite)

Paste the tag into `index.html` at the project root, before `</head>`. Vite [treats that file as the entry point](https://vite.dev/guide/#index-html-and-project-root) of the app. Route changes from Vue Router are tracked through `history.pushState` with `createWebHistory()`; `createWebHashHistory()` keeps the route in the hash, which is not counted. See [Single-page apps](spa.md).

```html,file:index.html
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

To pass the visitor ID to a checkout you open from the browser, read it with `window.virling?.vid?.()`; see [Link checkout to visits](../payments/checkout.md#read-the-visitor-id).

## Angular

Paste the tag into `src/index.html`, before `</head>`. That file is [the main HTML page](https://angular.dev/reference/configs/file-structure) of the app. Angular's router uses `history.pushState` by default, so client-side navigation is counted; with the hash location strategy it is not. See [Single-page apps](spa.md).

```html,file:src/index.html
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

To pass the visitor ID to a checkout you open from the browser, read it with `window.virling?.vid?.()`; see [Link checkout to visits](../payments/checkout.md#read-the-visitor-id).

## Remix

Add the tag to the `<head>` in the root route, next to `<Meta />` and `<Links />`. Remix renders it into the HTML of every page, and client-side navigation is counted on its own; see [Single-page apps](spa.md).

```jsx,file:app/root.tsx
<head>
  <Meta />
  <Links />
  <script defer src="https://stats.example.com/v.js"
    data-site="yourdomain.com"></script>
</head>
```

To pass the visitor ID to checkout, read the `virling_vid` cookie from the request's `Cookie` header in the action that creates the checkout; see [Link checkout to visits](../payments/checkout.md).

## Gatsby

Add the tag to the head of every page with `setHeadComponents` in `gatsby-ssr.js`. Gatsby renders it into the HTML with its attributes, so the script can read `data-site`. Route changes are tracked through `history.pushState`, so client-side navigation is counted; see [Single-page apps](spa.md).

```jsx,file:gatsby-ssr.js
import React from "react";

export const onRenderBody = ({ setHeadComponents }) => {
  setHeadComponents([
    <script key="virling" defer src="https://stats.example.com/v.js"
      data-site="yourdomain.com"></script>,
  ]);
};
```

To pass the visitor ID to a checkout you open from the browser, read it with `window.virling?.vid?.()`; see [Link checkout to visits](../payments/checkout.md#read-the-visitor-id).

## Laravel

Paste the tag before `</head>` in the Blade layout every page extends, usually `resources/views/layouts/app.blade.php`.

```html,file:resources/views/layouts/app.blade.php
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

To pass the visitor ID to checkout, read it in the controller with `$request->cookie('virling_vid')`. Laravel encrypts cookies by default and returns nothing for one it did not set, so leave this one out: in Laravel 11 and later add `$middleware->encryptCookies(except: ['virling_vid']);` in `bootstrap/app.php`, and in older versions add `'virling_vid'` to `$except` in `app/Http/Middleware/EncryptCookies.php`. See [Link checkout to visits](../payments/checkout.md).

## Ruby on Rails

Paste the tag before `</head>` in `app/views/layouts/application.html.erb`. With Turbo, page visits use `history.pushState`, so each one is counted; see [Single-page apps](spa.md).

```html,file:app/views/layouts/application.html.erb
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

To pass the visitor ID to checkout, read `cookies[:virling_vid]` in the controller that creates the checkout, for example as `client_reference_id` on a Stripe Checkout Session. See [Link checkout to visits](../payments/checkout.md).

## Django

Paste the tag before `</head>` in the template every page extends, usually `templates/base.html`.

```html,file:templates/base.html
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

To pass the visitor ID to checkout, read [`request.COOKIES.get("virling_vid")`](https://docs.djangoproject.com/en/stable/ref/request-response/#django.http.HttpRequest.COOKIES) in the view that creates the checkout. It is a plain cookie, so do not use `get_signed_cookie()`. See [Link checkout to visits](../payments/checkout.md) and [Django Stripe UTM tracking](https://virling.com/blog/django-stripe-utm-tracking).

## Symfony

Paste the tag before `</head>` in `templates/base.html.twig`, outside any block a child template overrides.

```html,file:templates/base.html.twig
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

To pass the visitor ID to checkout, read `$request->cookies->get('virling_vid')` in the controller that creates the checkout; [`cookies` is the equivalent of `$_COOKIE`](https://symfony.com/doc/current/components/http_foundation.html#accessing-request-data). See [Link checkout to visits](../payments/checkout.md) and [Symfony Stripe UTM tracking](https://virling.com/blog/symfony-stripe-utm-tracking).

## Phoenix

Paste the tag before `</head>` in the root layout, `lib/my_app_web/components/layouts/root.html.heex`, with your app's name in place of `my_app`. Controllers and LiveViews [share it](https://hexdocs.pm/phoenix/components.html#layouts). LiveView navigation uses `history.pushState`, so each page is counted; see [Single-page apps](spa.md).

```html,file:lib/my_app_web/components/layouts/root.html.heex
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

To pass the visitor ID to checkout, read `get_cookies(conn)["virling_vid"]` in the controller that creates the checkout. The default `:browser` pipeline has already fetched the cookies. A LiveView cannot read cookies from its socket, so post a form to that controller. See [Link checkout to visits](../payments/checkout.md) and [Phoenix Stripe UTM tracking](https://virling.com/blog/phoenix-stripe-utm-tracking).

## WordPress

The Virling plugin adds the tag for you and survives theme updates. Download the `integrations/wordpress/virling` folder from the [repository](https://github.com/virling/virling/tree/main/integrations/wordpress/virling), upload it to `wp-content/plugins/`, activate it, then enter your Virling server and site domain in Settings, Virling. It skips logged-in administrators by default.

Without the plugin, open Appearance, Theme File Editor, open `header.php` and paste the tag before `</head>`.

```html,file:header.php
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

## Ghost

1. In Ghost Admin, open **Settings**, then **Code injection**.
2. Paste the tag into **Site header** and save. It is added to every page of the site.

```html
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

Then [check that it works](script.md#check-that-it-works): open the published site and watch for the first pageview.

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

## Squarespace

1. In Squarespace, open **Settings**, **Developer tools**, then **Code injection**. On older menus it is under **Settings**, **Advanced**.
2. Paste the tag into **Header** and save. Code injection needs a plan that includes it.

```html
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

Then [check that it works](script.md#check-that-it-works): open the published site and watch for the first pageview.

## Wix

1. In the Wix dashboard, open **Settings**, then **Custom code** under **Advanced**. Custom code needs a premium plan and a connected domain.
2. Click **Add Custom Code**, paste the tag, choose **All pages** and **Load code once**, and place it in **Head**.
3. Apply, then publish the site.

```html
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

Wix moves between pages without a full reload; the script counts each page change on its own.

Then [check that it works](script.md#check-that-it-works): open the published site and watch for the first pageview.

## Bubble

1. In the Bubble editor, open the **Settings** tab, then **SEO / metatags**.
2. Paste the tag into **Script/meta tags in header**. Bubble adds it to the `<head>` of every page of the app; its [SEO settings page](https://manual.bubble.io/core-resources/application-settings/seo-metatags) lists no plan limit for this field.
3. Deploy the app to live.

```html
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

Then [check that it works](script.md#check-that-it-works): open the live app and watch for the first pageview.

## Carrd

1. In the Carrd builder, click **Add Element**, then **Embed**.
2. Set **Type** to **Code** and paste the tag into **Code**.
3. Set **Style** to **Hidden** and choose **Head**, so the tag goes in the site's `<head>`.
4. Publish the site. Embeds output nothing in the builder, and they need Pro Standard or higher; see [Embedding custom code](https://carrd.com/docs/building/embedding-custom-code).

```html
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

Then [check that it works](script.md#check-that-it-works): open the published site and watch for the first pageview.

## Hugo

Find the template in your theme that holds `</head>`, usually `baseof.html` or a head partial under `themes/<theme>/layouts/`. Copy it to the same path in your project's own `layouts/` folder and paste the tag before `</head>`. Hugo [uses the project's file over the theme's](https://gohugo.io/templates/lookup-order/), so theme updates do not remove it.

```html,file:layouts/baseof.html
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

Rebuild and deploy, then [check that it works](script.md#check-that-it-works): open the published site and watch for the first pageview.

## Jekyll

Paste the tag before `</head>` in `_includes/head.html`. With a gem-based theme such as Minima the file is not in your site yet: copy it from the theme into your own `_includes` folder first, and Jekyll [uses your copy over the theme's](https://jekyllrb.com/docs/themes/#overriding-theme-defaults).

```html,file:_includes/head.html
<script defer src="https://stats.example.com/v.js"
  data-site="yourdomain.com"></script>
```

Rebuild and deploy, then [check that it works](script.md#check-that-it-works): open the published site and watch for the first pageview.

## Docusaurus

Add the tag to `scripts` in the site config. Docusaurus [puts each entry in the `<head>`](https://docusaurus.io/docs/api/docusaurus-config#scripts) of every page with the attributes you give it, so the script can read `data-site`. Route changes are tracked through `history.pushState`, so client-side navigation is counted.

```js,file:docusaurus.config.js
export default {
  scripts: [
    { src: "https://stats.example.com/v.js", defer: true,
      "data-site": "yourdomain.com" },
  ],
};
```

Rebuild and deploy, then [check that it works](script.md#check-that-it-works): open the published site and watch for the first pageview.

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
