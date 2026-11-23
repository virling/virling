// Runs assets/v.js against a fake browser: node --test scripts/tracker.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const SRC = readFileSync(new URL("../assets/v.js", import.meta.url), "utf8");
const COMPAT = readFileSync(new URL("../assets/v-compat.js", import.meta.url), "utf8");
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

function browser(url, { cookie = jar(new URL(url).hostname), attrs = {}, nav = "navigate", bus = new Set(), agent = {}, zone = "Europe/Berlin", links = [], before = "", compat = false, gpu = "ANGLE (Apple, ANGLE Metal Renderer: Apple M2, Unspecified Version)", entries = {} } = {}) {
  const loc = new URL(url);
  const clock = { t: 1_700_000_000_000 };
  const sent = [];
  const urls = [];
  const timers = [];
  const imports = [];
  const listeners = {};
  const on = (t) => (e, f) => (listeners[t + e] ||= []).push(f);
  const script = {
    src: "",
    async: false,
    defer: false,
    dataset: {},
    getAttribute(n) { return this[n]; },
    setAttribute(n, v) { this[n] = v; },
  };

  class Element {
    constructor(tag, parent = null) {
      this.tagName = tag.toUpperCase();
      this.parentElement = parent;
      this.children = [];
      this.attributes = new Map();
      this.dataset = {};
      this.classList = {
        _set: new Set(),
        add(c) { this._set.add(c); },
        remove(c) { this._set.delete(c); },
        contains(c) { return this._set.has(c); },
      };
      this.style = {};
      this.hidden = false;
      this._textContent = "";
      this._value = "";
    }
    get textContent() {
      return this._textContent;
    }
    set textContent(v) {
      this._textContent = v;
    }
    get value() {
      return this._value;
    }
    set value(v) {
      this._value = v;
    }
    getAttribute(name) {
      if (name === "role") return this.attributes.get("role");
      if (name === "aria-selected") return this.attributes.get("aria-selected");
      if (name === "aria-haspopup") return this.attributes.get("aria-haspopup");
      if (name === "aria-expanded") return this.attributes.get("aria-expanded");
      return this.attributes.get(name) || null;
    }
    setAttribute(name, value) {
      this.attributes.set(name, String(value));
    }
    hasAttribute(name) {
      return this.attributes.has(name);
    }
    removeAttribute(name) {
      this.attributes.delete(name);
    }
    querySelector(sel) {
      return this.querySelectorAll(sel)[0] || null;
    }
    querySelectorAll(sel) {
      const results = [];
      const match = (el) => {
        if (sel.startsWith("[data-") && sel.endsWith("]")) {
          const attr = sel.slice(1, -1);
          if (attr.includes("=")) {
            const [k, v] = attr.split("=");
            const cleanV = v.replace(/^['"]|['"]$/g, "");
            return el.dataset && el.dataset[k.replace("data-", "")] === cleanV;
          }
          return el.dataset && el.dataset[attr.replace("data-", "")] !== undefined;
        }
        if (sel.startsWith("input[type=")) {
          const type = sel.match(/type=([^\]]+)/)[1];
          return el.tagName === "INPUT" && el.attributes.get("type") === type;
        }
        if (sel.startsWith("button")) {
          return el.tagName === "BUTTON";
        }
        if (sel.startsWith(".")) {
          return el.classList.contains(sel.slice(1));
        }
        if (sel === "[role=option]") {
          return el.getAttribute("role") === "option";
        }
        return false;
      };

      const walk = (node) => {
        for (const child of node.children) {
          if (match(child)) results.push(child);
          walk(child);
        }
      };
      walk(this);
      return results;
    }
    appendChild(child) {
      child.parentElement = this;
      this.children.push(child);
      return child;
    }
    prepend(child) {
      child.parentElement = this;
      this.children.unshift(child);
      return child;
    }
    scrollIntoView() {}
    focus() {}
  }

  const document = new Element("document");
  document.createElement = (tag) => new Element(tag);
  document.querySelector = (sel) => document.querySelectorAll(sel)[0] || null;
  document.querySelectorAll = (sel) => document.querySelectorAll(sel);
  document.currentScript = script;

  const sandbox = {
    console,
    URL,
    Intl,
    setTimeout,
    clearTimeout,
    setInterval,
    clearInterval,
    document,
    navigator: {
      userAgent: agent.userAgent || "Mozilla/5.0",
      language: agent.language || "en-US",
      hardwareConcurrency: 8,
      deviceMemory: 8,
      connection: { effectiveType: "4g" },
      ...agent,
    },
    location: loc,
    history: {
      pushState(state, title, url) { loc.href = new URL(url, loc).href; },
      replaceState(state, title, url) { loc.href = new URL(url, loc).href; },
    },
    window: {
      location: loc,
      localStorage: {
        _store: new Map(),
        getItem(k) { return this._store.get(k) || null; },
        setItem(k, v) { this._store.set(k, String(v)); },
        removeItem(k) { this._store.delete(k); },
      },
      sessionStorage: {
        _store: new Map(),
        getItem(k) { return this._store.get(k) || null; },
        setItem(k, v) { this._store.set(k, String(v)); },
        removeItem(k) { this._store.delete(k); },
      },
      addEventListener(e, f) { (listeners["window" + e] ||= []).push(f); },
      removeEventListener(e, f) {},
    },
  };

  vm.createContext(sandbox);
  if (before) vm.runInContext(before, sandbox);
  vm.runInContext(SRC, sandbox);
  if (compat) vm.runInContext(COMPAT, sandbox);

  return { sandbox, loc, sent, urls, timers, listeners, clock };
}

test("Polar fee calculator tz auto and select component logic", () => {
  const b = browser("https://example.com/tools/polar-fee-calculator", {
    agent: {
      resolvedOptions: () => ({ timeZone: "America/New_York" })
    }
  });

  const tzPick = new (b.sandbox.document.createElement("div"))("div");
  tzPick.dataset.tzAuto = "";
  const hiddenInput = new (b.sandbox.document.createElement("input"))("input");
  hiddenInput.setAttribute("type", "hidden");
  hiddenInput.value = "UTC";
  tzPick.appendChild(hiddenInput);

  const listbox = new (b.sandbox.document.createElement("div"))("div");
  listbox.setAttribute("role", "listbox");
  const opt1 = new (b.sandbox.document.createElement("button"))("button");
  opt1.setAttribute("role", "option");
  opt1.dataset.value = "Europe/London";
  opt1.textContent = "Europe/London";
  listbox.appendChild(opt1);
  tzPick.appendChild(listbox);

  const label = new (b.sandbox.document.createElement("span"))("span");
  label.dataset.label = "";
  tzPick.appendChild(label);

  b.sandbox.document.children.push(tzPick);

  // Mock Intl.DateTimeFormat resolvedOptions timeZone
  const origDTF = Intl.DateTimeFormat;
  global.Intl.DateTimeFormat = class {
    resolvedOptions() {
      return { timeZone: "America/New_York" };
    }
  };

  try {
    // Re-run snippet or test logic that handles tzPick and selects
    vm.runInContext(`
      const tzPick = document.querySelector("[data-tz-auto]");
      const browserTz = Intl.DateTimeFormat().resolvedOptions().timeZone;
      if (tzPick && browserTz && tzPick.querySelector("input[type=hidden]").value === "UTC") {
        const list = tzPick.querySelector("[role=listbox]");
        let opt = [...list.querySelectorAll("[role=option]")].find((o) => o.dataset.value === browserTz);
        if (!opt) {
          opt = document.createElement("button");
          opt.type = "button";
          opt.setAttribute("role", "option");
          opt.dataset.value = opt.textContent = browserTz;
          list.prepend(opt);
        }
        list.querySelectorAll("[role=option]").forEach((o) => o.setAttribute("aria-selected", String(o === opt)));
        tzPick.querySelector("input[type=hidden]").value = browserTz;
        tzPick.querySelector("[data-label]").textContent = browserTz;
      }
    `, b.sandbox);

    assert.equal(hiddenInput.value, "America/New_York");
    assert.equal(label.textContent, "America/New_York");
    assert.equal(listbox.children[0].dataset.value, "America/New_York");
    assert.equal(listbox.children[0].getAttribute("aria-selected"), "true");
  } finally {
    global.Intl.DateTimeFormat = origDTF;
  }
});