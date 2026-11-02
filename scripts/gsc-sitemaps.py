"""Resubmits sitemaps to Google Search Console with a service account key, or skips when no key is set."""
import base64, json, os, subprocess, sys, tempfile, time, urllib.parse, urllib.request

raw = os.environ.get("GSC_SERVICE_ACCOUNT_JSON", "")
if not raw:
    print("::notice::GSC_SERVICE_ACCOUNT_JSON is not set, skipping the Search Console sitemap submit")
    sys.exit(0)
dry_run = "--dry-run" in sys.argv
sitemaps = [a for a in sys.argv[1:] if a != "--dry-run"]
prop = os.environ.get("GSC_PROPERTY") or "sc-domain:virling.com"
account = json.loads(raw)

b64 = lambda b: base64.urlsafe_b64encode(b).rstrip(b"=").decode()
now = int(time.time())
claims = {"iss": account["client_email"], "scope": "https://www.googleapis.com/auth/webmasters", "aud": "https://oauth2.googleapis.com/token", "iat": now, "exp": now + 3600}
unsigned = b64(b'{"alg":"RS256","typ":"JWT"}') + "." + b64(json.dumps(claims).encode())
with tempfile.NamedTemporaryFile("w", suffix=".pem") as pem:
    pem.write(account["private_key"])
    pem.flush()
    sig = subprocess.run(["openssl", "dgst", "-sha256", "-sign", pem.name], input=unsigned.encode(), capture_output=True, check=True).stdout
form = urllib.parse.urlencode({"grant_type": "urn:ietf:params:oauth:grant-type:jwt-bearer", "assertion": unsigned + "." + b64(sig)}).encode()
token = "" if dry_run else json.load(urllib.request.urlopen("https://oauth2.googleapis.com/token", data=form, timeout=30))["access_token"]

for sitemap in sitemaps:
    api = f"https://searchconsole.googleapis.com/webmasters/v3/sites/{urllib.parse.quote(prop, safe='')}/sitemaps/{urllib.parse.quote(sitemap, safe='')}"
    if dry_run:
        print(f"Would PUT {api}")
        continue
    req = urllib.request.Request(api, data=b"", method="PUT", headers={"Authorization": f"Bearer {token}"})
    with urllib.request.urlopen(req, timeout=30) as res:
        print(f"Search Console {res.status}: resubmitted {sitemap} to {prop}")
