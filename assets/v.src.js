/*! Virling tracker, AGPL-3.0, github.com/virling/virling */
(function (w, d, n) {
  var s = d.currentScript, site = s && s.getAttribute("data-site");
  if (!site || (/^(localhost|127\.|\[::1\])/.test(location.hostname) && !s.hasAttribute("data-dev")) || (s.hasAttribute("data-respect-gpc") && n.globalPrivacyControl)) return;
  var endpoint = s.getAttribute("data-api") || new URL(s.src).origin + "/api/event";
  var ref = d.referrer || null, last, vid = null;

  if (!s.hasAttribute("data-cookieless")) {
    vid = (d.cookie.match(/(?:^|; )virling_vid=([^;]+)/) || [])[1];
    if (!vid) {
      vid = "v" + Date.now().toString(36) + Math.random().toString(36).slice(2, 12);
      d.cookie = "virling_vid=" + vid + ";path=/;max-age=31536000;SameSite=Lax" + (location.protocol == "https:" ? ";Secure" : "");
    }
  }

  function send(name, goal) {
    var body = JSON.stringify([{ n: name, u: location.href, r: ref, s: site, v: vid, w: screen.width, g: goal }]);
    ref = null;
    n.sendBeacon && n.sendBeacon(endpoint, body) || fetch(endpoint, { method: "POST", body: body, keepalive: true });
  }

  function pageview() {
    var path = location.pathname + location.search;
    if (path != last) { last = path; send("pageview"); }
  }

  var push = history.pushState;
  history.pushState = function () {
    push.apply(this, arguments);
    pageview();
  };
  w.addEventListener("popstate", pageview);

  // Carry the visitor ID into Stripe, Polar and Dodo payment links so the payment can be matched back.
  d.addEventListener("click", function (e) {
    var a = e.target.closest && e.target.closest("a[href]"), u, k;
    if (!a || !vid) return;
    try {
      u = new URL(a.href);
      k = u.hostname == "buy.stripe.com" ? "client_reference_id" : u.hostname == "buy.polar.sh" ? "custom_field_data.virling_vid" : u.hostname == "checkout.dodopayments.com" ? "metadata_virling_vid" : 0;
      if (k && !u.searchParams.has(k)) u.searchParams.set(k, vid), a.href = u.href;
    } catch (_) {}
  }, true);

  var q = (w.virling && w.virling.q) || [];
  w.virling = function (cmd, arg) {
    cmd == "goal" && arg && send("goal", String(arg));
  };
  w.virling.vid = function () {
    return vid;
  };
  q.forEach(function (a) { w.virling.apply(0, a); });
  pageview();
})(window, document, navigator);
