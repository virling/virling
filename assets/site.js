for (const select of document.querySelectorAll("[data-select]")) {
  const trigger = select.querySelector("button[aria-haspopup]");
  const list = select.querySelector("[role=listbox]");
  const options = [...list.querySelectorAll("[role=option]")];
  let active = 0;
  const open = (state) => {
    list.hidden = !state;
    trigger.setAttribute("aria-expanded", String(state));
    if (state) focusOption(Math.max(0, options.findIndex((o) => o.getAttribute("aria-selected") === "true")));
  };
  const focusOption = (i) => {
    active = (i + options.length) % options.length;
    options.forEach((o, j) => o.classList.toggle("active", j === active));
    options[active].focus();
  };
  const pick = (option) => {
    options.forEach((o) => o.setAttribute("aria-selected", String(o === option)));
    trigger.querySelector("span").textContent = option.textContent;
    trigger.classList.add("chosen");
    select.dataset.value = option.textContent;
    open(false);
    trigger.focus();
  };
  trigger.addEventListener("click", () => open(list.hidden));
  options.forEach((o) => o.addEventListener("click", () => pick(o)));
  list.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") { e.preventDefault(); focusOption(active + 1); }
    else if (e.key === "ArrowUp") { e.preventDefault(); focusOption(active - 1); }
    else if (e.key === "Escape") { open(false); trigger.focus(); }
  });
  trigger.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") { e.preventDefault(); open(true); }
  });
  document.addEventListener("click", (e) => {
    if (!select.contains(e.target)) open(false);
  });
}

for (const root of document.querySelectorAll("[data-wedge]")) {
  const form = root.querySelector("form.waitlist");
  const error = form.querySelector(".form-error");
  const button = form.querySelector("button[type=submit]");
  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    const email = form.email.value.trim();
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)) {
      error.textContent = "Enter a valid email.";
      form.email.focus();
      return;
    }
    const clients = root.querySelector("[data-select]")?.dataset.value || null;
    error.textContent = "";
    button.disabled = true;
    try {
      const res = await fetch("/api/waitlist", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ email, wedge: root.dataset.wedge, clients, referrer: document.referrer || null, visitor_id: window.virling?.vid?.() || null }),
      });
      const body = await res.json();
      if (!res.ok) throw new Error(body.error || "Something went wrong. Try again.");
      root.classList.add("is-joined");
      window.virling && virling("goal", "waitlist_" + root.dataset.wedge);
    } catch (err) {
      error.textContent = err.message;
    } finally {
      button.disabled = false;
    }
  });
}

for (const button of document.querySelectorAll("[data-copy-snippet]")) {
  const snippet = button.parentElement.querySelector("[data-snippet]");
  button.addEventListener("click", async () => {
    await navigator.clipboard.writeText(snippet.textContent);
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

for (const card of document.querySelectorAll("[data-preorder]")) {
  const spots = document.querySelector("[data-spots]");
  const section = card.closest("section");
  const setState = (state) => { card.dataset.state = state; };
  let started = false;
  let timer = null;

  const visitorId = async () => {
    for (let i = 0; i < 20 && !window.virling?.vid; i++) await new Promise((r) => setTimeout(r, 100));
    return window.virling?.vid?.() || null;
  };
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
    setState("loading");
    timer = setTimeout(fail, 20000);
    try {
      const visitor_id = await visitorId();
      const res = await fetch("/api/preorder/session", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ visitor_id, code: new URLSearchParams(location.search).get("code") }) });
      const body = await res.json();
      if (!res.ok || !body.checkout_url) throw new Error(body.error || "session");
      location.href = body.checkout_url;
    } catch {
      fail();
    }
  };

  card.querySelector("[data-start]").addEventListener("click", start);
  card.querySelector("[data-retry]").addEventListener("click", () => {
    started = false;
    start();
  });
  card.querySelector("[data-back]").addEventListener("click", (e) => {
    e.preventDefault();
    window.scrollTo({ top: 0, behavior: "smooth" });
  });

  const query = new URLSearchParams(location.search);
  status().then(showSpots).catch(() => {});
  if (query.get("preorder") === "done" || query.get("payment_id")) {
    history.replaceState(null, "", location.pathname + "#pricing");
    section.scrollIntoView();
    handleReturn("?" + query.toString());
  }
}
