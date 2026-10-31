#!/bin/sh
# Measures Virling on 1 vCPU and 1 GB RAM: a month of pageviews through the event API, then dashboard reads.
# Linux with Docker and ApacheBench (ab); run from the repository root after `cargo build --release`.
set -eu
BIN="$(pwd)/target/release/virling"
IMAGE=ubuntu:24.04
EVENTS="${EVENTS:-100000}"
SCALE="${SCALE:-1}"
# The base image has no CA certificates and the server needs them at startup.
LIMITS="--cpus 1 --memory 1g --memory-swap 1g -v /etc/ssl/certs:/etc/ssl/certs:ro"
work=$(mktemp -d)
trap 'docker rm -f virling-load >/dev/null 2>&1 || true' EXIT
run() {
  dir=$1
  shift
  docker run --rm $LIMITS -v "$BIN:/usr/local/bin/virling:ro" -v "$dir:/data" -e VIRLING_DATA=/data "$IMAGE" "$@"
}

echo "== Ingest: $EVENTS pageviews, 16 at a time"
mkdir "$work/ingest"
run "$work/ingest" virling add-site example.com --owner load@example.com >/dev/null
docker run -d --name virling-load $LIMITS -p 18080:8080 -v "$BIN:/usr/local/bin/virling:ro" \
  -v "$work/ingest:/data" -e VIRLING_DATA=/data -e VIRLING_EVENTS_PER_SECOND=100000 "$IMAGE" \
  virling serve >/dev/null
timeout 60 sh -c 'until curl -sf http://127.0.0.1:18080/health >/dev/null; do sleep 0.5; done' || { docker logs virling-load; exit 1; }
printf '{"n":"pageview","u":"https://example.com/pricing","r":"https://news.ycombinator.com/","s":"example.com","w":1440}' > "$work/event.json"
timeout 900 ab -q -n "$EVENTS" -c 16 -p "$work/event.json" -T text/plain http://127.0.0.1:18080/api/event \
  | grep -E "Complete requests|Failed requests|Requests per second|Time per request.*\(mean\)|  50%|  99%|100%"
docker stats --no-stream --format "memory after ingest: {{.MemUsage}}" virling-load
docker rm -f virling-load >/dev/null

echo "== Reads: every dashboard route on the demo data at ${SCALE}x, without the result cache"
mkdir "$work/reads"
timeout 1800 docker run --rm $LIMITS -v "$BIN:/usr/local/bin/virling:ro" -v "$work/reads:/data" -e VIRLING_DATA=/data "$IMAGE" virling bench --scale "$SCALE" --runs 5 --no-cache
if python3 -c "import duckdb" 2>/dev/null; then
  python3 - "$work/reads/events.duckdb" <<'PY'
import sys, duckdb
c = duckdb.connect(sys.argv[1], read_only=True)
total, month = c.execute(
    "SELECT count(*), count(*) FILTER (WHERE ts > now() - INTERVAL 30 DAY) FROM events WHERE kind = 'pageview'"
).fetchone()
print(f"reads data: {total} pageviews in all, {month} in the last 30 days")
PY
fi
