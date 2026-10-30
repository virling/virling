# virling

Run [Virling](https://virling.com), open-source revenue attribution, without installing anything else. It links website visits to payments from Stripe, Dodo Payments, Polar, LemonSqueezy, Creem and RevenueCat, so you see which post, search or AI answer brought each payment.

```sh
npx virling serve --app      # start a server
npx virling demo ./demo      # build the sample dashboard
npx virling --help
```

On first run it downloads the matching release binary from GitHub for Linux or macOS (x64 or arm64), checks its SHA-256, and keeps it in `~/.cache/virling`. Set `VIRLING_CACHE` to use another folder.

Docs: https://docs.virling.com. Hosted, with a 14-day free trial: https://app.virling.com.
