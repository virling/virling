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
