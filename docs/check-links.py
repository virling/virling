"""Fails when a page in the built book links to a missing page or anchor."""
import html.parser, pathlib, sys, urllib.parse

book = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "book")


class Page(html.parser.HTMLParser):
    def __init__(self):
        super().__init__()
        self.ids, self.links, self.side = set(), [], 0

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if "id" in a:
            self.ids.add(a["id"])
        if tag == "nav" and (self.side or a.get("id") == "side"):
            self.side += 1
        for key in ("href", "src"):
            if a.get(key):
                # The sidebar's links are relative to the book root; the theme prefixes them at runtime.
                self.links.append((a[key], self.side > 0))

    def handle_endtag(self, tag):
        if tag == "nav" and self.side:
            self.side -= 1


pages = {}
for f in book.rglob("*.html"):
    p = Page()
    p.feed(f.read_text())
    pages[f.resolve()] = p

bad = []
for f, page in pages.items():
    if f.name in ("toc.html", "404.html"):
        continue
    for link, from_root in page.links:
        url = urllib.parse.urlparse(link)
        if url.scheme or link.startswith(("//", "mailto:")):
            continue
        base = book.resolve() if from_root else f.parent
        target = (base / urllib.parse.unquote(url.path)).resolve() if url.path else f
        if target.is_dir():
            target = target / "index.html"
        if not target.exists():
            bad.append(f"{f.relative_to(book.resolve())}: {link} (missing file)")
        elif url.fragment and target in pages and url.fragment not in pages[target].ids:
            bad.append(f"{f.relative_to(book.resolve())}: {link} (missing anchor)")

print("\n".join(bad) or f"{len(pages)} pages, all internal links resolve")
sys.exit(1 if bad else 0)
