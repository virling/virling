/* Virling tracker, AGPL-3.0. https://github.com/virling/virling */
(function (w, d) {
  var s = d.currentScript;
  if (!s) return;
  var site = s.getAttribute("data-site");
  var endpoint = s.getAttribute("data-api") || new URL(s.src).origin + "/api/event";
  var cookieless = s.hasAttribute("data-cookieless");
  var local = /^(localhost|127\.|0\.0\.0\.0|\[::1\])/.test(location.hostname) && !s.hasAttribute("data-dev");
  if (!site || local || (s.hasAttribute("data-respect-gpc") && navigator.globalPrivacyControl)) return;

  var queue = [], timer, lastPath, ref = d.referrer || null;

  function visitorId() {
    if (cookieless) return null;
    var m = d.cookie.match(/(?:^|; )virling_vid=([^;]+)/);
    if (m) return m[1];
    var v = w.crypto && crypto.randomUUID ? crypto.randomUUID() : "v" + Date.now().toString(36) + Math.random().toString(36).slice(2, 12);
    d.cookie = "virling_vid=" + v + ";path=/;max-age=31536000;SameSite=Lax" + (location.protocol == "https:" ? ";Secure" : "");
    return v;
  }
  var vid = visitorId();

  function flush() {
    if (!queue.length) return;
    var body = JSON.stringify(queue);
    queue = [];
    clearTimeout(timer);
    if (!(navigator.sendBeacon && navigator.sendBeacon(endpoint, new Blob([body], { type: "text/plain" })))) {
      fetch(endpoint, { method: "POST", body: body, keepalive: true, headers: { "content-type": "text/plain" } }).catch(function () {});
    }
  }

  function send(name, goal) {
    queue.push({ n: name, u: location.href, r: ref, s: site, v: vid, w: screen.width, g: goal });
    ref = null;
    clearTimeout(timer);
    timer = setTimeout(flush, 1000);
  }

  function pageview() {
    var path = location.pathname + location.search;
    if (path === lastPath) return;
    lastPath = path;
    send("pageview");
  }

  ["pushState", "replaceState"].forEach(function (f) {
    var orig = history[f];
    history[f] = function () {
      var result = orig.apply(this, arguments);
      pageview();
      return result;
    };
  });
  w.addEventListener("popstate", pageview);
  w.addEventListener("pagehide", flush);
  d.addEventListener("visibilitychange", function () {
    if (d.visibilityState == "hidden") flush();
  });

  // Carry the visitor ID into Stripe Payment Links and Polar checkout links so the payment can be matched back.
  d.addEventListener("click", function (e) {
    var a = e.target.closest && e.target.closest("a[href]");
    if (!a || !vid) return;
    try {
      var u = new URL(a.href), h = u.hostname, k;
      if (h == "buy.stripe.com") k = "client_reference_id";
      else if (h == "buy.polar.sh" || (h == "sandbox-api.polar.sh" && !u.pathname.indexOf("/v1/checkout-links/"))) k = "custom_field_data.virling_vid";
      if (k && !u.searchParams.has(k)) {
        u.searchParams.set(k, vid);
        a.href = u.href;
      }
    } catch (err) {}
  }, true);

  var pending = (w.virling && w.virling.q) || [];
  var api = function (cmd, arg) {
    if (cmd == "goal" && arg) send("goal", String(arg));
  };
  api.vid = function () {
    return vid;
  };
  w.virling = api;
  pending.forEach(function (args) {
    api.apply(null, args);
  });
  pageview();
})(window, document);
