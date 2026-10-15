(function () {
  "use strict";
  var d = document, root = d.documentElement, doc = d.querySelector(".doc");
  var REPO = "https://github.com/virling/virling";

  function svg(paths, size, sw) {
    return '<svg width="' + size + '" height="' + size + '" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="' + (sw || 1.9) +
      '" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' + paths + "</svg>";
  }
  var I = {
    copy: '<rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15V6a2 2 0 0 1 2-2h8"/>',
    check: '<path d="M5 12l5 5L20 7"/>',
    info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v5M12 8h.01"/>',
    warn: '<path d="M12 3l9.5 17h-19z"/><path d="M12 10v4M12 17h.01"/>',
    hash: '<path d="M5 9h14M5 15h14M10 4L8 20M16 4l-2 16"/>',
    chev: '<path d="M9 6l6 6-6 6"/>',
    doc: '<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/>',
    menu: '<path d="M4 7h16M4 12h16M4 17h16"/>',
    down: '<path d="M6 9l6 6 6-6"/>'
  };
  function esc(s) {
    return s.replace(/[&<>"]/g, function (c) { return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]; });
  }
  function store(k, v) {
    try { if (v === undefined) return localStorage.getItem(k); localStorage.setItem(k, v); } catch (e) { return null; }
  }

  var mac = /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
  d.querySelectorAll("[data-mod-k]").forEach(function (k) { k.textContent = mac ? "⌘K" : "Ctrl K"; });

  d.querySelectorAll("[data-theme-toggle]").forEach(function (b) {
    b.addEventListener("click", function () {
      var t = root.classList.contains("dark") ? "light" : "dark";
      root.className = t;
      store("virling-docs-theme", t);
    });
  });
  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", function (e) {
    if (!store("virling-docs-theme")) root.className = e.matches ? "dark" : "light";
  });

  var scrim = d.querySelector(".shell>.scrim"), drawerBtn = d.querySelector("[data-drawer-open]");
  function drawer(open) {
    d.body.classList.toggle("drawer-open", open);
    scrim.hidden = !open;
    drawerBtn.setAttribute("aria-expanded", String(open));
    if (open) { var on = d.querySelector(".side a.on"); if (on) on.scrollIntoView({ block: "center" }); }
  }
  drawerBtn.addEventListener("click", function () { drawer(true); });
  d.querySelectorAll("[data-drawer-close]").forEach(function (b) { b.addEventListener("click", function () { drawer(false); }); });

  // Shell blocks: prompts, comments, strings, flags and VIRLING_ variables.
  var SH = /(^|\s)(#[^\n]*)|("[^"\n]*")|(\s--?[a-z][\w-]*)|(\bVIRLING_[A-Z_]+\b)/g;
  function shell(code) {
    var lines = code.textContent.replace(/\n$/, "").split("\n");
    var prompts = lines.some(function (l) { return l.indexOf("$ ") === 0; }), cont = false;
    code.innerHTML = lines.map(function (line) {
      if (prompts && line.indexOf("$ ") !== 0 && !cont) return '<span class="t-o">' + esc(line) + "</span>";
      cont = /\\$/.test(line);
      var out = "", pos = 0, m, body = line, pre = "";
      if (line.indexOf("$ ") === 0) { pre = '<span class="t-c">$ </span>'; body = line.slice(2); }
      SH.lastIndex = 0;
      while ((m = SH.exec(body))) {
        var cls = m[2] ? "t-c" : m[3] ? "t-s" : m[4] ? "t-f" : "t-v", tok = m[2] || m[3] || m[4] || m[5];
        var start = m.index + (m[2] ? m[1].length : 0);
        out += esc(body.slice(pos, start)) + '<span class="' + cls + '">' + esc(tok) + "</span>";
        pos = start + tok.length;
      }
      return pre + out + esc(body.slice(pos));
    }).join("\n");
  }
  function lang(code) {
    var m = /language-(\S+)/.exec(code.className);
    return m ? m[1] : "";
  }
  function label(code) {
    var f = /(?:^|\s)file:(\S+)/.exec(code.className);
    if (f) return f[1];
    return { sh: "terminal", bash: "terminal", shell: "terminal", console: "terminal", html: "HTML", js: "JavaScript", javascript: "JavaScript",
      ts: "TypeScript", tsx: "TSX", jsx: "JSX", json: "JSON", ini: "INI", nginx: "nginx", yaml: "YAML", http: "HTTP", php: "PHP" }[lang(code)] || "";
  }
  function highlight(code) {
    var l = lang(code);
    if (/^(sh|bash|shell|console)$/.test(l)) return shell(code);
    var map = { html: "xml", js: "javascript", ts: "typescript", tsx: "typescript", jsx: "javascript", caddy: "" };
    l = l in map ? map[l] : l;
    if (l && window.hljs && hljs.getLanguage(l)) {
      code.classList.add("language-" + l);
      hljs.highlightBlock(code);
    }
  }
  function copyText(pre) {
    var lines = pre.textContent.replace(/\n$/, "").split("\n");
    if (!lines.some(function (l) { return l.indexOf("$ ") === 0; })) return lines.join("\n");
    var out = [], cont = false;
    lines.forEach(function (l) {
      if (l.indexOf("$ ") === 0) { out.push(l.slice(2)); cont = /\\$/.test(l); }
      else if (cont) { out.push(l); cont = /\\$/.test(l); }
    });
    return out.join("\n");
  }
  function copyButton(getPre) {
    var b = d.createElement("button");
    b.type = "button";
    b.className = "copy";
    b.setAttribute("aria-label", "Copy code");
    b.innerHTML = '<span class="i-copy">' + svg(I.copy, 14) + '</span><span class="i-ok">' + svg(I.check, 14, 2.2) + "</span><span>Copy</span>";
    var text = b.lastChild, timer;
    b.addEventListener("click", function () {
      var done = function () {
        b.classList.add("ok");
        text.textContent = "Copied";
        clearTimeout(timer);
        timer = setTimeout(function () { b.classList.remove("ok"); text.textContent = "Copy"; }, 1600);
      };
      var t = copyText(getPre());
      if (navigator.clipboard) navigator.clipboard.writeText(t).then(done, function () { fallback(t); done(); });
      else { fallback(t); done(); }
    });
    return b;
  }
  function fallback(t) {
    var ta = d.createElement("textarea");
    ta.value = t;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    d.body.appendChild(ta);
    ta.select();
    try { d.execCommand("copy"); } catch (e) {}
    ta.remove();
  }

  d.querySelectorAll("pre>code").forEach(highlight);

  d.querySelectorAll(".tabset[data-tabs]").forEach(function (marker) {
    var names = marker.getAttribute("data-tabs").split("|"), key = "virling-docs-tab-" + (marker.getAttribute("data-key") || names.join("-"));
    var cb = d.createElement("div"), head = d.createElement("div"), tabs = d.createElement("div"), panels = [];
    cb.className = "cb";
    cb.setAttribute("data-tab-key", key);
    head.className = "cb-h";
    tabs.className = "tabs";
    tabs.setAttribute("role", "tablist");
    tabs.setAttribute("aria-label", marker.getAttribute("aria-label") || "Options");
    names.forEach(function (name, i) {
      var pre = marker.nextElementSibling;
      while (pre && pre.tagName !== "PRE") pre = pre.nextElementSibling;
      if (!pre) return;
      var panel = d.createElement("div"), id = "tp-" + Math.random().toString(36).slice(2, 8);
      panel.id = id;
      panel.setAttribute("role", "tabpanel");
      panel.setAttribute("aria-label", name);
      var foot = pre.nextElementSibling;
      panel.appendChild(pre);
      if (foot && foot.classList.contains("cb-foot")) panel.appendChild(foot);
      panels.push(panel);
      var t = d.createElement("button");
      t.type = "button";
      t.className = "tab";
      t.setAttribute("role", "tab");
      t.setAttribute("aria-controls", id);
      t.textContent = name;
      t.addEventListener("click", function () { pickAll(key, name); store(key, name); });
      t.addEventListener("keydown", function (e) {
        var all = tabs.querySelectorAll(".tab"), n = Array.prototype.indexOf.call(all, t);
        if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
          var next = all[(n + (e.key === "ArrowRight" ? 1 : all.length - 1)) % all.length];
          next.click();
          next.focus();
        }
      });
      tabs.appendChild(t);
    });
    head.appendChild(tabs);
    head.appendChild(copyButton(function () {
      var p = panels.filter(function (x) { return !x.hidden; })[0];
      return p.querySelector("pre");
    }));
    cb.appendChild(head);
    panels.forEach(function (p) { cb.appendChild(p); });
    marker.replaceWith(cb);
    pick(cb, store(key) || names[0]);
  });
  function pick(cb, name) {
    var tabs = cb.querySelectorAll(".tab");
    if (!Array.prototype.some.call(tabs, function (t) { return t.textContent === name; })) name = tabs[0].textContent;
    tabs.forEach(function (t) {
      var on = t.textContent === name;
      t.setAttribute("aria-selected", String(on));
      t.tabIndex = on ? 0 : -1;
      d.getElementById(t.getAttribute("aria-controls")).hidden = !on;
    });
  }
  function pickAll(key, name) {
    d.querySelectorAll('.cb[data-tab-key="' + key + '"]').forEach(function (cb) { pick(cb, name); });
  }

  d.querySelectorAll(".doc pre").forEach(function (pre) {
    if (pre.closest(".cb")) return;
    var code = pre.querySelector("code"), cb = d.createElement("div"), head = d.createElement("div"), f = d.createElement("span");
    cb.className = "cb";
    head.className = "cb-h";
    f.className = "cb-f";
    f.textContent = code ? label(code) : "";
    head.appendChild(f);
    head.appendChild(copyButton(function () { return pre; }));
    pre.parentNode.insertBefore(cb, pre);
    cb.appendChild(head);
    cb.appendChild(pre);
    var foot = cb.nextElementSibling;
    if (foot && foot.classList.contains("cb-foot")) cb.appendChild(foot);
  });
  d.querySelectorAll(".qs-c").forEach(function (box) {
    var pre = box.querySelector("pre");
    box.appendChild(copyButton(function () { return pre; }));
  });

  d.querySelectorAll(".doc blockquote.blockquote-tag").forEach(function (bq) {
    var warn = /blockquote-tag-(warning|caution)/.test(bq.className), native = bq.querySelector(".blockquote-tag-title"), title = native ? native.textContent.trim() : "";
    if (native) native.remove();
    var first = bq.firstElementChild;
    if (first && first.tagName === "P" && first.firstChild && first.firstChild.nodeName === "STRONG") {
      title = first.firstChild.textContent.replace(/[.:]$/, "");
      first.removeChild(first.firstChild);
      if (first.firstChild && first.firstChild.nodeType === 3) first.firstChild.textContent = first.firstChild.textContent.replace(/^\s+/, "");
      if (!first.textContent.trim() && !first.children.length) first.remove();
    }
    var co = d.createElement("div"), body = d.createElement("div"), t = d.createElement("span");
    co.className = "co" + (warn ? " warn" : "");
    co.setAttribute("role", "note");
    co.innerHTML = svg(warn ? I.warn : I.info, 18, 2);
    t.className = "t";
    t.textContent = title;
    body.appendChild(t);
    while (bq.firstChild) body.appendChild(bq.firstChild);
    co.appendChild(body);
    bq.replaceWith(co);
  });

  if (doc) doc.querySelectorAll("table").forEach(function (t) {
    if (t.parentNode.classList.contains("tbl")) return;
    var w = d.createElement("div");
    w.className = "tbl";
    t.parentNode.insertBefore(w, t);
    w.appendChild(t);
  });

  var marker = doc && doc.querySelector("[data-steps]");
  if (marker) {
    var ol = d.createElement("ol"), li = null, n = 0, node = marker.nextElementSibling;
    ol.className = "steps";
    while (node && !node.hasAttribute("data-steps-end")) {
      var next = node.nextElementSibling;
      if (node.tagName === "H2") {
        li = d.createElement("li");
        li.className = "step";
        li.id = "step-" + node.id;
        var sn = d.createElement("span");
        sn.className = "sn";
        sn.textContent = ++n;
        node.insertBefore(sn, node.firstChild);
        ol.appendChild(li);
      }
      if (li) li.appendChild(node);
      node = next;
    }
    marker.replaceWith(ol);
    if (node) node.remove();
  }

  var heads = doc ? Array.prototype.slice.call(doc.querySelectorAll("h2[id],h3[id]")).filter(function (h) { return !h.closest(".co"); }) : [];
  heads.forEach(function (h) {
    var a = d.createElement("a");
    a.className = "anchor";
    a.href = "#" + h.id;
    a.setAttribute("aria-label", "Link to this section");
    a.innerHTML = svg(I.hash, 15);
    h.appendChild(a);
  });
  var tocHeads = heads.filter(function (h) { return h.tagName === "H2"; });
  if (!tocHeads.length) tocHeads = heads;
  var toc = d.querySelector(".toc"), links = [];
  if (toc) toc.hidden = false;
  if (toc && !tocHeads.length) toc.querySelector("h6").hidden = true;
  if (toc && tocHeads.length) {
    var lists = [toc.querySelector(".toc-l")];
    var m = d.createElement("div");
    m.className = "mtoc";
    m.innerHTML = '<button type="button" aria-expanded="false">' + svg(I.menu, 16) + '<span>On this page</span><span class="i-down" style="display:inline-flex">' + svg(I.down, 16) + '</span></button><div class="toc-l" hidden></div>';
    lists.push(m.querySelector(".toc-l"));
    var mb = m.querySelector("button");
    mb.addEventListener("click", function () {
      var open = mb.getAttribute("aria-expanded") !== "true";
      mb.setAttribute("aria-expanded", String(open));
      lists[1].hidden = !open;
    });
    lists.forEach(function (list) {
      tocHeads.forEach(function (h) {
        var a = d.createElement("a");
        a.className = "toc-a" + (h.tagName === "H3" && tocHeads !== heads ? " sub" : "");
        a.href = "#" + h.id;
        var label = h.cloneNode(true);
        label.querySelectorAll(".sn,.anchor").forEach(function (x) { x.remove(); });
        a.textContent = label.textContent.trim();
        a.setAttribute("data-for", h.id);
        list.appendChild(a);
        links.push(a);
      });
    });
    var after = doc.querySelector(".meta") || doc.querySelector("h1+p") || doc.querySelector("h1");
    if (after && tocHeads.length > 1) after.parentNode.insertBefore(m, after.nextSibling);
    var ticking = false;
    var spy = function () {
      ticking = false;
      var cur = tocHeads[0];
      tocHeads.forEach(function (h) { if (h.getBoundingClientRect().top <= 110) cur = h; });
      if (window.innerHeight + window.scrollY >= d.documentElement.scrollHeight - 4) cur = tocHeads[tocHeads.length - 1];
      links.forEach(function (a) { a.classList.toggle("on", a.getAttribute("data-for") === cur.id); });
      doc.querySelectorAll(".step").forEach(function (s) { s.classList.toggle("on", s.id === "step-" + cur.id); });
    };
    window.addEventListener("scroll", function () { if (!ticking) { ticking = true; requestAnimationFrame(spy); } }, { passive: true });
    spy();
  }

  var active = d.querySelector(".side a.on"), crumbs = d.querySelector("[data-crumbs]");
  if (crumbs) {
    var parts = ['<a href="' + path_to_root + 'index.html">Docs</a>'];
    if (active) {
      var top = active.closest(".chapter>li"), p = top && top.previousElementSibling;
      while (p && !p.classList.contains("part-title")) p = p.previousElementSibling;
      if (p) parts.push("<span>" + esc(p.textContent.trim()) + "</span>");
      var parentSection = active.closest("ol.section");
      if (parentSection) {
        var parentLink = parentSection.parentNode.querySelector("a");
        if (parentLink) parts.push('<a href="' + parentLink.getAttribute("href") + '">' + esc(parentLink.textContent.trim()) + "</a>");
      }
      parts.push("<b>" + esc(active.textContent.trim()) + "</b>");
    }
    crumbs.innerHTML = parts.join(svg(I.chev, 12, 2));
  }

  var h1 = doc && doc.querySelector("h1"), pageTitle = h1 ? h1.textContent.trim() : d.title;
  var issue = REPO + "/issues/new?template=docs.yml&title=" + encodeURIComponent("Docs: " + pageTitle) + "&page=" + encodeURIComponent(location.origin + location.pathname);
  d.querySelectorAll("[data-issue]").forEach(function (a) { a.href = issue; });
  var help = d.querySelector("[data-help]");
  if (help) {
    var ask = help.querySelector(".help-ask"), thanks = help.querySelector(".thanks");
    help.querySelector("[data-help-yes]").addEventListener("click", function () { ask.hidden = true; thanks.hidden = false; });
    help.querySelector(".help-ask [data-issue]").addEventListener("click", function () {
      ask.hidden = true;
      thanks.hidden = false;
      thanks.lastChild.textContent = "Thanks. Tell us what was missing in the GitHub issue.";
    });
  }

  var box = d.querySelector(".search");
  if (!box || !window.virlingSearch) return;
  var input = box.querySelector(".sm-q"), meta = box.querySelector(".sm-meta"), body = box.querySelector(".sm-body");
  var index = null, loading = false, lastFocus = null, sel = 0;
  function load(src, cb) {
    var s = d.createElement("script");
    s.src = src;
    s.onload = cb;
    d.head.appendChild(s);
  }
  function ensure() {
    if (index || loading) return;
    loading = true;
    window.search = window.search || {};
    load(path_to_root + virlingSearch.lunr, function () {
      load(path_to_root + virlingSearch.index, function () {
        index = elasticlunr.Index.load(window.search.index);
        run();
      });
    });
  }
  function openSearch() {
    lastFocus = d.activeElement;
    drawer(false);
    box.hidden = false;
    d.body.style.overflow = "hidden";
    input.focus();
    input.select();
    ensure();
  }
  function closeSearch() {
    box.hidden = true;
    d.body.style.overflow = "";
    if (lastFocus && lastFocus.focus) lastFocus.focus();
  }
  d.querySelectorAll("[data-search-open]").forEach(function (b) { b.addEventListener("click", openSearch); });
  box.querySelectorAll("[data-search-close]").forEach(function (b) { b.addEventListener("click", closeSearch); });
  d.addEventListener("keydown", function (e) {
    var typing = /INPUT|TEXTAREA|SELECT/.test((e.target.tagName || "")) || e.target.isContentEditable;
    if ((e.key === "k" || e.key === "K") && (e.metaKey || e.ctrlKey)) { e.preventDefault(); box.hidden ? openSearch() : closeSearch(); return; }
    if (e.key === "/" && !typing && box.hidden) { e.preventDefault(); openSearch(); return; }
    if (e.key === "Escape") { if (!box.hidden) closeSearch(); else if (d.body.classList.contains("drawer-open")) drawer(false); }
  });
  input.addEventListener("input", run);
  input.addEventListener("keydown", function (e) {
    var items = body.querySelectorAll(".sr");
    if (!items.length) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      sel = (sel + (e.key === "ArrowDown" ? 1 : items.length - 1)) % items.length;
      mark(items);
    } else if (e.key === "Enter") {
      e.preventDefault();
      items[sel].click();
    }
  });
  function mark(items) {
    items.forEach(function (it, i) {
      it.classList.toggle("on", i === sel);
      it.setAttribute("aria-selected", String(i === sel));
    });
    if (items[sel]) items[sel].scrollIntoView({ block: "nearest" });
  }
  function partOf(path) {
    var a = d.querySelector('.side a[href="' + path_to_root + path + '"]');
    var top = a && a.closest(".chapter>li"), p = top && top.previousElementSibling;
    while (p && !p.classList.contains("part-title")) p = p.previousElementSibling;
    return p ? p.textContent.trim() : "";
  }
  function teaser(text, words) {
    var lower = text.toLowerCase(), at = -1;
    words.forEach(function (w) { var i = lower.indexOf(w); if (i >= 0 && (at < 0 || i < at)) at = i; });
    var start = Math.max(0, at - 24), snip = text.slice(start, start + 160).replace(/\s+/g, " ").trim();
    if (start > 0) snip = "…" + snip;
    var out = esc(snip);
    words.forEach(function (w) {
      if (w.length < 2) return;
      out = out.replace(new RegExp("(" + esc(w).replace(/[.*+?^${}()|[\]\\]/g, "\\$&") + ")(?![^<]*>)", "gi"), "<mark>$1</mark>");
    });
    return out;
  }
  function run() {
    var q = input.value.trim();
    sel = 0;
    if (!q) { body.innerHTML = ""; meta.textContent = ""; return; }
    if (!index) { meta.textContent = "Loading the index…"; return; }
    var opts = window.search.search_options, words = q.toLowerCase().split(/\s+/).filter(Boolean);
    var res = index.search(q, opts).slice(0, (window.search.results_options || {}).limit_results || 40);
    var groups = [], byPath = {};
    res.forEach(function (r) {
      var url = window.search.doc_urls[r.ref], path = url.split("#")[0], doc = index.documentStore.getDoc(r.ref);
      if (!byPath[path]) { byPath[path] = { path: path, page: doc.breadcrumbs.split(" » ")[0], items: [] }; groups.push(byPath[path]); }
      if (byPath[path].items.length < 4) byPath[path].items.push({ url: url, title: doc.title, body: doc.body });
    });
    var count = 0, html = "";
    groups.slice(0, 8).forEach(function (g) {
      var part = partOf(g.path);
      html += '<div class="sg">' + svg(I.doc, 14) + "<b>" + esc(g.page) + "</b>" + (part ? "<span>in " + esc(part) + "</span>" : "") + "</div>";
      g.items.forEach(function (it) {
        count++;
        var t = teaser(it.body, words);
        html += '<a class="sr" role="option" aria-selected="false" href="' + path_to_root + it.url + '">' + svg(I.hash, 15) +
          '<span class="sr-b"><span class="sr-t">' + esc(it.title) + '</span><span class="sr-x' + (/[{}]|\(\)|=(["']|&quot;)/.test(t) ? " mono" : "") + '">' + t + "</span></span>" +
          '<span class="sr-enter kbd">↵</span></a>';
      });
    });
    var shown = groups.slice(0, 8).length;
    meta.textContent = count ? count + (count === 1 ? " result" : " results") + " in " + shown + (shown === 1 ? " page" : " pages") : "";
    body.innerHTML = count ? html : '<div class="sm-empty">No results for “' + esc(q) + "”</div>";
    var items = body.querySelectorAll(".sr");
    items.forEach(function (it, i) {
      it.addEventListener("mousemove", function () { if (sel !== i) { sel = i; mark(items); } });
      it.addEventListener("click", function () { if (it.pathname === location.pathname) closeSearch(); });
    });
    mark(items);
  }
})();
