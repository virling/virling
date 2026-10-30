# Admin panel

An operator panel at `/admin` lists accounts, sites, plans, signups, applications and server health, and lets you change a plan, grant access, suspend an account, sign in as a user and delete a site or account. Who is an admin is set in the database only. Anyone else, including signed-in users, gets a 404.

## Settings

| Variable | Flag | Default |
| --- | --- | --- |
| `VIRLING_APPLICATIONS_URL` | `--applications-url` | Unset: no applications listed |
| `VIRLING_APPLICATIONS_TOKEN` | `--applications-token` | Unset: no applications listed |

## Granting and revoking admin

Admin access lives in the `admins` table of `virling.sqlite`. It is empty by default, so the panel is off. Nothing in the app, API, connected apps, CLI or the panel itself can add, remove or change an admin, and an admin cannot change another admin. Only someone with access to the database file can, on the server, self-hosted or hosted.

Grant, with the email in lower case. The person then signs in with the normal sign-in link and opens `/admin`.

```sh
sqlite3 /var/lib/virling/virling.sqlite "INSERT INTO admins (email, created_at) VALUES ('you@example.com', strftime('%s', 'now'))"
```

Revoke:

```sh
sqlite3 /var/lib/virling/virling.sqlite "DELETE FROM admins WHERE email = 'you@example.com'"
```

Changes apply on the next request. The panel lists the current admins, read only. An admin cannot be suspended, signed in as or deleted from the panel, so remove the row first. `VIRLING_ADMINS` from earlier versions is ignored.

## What it does

- **Accounts and sites:** search by the start of an email or domain, with events from the last 24 hours per site.
- **Plan:** set an account to None, Paid, Founding or Free access. Paid, Founding and Free access unlock the paid features on a hosted server.
- **Suspend:** signs the account out and blocks sign-in, API keys and connected apps until you unsuspend it. Tracking continues.
- **Sign in as:** opens the user's app read only for 30 minutes under a banner with an Exit button. It is never available for another admin.
- **Delete:** a site or an account, with its stored events, after typing the domain or email. This cannot be undone.
- **Applications:** approve an applicant and email the invite from the same sender as sign-in emails, set by `VIRLING_SMTP_URL` and `VIRLING_MAIL_FROM` (see [SMTP](smtp.md)). Without SMTP the panel says email is not configured and sends nothing.
- **Audit log:** every action above records who did it, what, the target and when. The panel shows the latest 100.

Admin pages are never cached, changes are rate limited, and they accept changes only from the same origin with a per-session token. A suspended or support session cannot reach them. Secrets are never shown.

## Applications

Applications are stored by the website, not the app server. The website serves them at `GET /api/applications` with the `Authorization: Bearer` token set as its `ADMIN_TOKEN` secret. Point `VIRLING_APPLICATIONS_URL` at that URL, such as `https://virling.com/api/applications`, and `VIRLING_APPLICATIONS_TOKEN` at the same token. The list is fetched in the background and kept for a minute.
