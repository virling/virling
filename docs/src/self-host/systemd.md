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

Without SMTP, sign-in links appear in `journalctl -u virling`.

## Add a site

Run the CLI as the same user, with the same data folder:

```sh
sudo -u virling env VIRLING_DATA=/var/lib/virling \
  virling add-site yourdomain.com --owner you@example.com
```
