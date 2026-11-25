# Sign-in email

Virling signs people in with a magic link. Set an SMTP server so the link is emailed.

## Settings

| Variable | Flag | Default |
| --- | --- | --- |
| `VIRLING_SMTP_URL` | `--smtp-url` | Unset: emails are written to the log |
| `VIRLING_MAIL_FROM` | `--mail-from` | `Virling <login@virling.com>` |

Set `VIRLING_MAIL_FROM` to an address on a domain your SMTP provider is allowed to send for. The default only works for virling.com itself.

## SMTP URL format

```sh
# Implicit TLS, usually port 465
VIRLING_SMTP_URL=smtps://USER:PASSWORD@smtp.example.com:465

# STARTTLS, usually port 587
VIRLING_SMTP_URL='smtp://USER:PASSWORD@smtp.example.com:587?tls=required'
```

URL-encode special characters in the user name and password, for example `@` as `%40`. Any SMTP service works, such as Amazon SES, Postmark, Resend, Mailgun or Cloudflare Email Service:

```sh
VIRLING_SMTP_URL=smtps://api_token:TOKEN@smtp.mx.cloudflare.net:465
```

## Without SMTP

When `VIRLING_SMTP_URL` is unset, nothing is sent. The full email, including the sign-in link, is logged at `info` level with "no SMTP configured, email not sent". Copy the link from the log:

```sh
docker logs virling 2>&1 | grep -A4 "email not sent"
journalctl -u virling | grep -A4 "email not sent"
```

This is fine for a single-person setup.

## The link

The link is valid once and for 15 minutes. After it is used, the session cookie `virling_session` lasts 30 days. Links point at `VIRLING_BASE_URL`, so set it to the public URL.

## Check that it works

Virling does not contact the SMTP server on start, so request a link to test it. Open `https://stats.example.com/login`, enter your email and look at the log:

```sh
journalctl -u virling -n 20
```

| What you see | What it means |
| --- | --- |
| "Check your email", and nothing new in the log | Your SMTP server accepted the email. A sent email is not logged. |
| "Check your email", and "no SMTP configured, email not sent" in the log | `VIRLING_SMTP_URL` is not set for the running server. The link is in the log. |
| "We couldn't send the email. Try again in a minute." | The log says "sending login email failed" with the reason. See [below](#why-is-the-email-not-sent). |

Without SMTP, the log shows the email like this:

```text
INFO virling::mail: no SMTP configured, email not sent:
Sign in to Virling as you@example.com:

https://stats.example.com/auth/verify?token=...
```

These commands read the journal of the [systemd](systemd.md) service. Started [by hand](binary.md), Virling prints the log in the terminal. With Docker, use `docker logs virling`.

## Common questions

### Why is the email not sent?

Search the log for the reason, which follows `err=`:

```sh
journalctl -u virling | grep "sending login email failed"
```

| Reason in the log | Cause and fix |
| --- | --- |
| "Connection refused" or "failed to lookup address information" | The host or port in `VIRLING_SMTP_URL` is wrong, or a firewall blocks it. |
| "received corrupt message of type InvalidContentType" | `smtps://` on a port that expects STARTTLS. Use `smtp://` with `?tls=required`, usually on port 587. |
| "STARTTLS is not supported on this server" | The server does not offer STARTTLS on that port. Use `smtps://`, usually on port 465. |
| "permanent error (535)" | The server refused the user name or password. URL-encode special characters in both. |
| "permanent error" about the sender, such as 550 or 553 | Set `VIRLING_MAIL_FROM` to an address the server lets you send from. |

Error codes and their text come from your SMTP server, so they vary. Restart Virling after changing a setting. If it then exits with an SMTP error, the URL is not valid: see [Why does it not start?](binary.md#why-does-it-not-start)

### Nothing arrives, and the log has no error. Why?

- **The log says "login link rate limit reached".** Each email gets 5 links an hour, and each IP address 5 in 10 minutes and 20 in an hour. Behind a proxy without `VIRLING_TRUST_PROXY=true`, everyone shares the proxy's address.
- **The SMTP server accepted the email.** Check the spam folder and your provider's delivery log, and that the domain in `VIRLING_MAIL_FROM` is verified there.
- **`RUST_LOG` is `warn` or `error`.** Without SMTP the link is logged at `info`, so it is hidden. Remove the setting or use `info`.

### Why do links point at the wrong address?

Links start with `VIRLING_BASE_URL`, which is `http://localhost:8080` when unset. Set it to the address people open, with `https://` and the port if there is one, and restart:

```sh
VIRLING_BASE_URL=https://stats.example.com
```

With [systemd](systemd.md#environment-file), it goes in `/etc/virling/virling.env`. Links that were already sent keep the old address, so request a new one. If confirming the link gives a 403, or returns to the sign-in page, the address in the browser does not match `VIRLING_BASE_URL`, for example `http` on one side and `https` on the other.

### Can I change the port of the SMTP server?

Yes, it is the number at the end of the host in `VIRLING_SMTP_URL`. It has nothing to do with the port Virling listens on, which is [`VIRLING_ADDR`](binary.md#how-do-i-change-the-port). Port 465 goes with `smtps://`, and 587 with `smtp://` and `?tls=required`.
