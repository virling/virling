// Runs assets/v.js against a fake browser: node --test scripts/tracker.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const SRC = readFileSync(new URL("../assets/v.js", import.meta.url), "utf8");
const SUFFIXES = new Set(["com", "uk", "co.uk", "io"]);

// A cookie jar that, like a browser, rejects public suffixes and domains the host is not under.
function jar(host) {
  const cookies = new Map();
  return {
    cookies,
    get() {
      return [...cookies.values()].filter((c) => c.value !== null).map((c) => `${c.name}=${c.value}`).join("; ");
    },
    set(raw) {
      const [pair, ...attrs] = raw.split(";");
      const [name, value] = pair.split("=");
      const a = Object.fromEntries(attrs.map((x) => x.split("=")).map(([k, v]) => [k.toLowerCase(), v ?? true]));
      const domain = a.domain || "";
      if (domain && (SUFFIXES.has(domain) || !(host == domain || host.endsWith("." + domain)))) return;
      const key = `${name}@${domain || host}`;
      if (a["max-age"] === "0") cookies.delete(key);
      else cookies.set(key, { name, value, domain: domain || null });
    },
  };
}

function browser(url, { cookie = jar(new URL(url).hostname), attrs = {} } = {}) {
  const loc = new URL(url);
  const sent = [];
  const listeners = {};
  const on = (t) => (e, f) => (listeners[t + e] ||= []).push(f);
  const script = {
    src: "https://virling.test/v.js",
    hasAttribute: (k) => k in attrs,
    getAttribute: (k) => (k == "data-site" ? attrs[k] ?? "example.com" : attrs[k] ?? null),
  };
  const document = {
    currentScript: script,
    referrer: "",
    visibilityState: "visible",
    addEventListener: on("d:"),
    get cookie() { return cookie.get(); },
    set cookie(v) { cookie.set(v); },
  };
  const history = {
    pushState(_s, _t, u) { setUrl(u); },
    replaceState(_s, _t, u) { setUrl(u); },
  };
  const setUrl = (u) => {
    const n = new URL(u, loc.href);
    for (const k of ["href", "pathname", "search", "hash"]) location[k] = n[k];
  };
  const location = { href: loc.href, hostname: loc.hostname, protocol: loc.protocol, pathname: loc.pathname, search: loc.search, hash: loc.hash, origin: loc.origin };
  const window = { addEventListener: on("w:") };
  const navigator = {
    language: "en-US",
    sendBeacon: (_u, body) => (sent.push(...JSON.parse(body)), true),
  };
  const ctx = { window, document, navigator, location, history, screen: { width: 1440 }, URL, JSON, Date, Math, fetch: () => {} };
  vm.runInNewContext(SRC, ctx);
  const fire = (t, e, ev = {}) => (listeners[t + e] || []).forEach((f) => f(ev));
  return { sent, cookie, window, document, history, location, fire };
}

test("pageview with page, screen and language", () => {
  const b = browser("https://example.com/pricing?x=1");
  assert.equal(b.sent.length, 1);
  assert.deepEqual(
    { n: b.sent[0].n, u: b.sent[0].u, s: b.sent[0].s, w: b.sent[0].w, l: b.sent[0].l },
    { n: "pageview", u: "https://example.com/pricing?x=1", s: "example.com", w: 1440, l: "en-US" },
  );
  assert.match(b.sent[0].v, /^v[a-z0-9]+$/);
});

test("goals from the queue and from virling()", () => {
  const b = browser("https://example.com/");
  b.window.virling("goal", "signup");
  assert.deepEqual(b.sent.slice(1).map((e) => [e.n, e.g]), [["goal", "signup"]]);
});

for (const [host, domain] of [
  ["www.example.com", "example.com"],
  ["example.com", "example.com"],
  ["app.example.com", "example.com"],
  ["shop.example.co.uk", "example.co.uk"],
  ["localhost", null],
]) {
  test(`visitor cookie on ${host} lives on ${domain ?? "the host"}`, () => {
    const b = browser(`https://${host}/`, { attrs: { "data-dev": "" } });
    const vids = [...b.cookie.cookies.values()].filter((c) => c.name == "virling_vid");
    assert.deepEqual(vids.map((c) => c.domain), [domain]);
    assert.equal(vids[0].value, b.sent[0].v);
    assert.ok(![...b.cookie.cookies.values()].some((c) => c.name == "virling_t"), "the probe is removed");
  });
}

test("the visitor ID is shared between example.com and app.example.com", () => {
  const site = browser("https://example.com/");
  const shared = jar("app.example.com");
  for (const c of site.cookie.cookies.values()) shared.set(`${c.name}=${c.value};domain=${c.domain}`);
  const app = browser("https://app.example.com/checkout", { cookie: shared });
  assert.equal(app.sent[0].v, site.sent[0].v);
});

test("an ID from a host-only cookie moves to the registrable domain", () => {
  const cookie = jar("www.example.com");
  cookie.set("virling_vid=vold123456;path=/");
  const b = browser("https://www.example.com/", { cookie });
  assert.equal(b.sent[0].v, "vold123456");
  assert.ok([...cookie.cookies.values()].some((c) => c.domain == "example.com" && c.value == "vold123456"));
});

test("cookieless mode sets no cookie", () => {
  const b = browser("https://example.com/", { attrs: { "data-cookieless": "" } });
  assert.equal(b.cookie.cookies.size, 0);
  assert.equal(b.sent[0].v, null);
});

test("outbound clicks are exits and payment links carry the visitor ID", () => {
  const b = browser("https://example.com/");
  const vid = b.sent[0].v;
  const link = (href) => {
    const a = { href };
    b.fire("d:", "click", { target: { closest: () => a } });
    return a.href;
  };
  assert.equal(link("https://buy.stripe.com/abc"), `https://buy.stripe.com/abc?client_reference_id=${vid}`);
  assert.equal(link("https://buy.polar.sh/p"), `https://buy.polar.sh/p?custom_field_data.virling_vid=${vid}`);
  assert.equal(link("https://checkout.dodopayments.com/buy/p"), `https://checkout.dodopayments.com/buy/p?metadata_virling_vid=${vid}`);
  link("https://x.com/virling");
  link("https://example.com/about");
  link("https://www.example.com/about");
  assert.deepEqual(
    b.sent.filter((e) => e.n == "exit").map((e) => e.g),
    ["https://buy.stripe.com/abc", "https://buy.polar.sh/p", "https://checkout.dodopayments.com/buy/p", "https://x.com/virling"],
  );
});

test("www and the bare domain are one site", () => {
  const b = browser("https://www.example.com/");
  b.fire("d:", "click", { target: { closest: () => ({ href: "https://example.com/pricing" }) } });
  b.fire("d:", "click", { target: { closest: () => ({ href: "https://blog.example.net/" }) } });
  assert.deepEqual(b.sent.filter((e) => e.n == "exit").map((e) => e.g), ["https://blog.example.net/"]);
});

test("pushState and replaceState route changes log pageviews, once per URL", () => {
  const b = browser("https://example.com/");
  b.history.pushState({}, "", "/a");
  b.history.replaceState({}, "", "/b?tab=1");
  b.history.replaceState({}, "", "/b?tab=1");
  b.history.replaceState({ scroll: 1 }, "", "/b?tab=1#top");
  b.history.pushState({}, "", "/a");
  Object.assign(b.location, { href: "https://example.com/c", pathname: "/c", search: "" });
  b.fire("w:", "popstate");
  b.fire("w:", "popstate");
  assert.deepEqual(
    b.sent.map((e) => new URL(e.u).pathname),
    ["/", "/a", "/b", "/a", "/c"],
  );
});
