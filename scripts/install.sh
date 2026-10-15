#!/bin/sh
# Installs the latest Virling release: curl -fsSL https://virling.com/install.sh | sh
set -eu

REPO="virling/virling"
VERSION="${VIRLING_VERSION:-}"
DOWNLOAD_URL="${VIRLING_DOWNLOAD_URL:-https://github.com/$REPO/releases/download}"
BIN_DIR="${VIRLING_INSTALL_DIR:-$HOME/.local/bin}"
SYSTEM=0
SYSTEMD=ask
DRY_RUN=0

usage() {
	cat <<EOF
Usage: install.sh [options]

  --system         Install to /usr/local/bin with sudo
  --dir DIR        Install to DIR (default: ~/.local/bin)
  --version X.Y.Z  Install this version instead of the latest
  --systemd        Also set up a systemd service (Linux, needs --system)
  --no-systemd     Do not ask about a systemd service
  --dry-run        Print what would happen without changing anything
  -h, --help       Show this help

Pass options through the pipe with: curl -fsSL https://virling.com/install.sh | sh -s -- --system
EOF
}

say() { printf '%s\n' "$*"; }
err() {
	printf 'virling install: %s\n' "$*" >&2
	exit 1
}
has() { command -v "$1" >/dev/null 2>&1; }

while [ $# -gt 0 ]; do
	case "$1" in
	--system) SYSTEM=1 BIN_DIR=/usr/local/bin ;;
	--dir)
		[ $# -ge 2 ] || err "--dir needs a value"
		BIN_DIR="$2"
		shift
		;;
	--version)
		[ $# -ge 2 ] || err "--version needs a value"
		VERSION="$2"
		shift
		;;
	--systemd) SYSTEMD=yes ;;
	--no-systemd) SYSTEMD=no ;;
	--dry-run) DRY_RUN=1 ;;
	-h | --help)
		usage
		exit 0
		;;
	*) err "unknown option: $1 (see --help)" ;;
	esac
	shift
done

run() {
	if [ "$DRY_RUN" = 1 ]; then
		say "+ $*"
	else
		"$@"
	fi
}

[ "$SYSTEMD" = yes ] && [ "$SYSTEM" != 1 ] && err "--systemd needs --system so the service runs the binary from /usr/local/bin"

SUDO=""
if [ "$SYSTEM" = 1 ] && [ "$(id -u)" != 0 ]; then
	has sudo || err "--system needs root or sudo"
	SUDO="sudo"
fi

case "$(uname -s)" in
Linux) os=unknown-linux-gnu ;;
Darwin) os=apple-darwin ;;
*) err "unsupported OS $(uname -s); use Docker or build from source" ;;
esac
case "$(uname -m)" in
x86_64 | amd64) arch=x86_64 ;;
aarch64 | arm64) arch=aarch64 ;;
*) err "unsupported CPU $(uname -m); use Docker or build from source" ;;
esac
# A shell running under Rosetta reports x86_64 on Apple silicon.
if [ "$os" = apple-darwin ] && [ "$arch" = x86_64 ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null)" = 1 ]; then
	arch=aarch64
fi
TARGET="$arch-$os"

if has curl; then
	fetch() { curl -fsSL --proto '=https,file' -o "$2" "$1"; }
elif has wget; then
	fetch() { wget -q -O "$2" "$1"; }
else
	err "needs curl or wget"
fi

if [ -z "$VERSION" ]; then
	has curl || err "needs curl to find the latest version; set VIRLING_VERSION instead"
	latest=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest") ||
		err "could not reach GitHub to find the latest release"
	VERSION="${latest##*/}"
fi
VERSION="${VERSION#v}"
case "$VERSION" in
[0-9]*.[0-9]*.[0-9]*) ;;
*) err "no release found (got '$VERSION')" ;;
esac

NAME="virling-v$VERSION-$TARGET"
URL="$DOWNLOAD_URL/v$VERSION/$NAME.tar.gz"
say "Installing Virling $VERSION for $TARGET into $BIN_DIR"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT INT TERM

fetch "$URL" "$TMP/$NAME.tar.gz" || err "download failed: $URL"
fetch "$URL.sha256" "$TMP/$NAME.tar.gz.sha256" || err "download failed: $URL.sha256"

want=$(cut -d' ' -f1 <"$TMP/$NAME.tar.gz.sha256")
if has sha256sum; then
	got=$(sha256sum "$TMP/$NAME.tar.gz" | cut -d' ' -f1)
elif has shasum; then
	got=$(shasum -a 256 "$TMP/$NAME.tar.gz" | cut -d' ' -f1)
else
	err "needs sha256sum or shasum to verify the download"
fi
[ -n "$want" ] && [ "$want" = "$got" ] || err "checksum mismatch for $NAME.tar.gz (expected $want, got $got)"
say "Checksum OK"

tar xzf "$TMP/$NAME.tar.gz" -C "$TMP"
[ -f "$TMP/$NAME/virling" ] || err "archive does not contain $NAME/virling"

run $SUDO mkdir -p "$BIN_DIR"
run $SUDO install -m 755 "$TMP/$NAME/virling" "$BIN_DIR/virling"
say "Installed $BIN_DIR/virling"

setup_systemd() {
	unit=/etc/systemd/system/virling.service
	envfile=/etc/virling/virling.env
	if ! id virling >/dev/null 2>&1; then
		run $SUDO useradd --system --home-dir /var/lib/virling --shell /usr/sbin/nologin virling
	fi
	run $SUDO mkdir -p /var/lib/virling /etc/virling
	run $SUDO chown virling: /var/lib/virling
	if [ -e "$envfile" ]; then
		say "Keeping existing $envfile"
	else
		printf 'VIRLING_DATA=/var/lib/virling\nVIRLING_ADDR=0.0.0.0:8080\n' >"$TMP/virling.env"
		run $SUDO install -m 640 -o root -g virling "$TMP/virling.env" "$envfile"
	fi
	cat >"$TMP/virling.service" <<EOF
[Unit]
Description=Virling
After=network-online.target
Wants=network-online.target

[Service]
User=virling
Group=virling
EnvironmentFile=$envfile
ExecStart=$BIN_DIR/virling serve --app
KillSignal=SIGINT
Restart=on-failure
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/virling

[Install]
WantedBy=multi-user.target
EOF
	run $SUDO install -m 644 "$TMP/virling.service" "$unit"
	run $SUDO systemctl daemon-reload
	run $SUDO systemctl enable --now virling
	say ""
	say "Virling is running as a systemd service on port 8080."
	say "Set VIRLING_BASE_URL in $envfile, then: sudo systemctl restart virling"
	say "Logs, including sign-in links without SMTP: journalctl -u virling -f"
}

if [ "$os" = unknown-linux-gnu ] && [ "$SYSTEM" = 1 ] && [ "$SYSTEMD" != no ] && has systemctl && [ -d /run/systemd/system ]; then
	if [ "$SYSTEMD" = ask ] && (: </dev/tty) 2>/dev/null; then
		printf 'Set up and start a systemd service for Virling? [y/N] '
		read -r answer </dev/tty || answer=""
		case "$answer" in
		[yY]*) SYSTEMD=yes ;;
		esac
	fi
	if [ "$SYSTEMD" = yes ]; then
		setup_systemd
		exit 0
	fi
elif [ "$SYSTEMD" = yes ]; then
	say "systemd was not found, so no service was set up"
fi

say ""
case ":$PATH:" in
*":$BIN_DIR:"*) cmd=virling ;;
*)
	cmd="$BIN_DIR/virling"
	say "$BIN_DIR is not on your PATH. Add it with: export PATH=\"$BIN_DIR:\$PATH\""
	;;
esac
if [ "$os" = unknown-linux-gnu ] && [ "$SYSTEM" != 1 ] && has systemctl; then
	say "To run it as a systemd service instead, rerun with: sh -s -- --system --systemd"
fi
say "Start Virling with:"
say ""
say "  $cmd serve --app"
say ""
say "Then sign in at http://localhost:8080/login. Docs: https://docs.virling.com"
