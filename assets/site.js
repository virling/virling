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
        body: JSON.stringify({ email, wedge: root.dataset.wedge, clients, referrer: document.referrer || null }),
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

for (const term of document.querySelectorAll("[data-tabs]")) {
  const tabs = [...term.querySelectorAll("[role=tab]")];
  const copy = term.querySelector("[data-copy]");
  tabs.forEach((tab) =>
    tab.addEventListener("click", () => {
      tabs.forEach((t) => {
        t.setAttribute("aria-selected", String(t === tab));
        document.getElementById(t.getAttribute("aria-controls")).hidden = t !== tab;
      });
      copy.textContent = "Copy";
    })
  );
  copy.addEventListener("click", async () => {
    const panel = term.querySelector("pre:not([hidden])");
    const commands = panel.textContent.split("\n").filter((l) => l.startsWith("$ ") || l.startsWith("    ")).map((l) => l.replace(/^\$ /, "")).join("\n");
    await navigator.clipboard.writeText(commands);
    copy.textContent = "Copied";
  });
}
