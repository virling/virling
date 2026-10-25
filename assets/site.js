for (const button of document.querySelectorAll("[data-copy-snippet]")) {
  const snippet = button.parentElement.querySelector("[data-snippet]");
  button.addEventListener("click", async () => {
    await navigator.clipboard.writeText(snippet.textContent);
    window.virling && virling("goal", "copy_snippet");
    button.textContent = "Copied";
    setTimeout(() => { button.textContent = "Copy"; }, 2000);
  });
}

for (const mini of document.querySelectorAll("[data-mini]")) {
  const button = mini.querySelector("button");
  button.addEventListener("click", () => {
    const open = mini.classList.toggle("open");
    button.setAttribute("aria-expanded", String(open));
    button.querySelector("span").textContent = open ? button.dataset.hide : button.dataset.show;
  });
}

const visitorId = async () => {
  for (let i = 0; i < 20 && !window.virling?.vid; i++) await new Promise((r) => setTimeout(r, 100));
  return window.virling?.vid?.() || null;
};

for (const card of document.querySelectorAll("[data-preorder]")) {
  const SDK = "https://cdn.jsdelivr.net/npm/dodopayments-checkout@2.0.0/dist/index.js";
  const spots = document.querySelector("[data-spots]");
  const section = card.closest("section");
  const setState = (state) => { card.dataset.state = state; };
  let started = false;
  let timer = null;

  const status = async (payment) => {
    const res = await fetch("/api/preorder/status" + (payment ? "?payment_id=" + encodeURIComponent(payment) : ""), { cache: "no-store" });
    if (!res.ok) throw new Error("status " + res.status);
    return res.json();
  };
  const showSpots = (body) => {
    if (!spots || typeof body.spots_left !== "number") return;
    spots.querySelector("b").textContent = body.spots_left;
    spots.hidden = false;
  };
  const loadSdk = () => new Promise((resolve, reject) => {
    if (window.DodoPaymentsCheckout) return resolve(window.DodoPaymentsCheckout.DodoPayments);
    const s = document.createElement("script");
    s.src = SDK;
    s.onload = () => window.DodoPaymentsCheckout ? resolve(window.DodoPaymentsCheckout.DodoPayments) : reject(new Error("sdk"));
    s.onerror = () => reject(new Error("sdk"));
    document.head.appendChild(s);
  });
  const fail = () => {
    clearTimeout(timer);
    if (card.dataset.state !== "paid") setState("error");
  };

  const paid = async (payment, email) => {
    clearTimeout(timer);
    setState("paid");
    if (email) card.querySelector("[data-paid-email]").textContent = " to " + email;
    window.virling && virling("goal", "preorder_agency");
    const until = Date.now() + 20000;
    while (payment && Date.now() < until) {
      try {
        const body = await status(payment);
        showSpots(body);
        if (body.number) {
          card.querySelector("[data-paid-title]").textContent = "You're founding agency #" + body.number + ".";
          return;
        }
      } catch {}
      await new Promise((r) => setTimeout(r, 2000));
    }
  };
  const handleReturn = (url) => {
    const params = new URL(url, location.href).searchParams;
    const state = params.get("status");
    if (state && !["active", "succeeded"].includes(state)) return fail();
    paid(params.get("payment_id"), params.get("email"));
  };

  const start = async () => {
    if (started) return;
    started = true;
    window.virling && virling("goal", "checkout_started");
    setState("loading");
    timer = setTimeout(fail, 20000);
    status().then(showSpots).catch(() => {});
    try {
      const [Dodo, res] = await Promise.all([
        loadSdk(),
        visitorId().then((visitor_id) => fetch("/api/preorder/session", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ visitor_id, code: new URLSearchParams(location.search).get("code") }) })),
      ]);
      const body = await res.json();
      if (!res.ok || !body.checkout_url) throw new Error(body.error || "session");
      Dodo.Initialize({
        mode: "live",
        displayType: "inline",
        onEvent: (event) => {
          const type = event.event_type;
          if (type === "checkout.form_ready" || type === "checkout.opened") {
            clearTimeout(timer);
            if (card.dataset.state === "loading") setState("checkout");
          } else if (type === "checkout.error") {
            if (card.dataset.state === "loading") fail();
          } else if (type === "checkout.redirect_requested") {
            const url = event.data?.message?.redirect_to || event.data?.redirect_to || event.data?.url || event.data?.message?.url;
            if (!url) return;
            if (new URL(url, location.href).origin === location.origin) handleReturn(url);
            else location.href = url;
          } else if (type === "checkout.status") {
            const state = event.data?.message?.status || event.data?.status;
            if (["succeeded", "active"].includes(state)) setTimeout(() => card.dataset.state === "paid" || paid(null, null), 4000);
          }
        },
      });
      Dodo.Checkout.open({
        checkoutUrl: body.checkout_url,
        elementId: "dodo-inline-checkout",
        options: { payButtonText: "Reserve your spot for $29", fontSize: "md", fontWeight: "normal", manualRedirect: true },
      });
    } catch {
      fail();
    }
  };

  card.querySelector("[data-retry]").addEventListener("click", () => {
    window.DodoPaymentsCheckout?.DodoPayments.Checkout.close();
    started = false;
    start();
  });
  card.querySelector("[data-back]").addEventListener("click", (e) => {
    e.preventDefault();
    window.scrollTo({ top: 0, behavior: "smooth" });
  });

  const query = new URLSearchParams(location.search);
  if (query.get("preorder") === "done" || query.get("payment_id")) {
    history.replaceState(null, "", location.pathname + "#pricing");
    section.scrollIntoView();
    handleReturn("?" + query.toString());
  } else if ("IntersectionObserver" in window) {
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) { observer.disconnect(); start(); }
    }, { rootMargin: "600px 0px" });
    observer.observe(section);
    for (const link of document.querySelectorAll('a[href="#pricing"]')) link.addEventListener("click", start);
  } else {
    start();
  }
}

// Personal founding spots go to Dodo's hosted checkout and come back to /pricing.
for (const box of document.querySelectorAll("[data-claim]")) {
  const plan = box.dataset.claim;
  const button = box.querySelector("[data-claim-start]");
  const error = box.querySelector("[data-claim-error]");
  const spots = box.querySelector("[data-claim-spots]");
  const failed = "Checkout couldn't load. Nothing was charged.";
  const status = async (payment) => {
    const res = await fetch("/api/preorder/status?plan=" + plan + (payment ? "&payment_id=" + encodeURIComponent(payment) : ""), { cache: "no-store" });
    if (!res.ok) throw new Error("status " + res.status);
    return res.json();
  };
  const showSpots = (body) => {
    if (typeof body.spots_left !== "number") return;
    spots.querySelector("b").textContent = body.spots_left;
    spots.hidden = false;
  };
  status().then(showSpots).catch(() => {});

  button.addEventListener("click", async () => {
    button.disabled = true;
    error.textContent = "";
    window.virling && virling("goal", "checkout_started");
    try {
      const visitor_id = await visitorId();
      const res = await fetch("/api/preorder/session", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ plan, visitor_id, code: new URLSearchParams(location.search).get("code") }) });
      const body = await res.json();
      if (!res.ok || !body.checkout_url) throw new Error(body.error || "session");
      location.href = body.checkout_url;
    } catch {
      error.textContent = failed;
      button.disabled = false;
    }
  });

  const query = new URLSearchParams(location.search);
  const payment = query.get("payment_id");
  if (box.hasAttribute("data-claim-confirm") && payment && (query.get("preorder") === "done" || query.get("payment_id"))) {
    history.replaceState(null, "", location.pathname);
    const state = query.get("status");
    if (state && !["active", "succeeded"].includes(state)) {
      error.textContent = failed;
    } else {
      const show = (title, text) => {
        const paid = document.createElement("div");
        paid.className = "joined";
        paid.setAttribute("role", "status");
        paid.tabIndex = -1;
        paid.innerHTML = '<span class="check"><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12l5 5 9-10"/></svg></span><span><b></b> <span class="muted"></span></span>';
        paid.querySelector("b").textContent = title;
        paid.querySelector(".muted").textContent = text;
        box.append(paid);
        box.classList.add("is-paid");
        paid.focus();
      };
      (async () => {
        const until = Date.now() + 20000;
        while (Date.now() < until) {
          try {
            const body = await status(payment);
            if (body.number) {
              window.virling && virling("goal", "preorder_" + plan);
              show("You're founding customer #" + body.number + ".", "Your spot is held and your first month after launch is paid. A welcome email is on its way.");
              return;
            }
          } catch {}
          await new Promise((r) => setTimeout(r, 2000));
        }
        show("We're confirming your payment.", "This can take a minute. A welcome email follows once it clears.");
      })();
    }
  }
}

// Example dates follow the calendar so the demo never looks stale.
const dayFmt = (d) => d.toLocaleDateString("en-US", { month: "short", day: "numeric" });
const daysAgo = (n) => { const d = new Date(); d.setDate(d.getDate() - n); return d; };
const axis = document.querySelectorAll(".pm-x span");
if (axis.length === 3) [29, 14, 0].forEach((n, i) => { axis[i].textContent = dayFmt(daysAgo(n)); });
const week = document.querySelector("[data-week]");
if (week) {
  const sunday = daysAgo(new Date().getDay() || 7);
  const monday = new Date(sunday); monday.setDate(sunday.getDate() - 6);
  const end = monday.getMonth() === sunday.getMonth() ? sunday.getDate() : dayFmt(sunday);
  week.textContent = "Revenue report, " + dayFmt(monday) + " to " + end;
}
const shiftDates = (box) => {
  const months = { Sep: 8, Oct: 9 };
  const offset = Math.round((daysAgo(3) - new Date(2026, 9, 9)) / 864e5);
  for (const t of box.querySelectorAll("text")) {
    t.textContent = t.textContent.replace(/\b(Sep|Oct) (\d{1,2})\b/, (_, m, d) => dayFmt(new Date(2026, months[m], +d + offset)));
  }
};

// Diagrams load just before they scroll into view, keeping the first HTML small.
const loadDiagrams = (box) => {
  for (const el of box.querySelectorAll("[data-dg]:not([data-loaded])")) {
    el.dataset.loaded = "1";
    fetch(el.dataset.dg).then((r) => r.text()).then((svg) => { el.innerHTML = svg; el.style.aspectRatio = ""; shiftDates(el); }).catch(() => {});
  }
};
const boxes = new Set([...document.querySelectorAll("[data-dg]")].map((el) => el.parentElement));
if ("IntersectionObserver" in window) {
  const seen = new IntersectionObserver((entries) => {
    for (const e of entries) if (e.isIntersecting) { seen.unobserve(e.target); loadDiagrams(e.target); }
  }, { rootMargin: "800px 0px" });
  boxes.forEach((box) => seen.observe(box));
} else {
  boxes.forEach(loadDiagrams);
}

document.addEventListener("click", (e) => {
  const el = e.target.closest && e.target.closest("[data-goal]");
  if (el && window.virling) virling("goal", el.dataset.goal);
}, true);

const modelContext = document.modelContext || navigator.modelContext;
if (modelContext && modelContext.registerTool) {
  const stop = new AbortController();
  addEventListener("pagehide", () => stop.abort());
  const say = (t) => ({ content: [{ type: "text", text: t }] });
  const pages = { home: "/", pricing: "/pricing", "self-host": "/self-host", agency: "/agency", demo: "/demo", integrations: "/integrations", changelog: "/changelog", blog: "/blog" };
  modelContext.registerTool({
    name: "open_page",
    description: "Open a page of the Virling site.",
    inputSchema: { type: "object", properties: { page: { type: "string", enum: Object.keys(pages) } }, required: ["page"] },
    execute: async ({ page }) => {
      if (!pages[page]) return say("Unknown page.");
      location.href = pages[page];
      return say("Opening " + page + ".");
    },
  }, { signal: stop.signal });
  modelContext.registerTool({
    name: "read_page",
    description: "Read the page that is open now as Markdown.",
    inputSchema: { type: "object", properties: {} },
    execute: async () => {
      const path = location.pathname.replace(/\/$/, "") || "/index";
      const res = await fetch(path + ".md");
      return say(res.ok ? await res.text() : "No Markdown copy of this page.");
    },
  }, { signal: stop.signal });
  modelContext.registerTool({
    name: "start_beta",
    description: "Open the sign-in page of the hosted Virling beta. A person signs in there with their email; no application is needed.",
    inputSchema: { type: "object", properties: {} },
    execute: async () => {
      location.href = "https://beta.virling.com/login";
      return say("Opening the beta sign-in page.");
    },
  }, { signal: stop.signal });
}
