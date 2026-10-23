# Admin panel

An operator panel at `/admin` lists accounts, sites, plans, signups, beta applications and server health, and lets you change a plan, grant access, suspend an account, sign in as a user and delete a site or account. It is off until you set `VIRLING_ADMINS`. Anyone else, including signed-in users, gets a 404.

## Settings

| Variable | Flag | Default |
| --- | --- | --- |
| `VIRLING_ADMINS` | `--admins` | Unset: no admin panel |
| `RESEND_API_KEY` | `--resend-key` | Unset: beta invites cannot be sent |
| `VIRLING_APPLICATIONS_URL` | `--applications-url` | Unset: no beta applications listed |
| `VIRLING_APPLICATIONS_TOKEN` | `--applications-token` | Unset: no beta applications listed |

`VIRLING_ADMINS` is a comma separated list of emails, such as `you@example.com,partner@example.com`. Admins sign in with the normal sign-in link and then open `/admin`.

## What it does

- **Accounts and sites:** search by the start of an email or domain, with events from the last 24 hours per site.
- **Plan:** set an account to None, Paid, Founding or Free access. Paid, Founding and Free access unlock the paid features on a hosted server.
- **Suspend:** signs the account out and blocks sign-in, API keys and connected apps until you unsuspend it. Tracking continues.
- **Sign in as:** opens the user's app read only for 30 minutes under a banner with an Exit button. It is never available for another admin.
- **Delete:** a site or an account, with its stored events, after typing the domain or email. This cannot be undone.
- **Beta:** approve an applicant and email the invite through the [Resend](https://resend.com) API with `RESEND_API_KEY`.
- **Audit log:** every action above records who did it, what, the target and when. The panel shows the latest 100.

Admin pages are never cached, are rate limited, and accept changes only from the same origin with a per-session token. A suspended or support session cannot reach them. Secrets are never shown.

## Beta applications

Applications are stored by the website, not the app server. The website serves them at `GET /api/applications` with the `Authorization: Bearer` token set as its `ADMIN_TOKEN` secret. Point `VIRLING_APPLICATIONS_URL` at that URL, such as `https://virling.com/api/applications`, and `VIRLING_APPLICATIONS_TOKEN` at the same token. The list is fetched in the background and kept for a minute.
