# Troubleshooting

The problems people hit most, with what to check first. On hosted Virling, still stuck? Open **Help and support** from the account menu in the app, or email help@virling.com. Self-hosted: open an issue on [GitHub](https://github.com/virling/virling/issues).

## No visitors show up

1. Open your site, then your browser's network tab. You should see a request to `/api/event` on your Virling server answered with `202`.
2. No request at all: the script tag is missing on that page, or an ad blocker stopped it. Try a private window with extensions off.
3. A request answered with anything but `202`: check that `data-site` matches the domain exactly as you added it, without `www.` or a path. See [Add the script](track/script.md).
4. Your own site sends a `Content-Security-Policy`? Allow the Virling server in `script-src` and `connect-src`.
5. The script stays quiet on `localhost` unless the tag has `data-dev`.

The setup checklist on the dashboard turns **Script installed** green with the first event.

## Payments show as Unattributed

A payment is Unattributed when no visitor ID reached checkout, or the visitor's sessions are outside the lookback window. The usual fix is to pass `virling_vid` into checkout: see [Link checkout to visits](payments/checkout.md). Every cause is listed in [Unattributed revenue](attribution/unattributed.md).

## Payments do not appear at all

1. In Site settings, check that the provider shows as connected and when it last synced.
2. A refused key shows its error there. Create a new read-only key with the permissions on the provider's page under [Payments](payments/index.md) and save it again.
3. Webhook providers (LemonSqueezy, Creem, RevenueCat) need the webhook URL from Site settings added on the provider's side, and Virling must be reachable over https.

## Checkout is not linked

The setup checklist shows **Link checkout** until a payment arrives with a visitor ID. Open the payment on the Payments page: if it lists no visitor ID, checkout did not carry `virling_vid`. Follow [Link checkout to visits](payments/checkout.md) for your provider, then make a test purchase.

## Event limits on hosted plans

Personal includes 100,000 events a month and Agency 1,000,000. At 80% and at 100% the account owner gets an email and the app shows a banner. Virling never stops collecting and never drops data when you go over. If you keep going over, move to a larger plan from Account settings.

## Numbers differ from another tool

Different tools count visitors, sessions and bots differently, and credit payments with different models. See [How attribution works](attribution/index.md) and [Model and lookback window](attribution/model.md).
