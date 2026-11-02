"""Submits the pages whose source changed to IndexNow, or the whole sitemap when the change range is unknown."""
import argparse, json, pathlib, re, subprocess, urllib.request

ap = argparse.ArgumentParser()
ap.add_argument("--site", required=True, help="e.g. https://virling.com")
ap.add_argument("--src", required=True, help="directory whose .html or .md files are the pages")
ap.add_argument("--home", default="index", help="source file name of the home page, without extension")
ap.add_argument("--key-file", required=True)
ap.add_argument("--before", default="", help="commit before the push; empty means submit the whole sitemap")
ap.add_argument("--dry-run", action="store_true", help="print the payload instead of sending it")
args = ap.parse_args()

site = args.site.rstrip("/")
key_file = pathlib.Path(args.key_file)
key = key_file.read_text().strip()
req = urllib.request.Request(site + "/sitemap.xml", headers={"User-Agent": "virling-indexnow"})
sitemap = re.findall(r"<loc>([^<]+)</loc>", urllib.request.urlopen(req, timeout=30).read().decode())


def url(path):
    page = re.sub(r"\.(html|md)$", "", path.relative_to(args.src).as_posix())
    if page == args.home:
        return site + "/"
    return site + "/" + (page[: -len("index")] if page.endswith("/index") else page)


known = args.before and not re.fullmatch(r"0+", args.before) and subprocess.run(["git", "cat-file", "-e", args.before + "^{commit}"], capture_output=True).returncode == 0
if known:
    diff = subprocess.run(["git", "diff", "--name-only", args.before, "HEAD", "--", args.src], capture_output=True, text=True, check=True).stdout.split()
    changed = {url(pathlib.Path(f)) for f in diff if re.search(r"\.(html|md)$", f)}
    urls = [u for u in sitemap if u in changed]
else:
    print("Change range unknown, submitting the whole sitemap")
    urls = sitemap

if not urls:
    print("No pages in the sitemap changed, nothing to submit")
for i in range(0, len(urls), 10000):
    body = json.dumps({"host": site.split("://", 1)[1], "key": key, "keyLocation": f"{site}/{key_file.name}", "urlList": urls[i : i + 10000]}, indent=2)
    if args.dry_run:
        print(body)
        continue
    req = urllib.request.Request("https://api.indexnow.org/indexnow", data=body.encode(), method="POST", headers={"Content-Type": "application/json; charset=utf-8"})
    with urllib.request.urlopen(req, timeout=30) as res:
        print(f"IndexNow {res.status}: submitted {len(urls[i : i + 10000])} URLs")
