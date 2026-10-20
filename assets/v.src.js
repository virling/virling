/*! Virling, AGPL-3.0 */
((w, d, n, L) => {
  var s = d.currentScript, has = (a) => s.hasAttribute(a), on = (t, e, f, c) => t.addEventListener(e, f, c),
    site = s && s.getAttribute("data-site");
  if (!site || (/^(localhost|127\.|\[::1\])/.test(L.hostname) && !has("data-dev")) || (has("data-respect-gpc") && n.globalPrivacyControl)) return;
  var endpoint = s.getAttribute("data-api") || new URL(s.src).origin + "/api/event";
  var ref = d.referrer, last, vid = null, nav = performance.getEntriesByType("navigation")[0]?.type;

  if (!has("data-cookieless")) {
    vid = (d.cookie.match(/virling_vid=([^;]+)/) || [])[1] || "v" + Date.now().toString(36) + Math.random().toString(36).slice(2, 12);
    var dom, ck = (a) => (d.cookie = "virling_vid=" + vid + ";path=/;SameSite=Lax;max-age=" + a + (dom ? ";domain=" + dom : "") + (L.protocol == "https:" ? ";Secure" : ""), d.cookie.includes("virling_vid=" + vid));
    // Drop a host-only cookie, then walk up the hostname until the cookie sticks, so the ID lives on the registrable domain.
    ck(0);
    for (var p = L.hostname.split("."), i = p.length - 1; i-- > 0 && !ck(31536000, dom = p.slice(i).join(".")););
    i < 0 && ck(31536000, dom = 0);
  }

  var send = (name, goal) => {
    var body = JSON.stringify([{ n: name, u: L.href, r: ref, s: site, v: vid, w: screen.width, g: goal, l: n.language, k: nav, b: n.webdriver || !n.languages?.length || void 0 }]);
    ref = nav = null;
    n.sendBeacon?.(endpoint, body) || fetch(endpoint, { method: "POST", body, keepalive: true });
  };

  var pageview = () => {
    var path = L.pathname + L.search;
    if (path != last) { last = path; send("pageview"); }
  };

  // Routers navigate with pushState or replaceState; pageview skips a repeat of the same URL.
  ["pushState", "replaceState"].forEach((k, f) => {
    f = history[k];
    history[k] = function () { f.apply(this, arguments); pageview(); };
  });
  on(w, "popstate", pageview);

  // Record outbound clicks as exits, and carry the visitor ID into Stripe, Polar and Dodo payment links.
  var bare = (h) => h.replace(/^www\./, "");
  var pay = { "buy.stripe.com": "client_reference_id", "buy.polar.sh": "custom_field_data.virling_vid", "checkout.dodopayments.com": "metadata_virling_vid" };
  on(d, "click", (e) => {
    var a = e.target.closest?.("a[href]"), u, k;
    if (!a) return;
    try {
      u = new URL(a.href);
      if (/^http/.test(u.protocol) && bare(u.hostname) != bare(L.hostname)) send("exit", u.origin + u.pathname);
      k = vid && pay[u.hostname];
      if (k && !u.searchParams.has(k)) u.searchParams.set(k, vid), a.href = u.href;
    } catch (_) {}
  }, true);
  // Count the site's other open tabs, so closing one of several tabs or switching tabs is not a leave.
  var me = Math.random(), peers, bc, tabs = () => {
    peers = 0;
    if (w.BroadcastChannel) {
      bc = new BroadcastChannel("virling");
      // A new tab says its ID, open tabs answer with its negative, a closing tab says 0.
      bc.onmessage = (e, m) => { m = e.data; m > 0 ? (peers++, bc.postMessage(-m)) : m ? m == -me && peers++ : peers--; };
      bc.postMessage(me);
    }
  };
  tabs();
  // Leave only when the page itself goes away; the channel closes so the page can still enter the back/forward cache.
  on(w, "pagehide", () => { bc && (bc.postMessage(0), bc.close()); peers > 0 || send("leave"); });
  // A page restored by Back from the back/forward cache is a return, sent like a fresh load.
  on(w, "pageshow", (e) => { if (e.persisted) { tabs(); nav = "back_forward"; send("pageview"); } });

  var q = w.virling?.q || [];
  w.virling = (cmd, arg) => { cmd == "goal" && arg && send("goal", "" + arg); };
  w.virling.vid = () => vid;
  q.forEach((a) => w.virling.apply(0, a));
  pageview();
})(window, document, navigator, location);
