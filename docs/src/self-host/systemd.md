# systemd

The repo does not ship a systemd unit yet <span class="na">Not available yet</span>. This example runs a [release binary](../install/binary.md) installed at `/usr/local/bin/virling` as a `virling` user.

## Create the user and folder

```sh
sudo useradd --system --home-dir /var/lib/virling --shell /usr/sbin/nologin virling
sudo mkdir -p /var/lib/virling /etc/virling
sudo chown virling: /var/lib/virling
```

## Environment file

Keep settings and secrets out of the unit:

```ini,file:/etc/virling/virling.env
VIRLING_DATA=/var/lib/virling
VIRLING_ADDR=127.0.0.1:8080
VIRLING_BASE_URL=https://stats.example.com
VIRLING_TRUST_PROXY=true
VIRLING_SMTP_URL=smtps://user:pass@smtp.example.com:465
VIRLING_MAIL_FROM=Virling <stats@example.com>
```

```sh
sudo chown root:virling /etc/virling/virling.env
sudo chmod 640 /etc/virling/virling.env
```

## The unit

```ini,file:/etc/systemd/system/virling.service
[Unit]
Description=Virling
After=network-online.target
Wants=network-online.target

[Service]
User=virling
Group=virling
EnvironmentFile=/etc/virling/virling.env
ExecStart=/usr/local/bin/virling serve --app
KillSignal=SIGINT
Restart=on-failure
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/virling

[Install]
WantedBy=multi-user.target
```

`KillSignal=SIGINT` lets Virling shut down gracefully: it stops on Ctrl+C, the same signal. `ProtectHome=true` is fine because the data folder is outside `/home`.

## Start it

```sh
sudo systemctl daemon-reload
sudo systemctl enable --now virling
systemctl status virling
journalctl -u virling -f
```

Without [SMTP](smtp.md), sign-in links appear in `journalctl -u virling`.

## Add a site

Run the CLI as the same user, with the same data folder:

```sh
sudo -u virling env VIRLING_DATA=/var/lib/virling \
  virling add-site example.com --owner you@example.com
```

## Check that it works

1. **The service runs.** This prints `active`:

   ```sh
   systemctl is-active virling
   ```

2. **The log shows the right address and folder:**

   ```sh
   journalctl -u virling | grep "listening on"
   ```

   Each start has a line that ends like this:

   ```text
   INFO virling: listening on http://127.0.0.1:8080, data in /var/lib/virling
   ```

3. **The server answers.** Both print `ok`. The second one needs the [reverse proxy](reverse-proxy.md):

   ```sh
   curl -fsS http://127.0.0.1:8080/health
   curl -fsS https://stats.example.com/health
   ```

4. **You can sign in.** Open `https://stats.example.com/login` and enter the owner email. Then [check the email](smtp.md#check-that-it-works).

If the service does not start, `journalctl -u virling -n 20` ends with the reason. The common ones are listed under [Run the binary](binary.md#why-does-it-not-start).

## Common questions

### How do I change the port?

Change `VIRLING_ADDR` in `/etc/virling/virling.env`, for example to `127.0.0.1:9000`, and restart:

```sh
sudo systemctl restart virling
curl -fsS http://127.0.0.1:9000/health
```

Then point your [reverse proxy](reverse-proxy.md) at the new port. Change `VIRLING_BASE_URL` too if the port is part of your public address. The `virling` user cannot listen on ports below 1024, such as 80 or 443, so leave those to the proxy.

### Where are the logs?

In the journal:

```sh
journalctl -u virling -f
journalctl -u virling --since "1 hour ago"
```

To see more, add `RUST_LOG=debug` to `/etc/virling/virling.env` and restart. The default is `info`.

### How do I upgrade, and go back?

Read the [changelog](https://github.com/virling/virling/blob/main/CHANGELOG.md) and make a [backup](data.md#backups) first. Then run the [install script](../install/binary.md#install-script) again, or replace `/usr/local/bin/virling` yourself and restart. Migrations run on start.

```sh
curl -fsSL https://virling.com/install.sh | sh
```

The script keeps `/etc/virling/virling.env` but writes `virling.service` again. Put your own changes to the unit in a drop-in with `sudo systemctl edit virling`, which it does not touch.

To go back, stop the service, [restore the backup](data.md#restore-the-binarys-data-folder) without starting it, and install the version you had. The script starts it:

```sh
curl -fsSL https://virling.com/install.sh | sh -s -- --version X.Y.Z
```

Installing the older version alone is not enough: downgrading after a migration ran is not supported.

### Why do sign-in links point at the wrong address?

Links use `VIRLING_BASE_URL`. The environment file the install script creates does not set it, so links point at `http://localhost:8080` until you add it and restart. More under [Sign-in email](smtp.md#why-do-links-point-at-the-wrong-address). If no email arrives at all, see [Why is the email not sent?](smtp.md#why-is-the-email-not-sent)
