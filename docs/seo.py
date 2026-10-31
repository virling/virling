"""Gives each built page a canonical URL and its own description, and writes sitemap.xml."""
import html, json, pathlib, re, sys

book = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "book")
site = "https://docs.virling.com/"
skip = {"404.html", "toc.html", "print.html"}
urls = []
llms = []
redirects = []
version = re.search(r'^version = "([^"]+)"', (pathlib.Path(__file__).resolve().parent.parent / "Cargo.toml").read_text(), re.M).group(1)


def clean(url):
    base, hash_, frag = url.partition("#")
    if not base.endswith(".html") or re.match(r"[a-z]+:", base) and not base.startswith(site):
        return url
    base = base[: -len(".html")]
    if base.endswith("index"):
        base = base[: -len("index")] or "./"
    return base + hash_ + frag

for f in sorted(book.rglob("*.html")):
    rel = f.relative_to(book).as_posix()
    if rel in skip:
        continue
    path = rel[: -len("index.html")] if rel.endswith("index.html") else rel[: -len(".html")]
    url = site + path
    urls.append(url)
    page = f.read_text()
    head = f'<meta name="description" content="'
    if rel != "index.html":
        m = re.search(r'id="content">.*?<p>(.*?)</p>', page, re.S)
        if m:
            text = " ".join(html.unescape(re.sub(r"<[^>]+>", "", m.group(1))).split())
            if len(text) > 160:
                text = text[:157].rsplit(" ", 1)[0].rstrip(",;:") + "..."
            page = re.sub(head + r'[^"]*">', lambda _: head + html.escape(text) + '">', page, count=1)
    root = re.search(r'var path_to_root = "([^"]*)";', page)
    if root:
        fix = lambda m: m.group(1) + (m.group(2) if re.match(r"[a-z]+:|/|#", m.group(2)) else root.group(1) + m.group(2)) + '"'
        page = re.sub(r'(<nav class="side"[^>]*?)>(.*?)</nav>', lambda n: re.sub(r'(href=")([^"]*)"', fix, n.group(1) + " data-abs>" + n.group(2) + "</nav>"), page, count=1, flags=re.S)
    page = re.sub(r'(<a [^>]*?href=")([^"]*)"', lambda m: m.group(1) + clean(m.group(2)) + '"', page)
    page = page.replace("VIRLING_VERSION", version)
    title = html.unescape(re.search(r"<title>(.*?)</title>", page, re.S).group(1).strip())
    desc = html.unescape(re.search(r'<meta name="description" content="([^"]*)">', page).group(1))
    graph = [{"@type": "TechArticle", "headline": title, "description": desc, "url": url, "isPartOf": {"@type": "WebSite", "name": "Virling docs", "url": site}, "publisher": {"@type": "Organization", "@id": "https://virling.com/#org", "name": "Virling", "url": "https://virling.com/"}}]
    faq = re.findall(r"<h2[^>]*>(.*?)</h2>\s*<p>(.*?)</p>", page, re.S) if rel == "faq.html" else []
    if faq:
        text = lambda h: " ".join(html.unescape(re.sub(r"<[^>]+>", "", h)).split())
        graph.append({"@type": "FAQPage", "mainEntity": [{"@type": "Question", "name": text(q), "acceptedAnswer": {"@type": "Answer", "text": text(a)}} for q, a in faq]})
    ld = json.dumps({"@context": "https://schema.org", "@graph": graph}, ensure_ascii=False).replace("</", "<\\/")
    og = "".join(f'<meta property="og:{k}" content="{html.escape(v)}">\n' for k, v in [("type", "article"), ("site_name", "Virling docs"), ("url", url), ("title", title), ("description", desc)])
    page = page.replace("</head>", f'<link rel="canonical" href="{url}">\n{og}<meta name="twitter:card" content="summary">\n<script type="application/ld+json">{ld}</script>\n</head>', 1)
    f.write_text(page)
    src = book.parent / "src" / (rel[: -len(".html")] + ".md")
    if src.exists():
        (book / (rel[: -len(".html")] + ".md")).write_text(src.read_text())
    llms.append(f"- [{title}]({url}): {desc}")
    if rel != "index.html":
        redirects.append(f"/{rel} /{path} 301")

for f in book.glob("searchindex*.js*"):
    f.write_text(f.read_text().replace("VIRLING_VERSION", version))

rows = "".join(f"  <url><loc>{u}</loc></url>\n" for u in urls)
(book / "sitemap.xml").write_text(
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n' + rows + "</urlset>\n"
)
print(f"{len(urls)} pages in sitemap.xml")
(book / "llms.txt").write_text(
    "# Virling docs\n\n> How to run Virling, open-source revenue attribution, add the tracking script, connect payments and read attribution. Product site: https://virling.com/llms.txt\n\n"
    + "\n".join(llms) + "\n"
)
# Relative links followed from a wrong URL nest real paths under junk prefixes; send them home.
deep = [r.split()[0] for r in redirects]
splats = [f"/*{src} /{src[1:-5].removesuffix('index')} 301" for src in sorted(deep, key=lambda x: -x.count("/"))]
(book / "_redirects").write_text("\n".join(redirects + splats) + "\n")
