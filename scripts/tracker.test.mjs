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

// BroadcastChannel over a shared set of open channels, delivered asynchronously like a browser does.
const channel = (bus) =>
  class {
    constructor() { bus.add(this); }
    postMessage(data) { for (const o of bus) if (o !== this) setTimeout(() => bus.has(o) && o.onmessage?.({ data })); }
    close() { bus.delete(this); }
  };
const tick = () => new Promise((r) => setTimeout(r, 5));

function browser(url, { cookie = jar(new URL(url).hostname), attrs = {}, nav = "navigate", bus = new Set(), agent = {}, zone = "Europe/Berlin" } = {}) {
  const loc = new URL(url);
  const sent = [];
  const urls = [];
  const timers = [];
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
    hidden: false,
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
  const window = { addEventListener: on("w:"), BroadcastChannel: channel(bus) };
  const navigator = {
    language: "en-US",
    languages: ["en-US"],
    sendBeacon: (u, body) => (urls.push(String(u)), sent.push(...JSON.parse(body)), true),
    ...agent,
  };
  const performance = { getEntriesByType: (t) => (t == "navigation" ? [{ type: nav }] : []) };
  const ctx = { window, document, navigator, location, history, performance, BroadcastChannel: window.BroadcastChannel, screen: { width: 1440 }, devicePixelRatio: 2, Intl: { DateTimeFormat: () => ({ resolvedOptions: () => ({ timeZone: zone }) }) }, URL, JSON, Date, Math, fetch: () => {}, setInterval: (f, ms) => timers.push([f, ms]) };
  vm.runInNewContext(SRC, ctx);
  const fire = (t, e, ev = {}) => (listeners[t + e] || []).forEach((f) => f(ev));
  return { sent, urls, timers, cookie, window, document, history, location, fire };
}

test("pageview with page, screen and language", () => {
  const b = browser("https://example.com/pricing?x=1");
  assert.equal(b.sent.length, 1);
  assert.deepEqual(
    { n: b.sent[0].n, u: b.sent[0].u, s: b.sent[0].s, w: b.sent[0].w, d: b.sent[0].d, l: b.sent[0].l },
    { n: "pageview", u: "https://example.com/pricing?x=1", s: "example.com", w: 1440, d: 2, l: "en-US" },
  );
  assert.match(b.sent[0].v, /^v[a-z0-9]+$/);
});

test("time zone is sent only when the browser has one", () => {
  assert.equal(browser("https://example.com/").sent[0].z, "Europe/Berlin");
  assert.equal("z" in browser("https://example.com/", { zone: "" }).sent[0], false);
});

test("?virling=ignore silences this browser until ?virling=track", () => {
  const cookie = jar("example.com");
  assert.equal(browser("https://example.com/?virling=ignore", { cookie }).sent.length, 0);
  const later = browser("https://example.com/pricing", { cookie });
  later.fire("w:", "pagehide");
  assert.deepEqual([later.sent.length, later.timers.length], [0, 0]);
  assert.equal(browser("https://example.com/?virling=track", { cookie }).sent.length, 1);
  assert.equal(browser("https://example.com/", { cookie }).sent.length, 1);
  assert.equal([...cookie.cookies.values()].some((c) => c.name == "virling"), false);
});

test("automation and a browser without languages are flagged", () => {
  assert.equal(browser("https://example.com/").sent[0].b, undefined);
  assert.equal(browser("https://example.com/", { agent: { webdriver: true } }).sent[0].b, true);
  assert.equal(browser("https://example.com/", { agent: { languages: [] } }).sent[0].b, true);
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
  assert.deepEqual([...cookie.cookies.values()].map((c) => [c.domain, c.value]), [["example.com", "vold123456"]]);
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

test("the first pageview carries the navigation type, route changes do not", () => {
  const b = browser("https://example.com/", { nav: "reload" });
  b.history.pushState({}, "", "/a");
  assert.deepEqual(b.sent.map((e) => e.k), ["reload", null]);
});

test("a visible tab pings every 30 s, a hidden one does not", () => {
  const b = browser("https://example.com/");
  assert.deepEqual(b.timers.map(([, ms]) => ms), [30000]);
  b.timers[0][0]();
  b.document.hidden = true;
  b.timers[0][0]();
  assert.deepEqual(b.sent.map((e) => e.n), ["pageview", "ping"]);
  assert.deepEqual([...new Set(b.urls)], ["https://virling.test/api/event"]);
});

test("leave fires when the page goes away, not when the tab is hidden", () => {
  const b = browser("https://example.com/");
  b.document.visibilityState = "hidden";
  b.fire("d:", "visibilitychange");
  assert.deepEqual(b.sent.map((e) => e.n), ["pageview"]);
  b.fire("w:", "pagehide", { persisted: false });
  assert.deepEqual(b.sent.map((e) => e.n), ["pageview", "leave"]);
});

test("closing one of two open tabs is not a leave, closing the last one is", async () => {
  const bus = new Set();
  const a = browser("https://example.com/", { bus });
  await tick();
  const b = browser("https://example.com/pricing", { bus });
  await tick();
  a.fire("w:", "pagehide", { persisted: false });
  await tick();
  b.fire("w:", "pagehide", { persisted: false });
  assert.deepEqual(a.sent.map((e) => e.n), ["pageview"]);
  assert.deepEqual(b.sent.map((e) => e.n), ["pageview", "leave"]);
});

test("a back/forward cache restore is sent as a return pageview", () => {
  const b = browser("https://example.com/pricing");
  b.fire("d:", "click", { target: { closest: () => ({ href: "https://x.com/" }) } });
  b.fire("w:", "pagehide", { persisted: true });
  b.fire("w:", "pageshow", { persisted: false });
  b.fire("w:", "pageshow", { persisted: true });
  assert.deepEqual(b.sent.map((e) => [e.n, e.k ?? null]), [
    ["pageview", "navigate"],
    ["exit", null],
    ["leave", null],
    ["pageview", "back_forward"],
  ]);
  assert.equal(b.sent[3].u, "https://example.com/pricing");
  assert.equal(b.sent[3].r, null);
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
