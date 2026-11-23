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
    getAttribute(n) { return this[n]; },
    setAttribute(n, v) { this[n] = v; },
  };

  class DOMTokenList extends Set {
    contains(token) { return super.has(token); }
    add(token) { super.add(token); }
    remove(token) { super.delete(token); }
  }

  class Element {
    constructor(tag, opts = {}) {
      this.tagName = tag.toUpperCase();
      this.attributes = new Map();
      this.classList = new DOMTokenList();
      this.children = [];
      this.hidden = false;
      this._textContent = "";
      this._value = "";
      this.dataset = {};
      for (const [k, v] of Object.entries(opts)) {
        if (k === "dataset") Object.assign(this.dataset, v);
        else if (k === "hidden") this.hidden = v;
        else if (k === "value") this._value = v;
        else this.attributes.set(k, String(v));
      }
    }
    getAttribute(n) { return this.attributes.get(n) ?? null; }
    setAttribute(n, v) { this.attributes.set(n, String(v)); }
    removeAttribute(n) { this.attributes.delete(n); }
    querySelector(sel) {
      if (sel === "[data-tz-auto]") {
        return this.children.find(c => c.dataset?.tzAuto !== undefined || c.getAttribute("data-tz-auto") !== null) || null;
      }
      if (sel === "input[type=hidden]") {
        return this.children.find(c => c.tagName === "INPUT" && c.getAttribute("type") === "hidden") || null;
      }
      if (sel === "[role=listbox]") {
        return this.children.find(c => c.getAttribute("role") === "listbox") || null;
      }
      if (sel === "[role=option]") {
        return this.children.find(c => c.getAttribute("role") === "option") || null;
      }
      if (sel === "[data-label]") {
        return this.children.find(c => c.dataset?.label !== undefined || c.getAttribute("data-label") !== null) || null;
      }
      if (sel === "button[aria-haspopup]") {
        return this.children.find(c => c.tagName === "BUTTON" && c.getAttribute("aria-haspopup") !== null) || null;
      }
      if (sel === ".menu") {
        return this.children.find(c => c.classList.contains("menu")) || null;
      }
      if (sel === "input[type=search]") {
        return this.children.find(c => c.tagName === "INPUT" && c.getAttribute("type") === "search") || null;
      }
      return this.children[0] || null;
    }
    querySelectorAll(sel) {
      if (sel === "[role=option]") {
        const results = [];
        const walk = (el) => {
          if (el.getAttribute("role") === "option") results.push(el);
          for (const ch of el.children) walk(ch);
        };
        walk(this);
        return results;
      }
      if (sel === "[data-select]") {
        const results = [];
        const walk = (el) => {
          if (el.dataset?.select !== undefined || el.getAttribute("data-select") !== null) results.push(el);
          for (const ch of el.children) walk(ch);
        };
        walk(this);
        return results;
      }
      return [];
    }
    appendChild(child) {
      this.children.push(child);
      return child;
    }
    prepend(child) {
      this.children.unshift(child);
      return child;
    }
    get textContent() { return this._textContent; }
    set textContent(v) { this._textContent = v; }
    get value() { return this._value; }
    set value(v) { this._value = v; }
    scrollIntoView() {}
    focus() {}
  }

  const document = {
    createElement(tag) { return new Element(tag); },
    querySelector(sel) {
      if (sel === "[data-tz-auto]") {
        return root.children.find(c => c.dataset?.tzAuto !== undefined || c.getAttribute("data-tz-auto") !== null) || null;
      }
      return null;
    },
    querySelectorAll(sel) {
      if (sel === "[data-select]") {
        const results = [];
        const walk = (el) => {
          if (el.dataset?.select !== undefined || el.getAttribute("data-select") !== null) results.push(el);
          for (const ch of el.children) walk(ch);
        };
        for (const ch of root.children) walk(ch);
        return results;
      }
      return [];
    }
  };

  const root = new Element("div");

  const sandbox = {
    console,
    setTimeout,
    clearTimeout,
    setInterval,
    clearInterval,
    URL,
    Intl: {
      DateTimeFormat() {
        return {
          resolvedOptions() {
            return { timeZone: zone };
          }
        };
      }
    },
    document,
    navigator: {
      userAgent: agent.userAgent || "Mozilla/5.0",
      languages: agent.languages || ["en-US"],
      hardwareConcurrency: 8,
      deviceMemory: 8,
      maxTouchPoints: 0,
    },
    location: loc,
    history: {
      pushState(state, title, url) {
        if (url) loc.href = new URL(url, loc).href;
      },
      replaceState(state, title, url) {
        if (url) loc.href = new URL(url, loc).href;
      }
    },
    window: {},
    fetch(u, opts) {
      urls.push({ u, opts });
      return Promise.resolve({ ok: true, json: () => Promise.resolve({}) });
    }
  };
  sandbox.window = sandbox;

  vm.createContext(sandbox);
  return { sandbox, root, document };
}

test("issue #1398: Free tool: Polar fee calculator timezone auto-detection", async () => {
  const { sandbox, root } = browser("https://example.com/", { zone: "America/New_York" });
  
  const tzPick = new Element("div", { dataset: { tzAuto: "" } });
  const hiddenInput = new Element("input", { type: "hidden", value: "UTC" });
  const listbox = new Element("div", { role: "listbox" });
  const label = new Element("span", { dataset: { label: "" } });
  
  const opt1 = new Element("button", { role: "option", dataset: { value: "Europe/London" } });
  opt1.textContent = "Europe/London";
  opt1.setAttribute("aria-selected", "false");
  listbox.appendChild(opt1);

  tzPick.appendChild(hiddenInput);
  tzPick.appendChild(listbox);
  tzPick.appendChild(label);
  root.appendChild(tzPick);

  vm.runInContext(SRC, sandbox);

  assert.equal(hiddenInput.value, "America/New_York");
  assert.equal(label.textContent, "America/New_York");
  const options = listbox.querySelectorAll("[role=option]");
  assert.equal(options.length, 2);
  assert.equal(options[0].dataset.value, "America/New_York");
  assert.equal(options[0].getAttribute("aria-selected"), "true");
  assert.equal(options[1].getAttribute("aria-selected"), "false");
});