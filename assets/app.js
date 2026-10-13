for (const select of document.querySelectorAll("[data-select]")) {
  const trigger = select.querySelector("button[aria-haspopup]");
  const menu = select.querySelector(".menu");
  const search = menu.querySelector("input[type=search]");
  const hidden = select.querySelector("input[type=hidden]");
  const label = select.querySelector("[data-label]");
  const options = [...menu.querySelectorAll("[role=option]")];
  const visible = () => options.filter((o) => !o.hidden);
  let active = -1;
  const highlight = (i) => {
    const list = visible();
    if (!list.length) return;
    active = (i + list.length) % list.length;
    options.forEach((o) => o.classList.remove("active"));
    list[active].classList.add("active");
    list[active].scrollIntoView({ block: "nearest" });
  };
  const open = (state) => {
    menu.hidden = !state;
    trigger.setAttribute("aria-expanded", String(state));
    if (!state) return;
    if (search) { search.value = ""; options.forEach((o) => (o.hidden = false)); search.focus(); }
    highlight(Math.max(0, visible().findIndex((o) => o.getAttribute("aria-selected") === "true")));
  };
  const pick = (o) => {
    options.forEach((x) => x.setAttribute("aria-selected", String(x === o)));
    hidden.value = o.dataset.value;
    label.textContent = o.textContent.trim();
    open(false);
    trigger.focus();
  };
  trigger.addEventListener("click", () => open(menu.hidden));
  options.forEach((o) => o.addEventListener("click", () => pick(o)));
  search?.addEventListener("input", () => {
    const q = search.value.toLowerCase();
    options.forEach((o) => (o.hidden = !o.textContent.toLowerCase().includes(q)));
    highlight(0);
  });
  menu.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") { e.preventDefault(); highlight(active + 1); }
    else if (e.key === "ArrowUp") { e.preventDefault(); highlight(active - 1); }
    else if (e.key === "Enter") { e.preventDefault(); const o = visible()[active]; if (o) pick(o); }
    else if (e.key === "Escape") { open(false); trigger.focus(); }
  });
  document.addEventListener("click", (e) => { if (!select.contains(e.target)) open(false); });
}

for (const box of document.querySelectorAll("[data-tabs]")) {
  const tabs = [...box.querySelectorAll("[role=tab]")];
  const copy = box.querySelector("[data-copy]");
  tabs.forEach((tab) => tab.addEventListener("click", () => {
    tabs.forEach((t) => {
      t.setAttribute("aria-selected", String(t === tab));
      document.getElementById(t.getAttribute("aria-controls")).hidden = t !== tab;
    });
    if (copy) copy.textContent = "Copy";
  }));
  copy?.addEventListener("click", async () => {
    const lines = box.querySelector("pre:not([hidden])").textContent.split("\n");
    const code = lines.filter((l) => !/^\s*(\/\/|<!--|\s+before|\s+Webflow)/.test(l)).join("\n").trim();
    await navigator.clipboard.writeText(code);
    copy.textContent = "Copied";
  });
}

for (const live of document.querySelectorAll("[data-live]")) {
  const check = async () => {
    try {
      const res = await fetch(live.dataset.live, { headers: { accept: "application/json" } });
      const e = res.ok ? await res.json() : null;
      if (!e) return setTimeout(check, 3000);
      live.classList.add("got");
      live.querySelectorAll("[data-waiting]").forEach((el) => (el.hidden = true));
      live.querySelectorAll("[data-got]").forEach((el) => (el.hidden = false));
      live.querySelector("[data-detail]").textContent =
        [e.path, e.country, e.device, e.referrer ? "from " + e.referrer : "direct"].filter(Boolean).join(" · ");
    } catch {
      setTimeout(check, 5000);
    }
  };
  check();
}
