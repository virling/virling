"""Gives each built page a canonical URL and its own description, and writes sitemap.xml."""
import html, pathlib, re, sys

book = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "book")
site = "https://docs.virling.com/"
skip = {"404.html", "toc.html", "print.html"}
urls = []

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
    page = page.replace("</head>", f'<link rel="canonical" href="{url}">\n</head>', 1)
    f.write_text(page)

rows = "".join(f"  <url><loc>{u}</loc></url>\n" for u in urls)
(book / "sitemap.xml").write_text(
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n' + rows + "</urlset>\n"
)
print(f"{len(urls)} pages in sitemap.xml")
