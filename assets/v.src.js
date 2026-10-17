/*! Virling, AGPL-3.0 */
((w, d, n, L) => {
  var s = d.currentScript, has = (a) => s.hasAttribute(a), on = (t, e, f, c) => t.addEventListener(e, f, c),
    site = s && s.getAttribute("data-site");
  if (!site || (/^(localhost|127\.|\[::1\])/.test(L.hostname) && !has("data-dev")) || (has("data-respect-gpc") && n.globalPrivacyControl)) return;
  var endpoint = s.getAttribute("data-api") || new URL(s.src).origin + "/api/event";
  var ref = d.referrer || null, last, vid = null;

  if (!has("data-cookieless")) {
    vid = (d.cookie.match(/(?:^|; )virling_vid=([^;]+)/) || [])[1];
    if (!vid) {
      vid = "v" + Date.now().toString(36) + Math.random().toString(36).slice(2, 12);
      d.cookie = "virling_vid=" + vid + ";path=/;max-age=31536000;SameSite=Lax" + (L.protocol == "https:" ? ";Secure" : "");
    }
  }

  var send = (name, goal) => {
    var body = JSON.stringify([{ n: name, u: L.href, r: ref, s: site, v: vid, w: screen.width, g: goal, l: n.language }]);
    ref = null;
    n.sendBeacon && n.sendBeacon(endpoint, body) || fetch(endpoint, { method: "POST", body, keepalive: true });
  };

  var pageview = () => {
    var path = L.pathname + L.search;
    if (path != last) { last = path; send("pageview"); }
  };

  var push = history.pushState;
  history.pushState = function () {
    push.apply(this, arguments);
    pageview();
  };
  on(w, "popstate", pageview);

  // Record outbound clicks as exits, and carry the visitor ID into Stripe, Polar and Dodo payment links.
  var pay = { "buy.stripe.com": "client_reference_id", "buy.polar.sh": "custom_field_data.virling_vid", "checkout.dodopayments.com": "metadata_virling_vid" };
  on(d, "click", (e) => {
    var a = e.target.closest && e.target.closest("a[href]"), u, k;
    if (!a) return;
    try {
      u = new URL(a.href);
      if (/^http/.test(u.protocol) && u.hostname != L.hostname) send("exit", u.origin + u.pathname);
      k = vid && pay[u.hostname];
      if (k && !u.searchParams.has(k)) u.searchParams.set(k, vid), a.href = u.href;
    } catch (_) {}
  }, true);
  // Mark when the page is left so time on page and the exit page are known.
  on(d, "visibilitychange", () => { d.visibilityState == "hidden" && send("leave"); });

  var q = (w.virling && w.virling.q) || [];
  w.virling = (cmd, arg) => { cmd == "goal" && arg && send("goal", "" + arg); };
  w.virling.vid = () => vid;
  q.forEach((a) => w.virling.apply(0, a));
  pageview();
})(window, document, navigator, location);
