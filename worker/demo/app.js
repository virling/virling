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

for (const wrap of document.querySelectorAll("[data-menu]")) {
  const trigger = wrap.querySelector("button");
  const menu = wrap.querySelector(".menu");
  const open = (state) => {
    menu.hidden = !state;
    trigger.setAttribute("aria-expanded", String(state));
    if (state) (menu.querySelector("[aria-current]") || menu.querySelector("a"))?.focus();
  };
  trigger.addEventListener("click", () => open(menu.hidden));
  menu.addEventListener("keydown", (e) => {
    const links = [...menu.querySelectorAll("a")];
    const i = links.indexOf(document.activeElement);
    if (e.key === "ArrowDown") { e.preventDefault(); links[(i + 1) % links.length].focus(); }
    else if (e.key === "ArrowUp") { e.preventDefault(); links[(i - 1 + links.length) % links.length].focus(); }
    else if (e.key === "Escape") { open(false); trigger.focus(); }
  });
  document.addEventListener("click", (e) => { if (!wrap.contains(e.target)) open(false); });
}

document.querySelector("[data-burger]")?.addEventListener("click", (e) => {
  const side = document.getElementById("side");
  side.classList.toggle("open");
  e.currentTarget.setAttribute("aria-expanded", String(side.classList.contains("open")));
});

for (const form of document.querySelectorAll("[data-providers]")) {
  form.addEventListener("change", (e) => {
    if (e.target.name !== "provider") return;
    for (const panel of form.querySelectorAll("[data-provider]")) {
      const on = panel.dataset.provider === e.target.value;
      panel.hidden = !on;
      panel.querySelector("input").disabled = !on;
    }
  });
}

for (const el of document.querySelectorAll("[data-reload]")) {
  setTimeout(() => location.reload(), Number(el.dataset.reload));
}

const drawer = document.getElementById("drawer");
if (drawer) {
  const back = document.querySelector(".drawer-back");
  const body = drawer.querySelector("[data-drawer-body]");
  const pos = drawer.querySelector("[data-drawer-pos]");
  const steps = drawer.querySelectorAll("[data-drawer-step]");
  const rows = () => [...document.querySelectorAll("a[data-drawer]")];
  let current = null;
  const close = () => {
    drawer.hidden = true;
    back.hidden = true;
    current?.classList.remove("sel");
    current?.focus();
    current = null;
  };
  const show = async (row) => {
    const res = await fetch(row.dataset.drawer || row.getAttribute("href"), { headers: { "x-drawer": "1" } });
    if (!res.ok) return (location.href = row.getAttribute("href"));
    body.innerHTML = await res.text();
    current?.classList.remove("sel");
    current = row;
    row.classList.add("sel");
    const list = rows();
    const i = list.indexOf(row);
    const box = row.closest("[data-rows]");
    pos.textContent = box ? `Payment ${Number(box.dataset.offset) + i + 1} of ${box.dataset.total}` : "";
    steps[0].disabled = i <= 0;
    steps[1].disabled = i >= list.length - 1;
    drawer.hidden = false;
    back.hidden = false;
    drawer.querySelector(".drawer-x").focus();
  };
  const step = (n) => {
    const list = rows();
    const next = list[list.indexOf(current) + n];
    if (next) show(next);
  };
  steps.forEach((b) => b.addEventListener("click", () => step(Number(b.dataset.drawerStep))));
  document.addEventListener("click", (e) => {
    if (e.target.closest("[data-drawer-close]")) return close();
    if (e.metaKey || e.ctrlKey || e.shiftKey) return;
    const link = e.target.closest("a[data-drawer]");
    if (!link) return;
    e.preventDefault();
    show(link);
  });
  document.addEventListener("keydown", (e) => {
    if (drawer.hidden) return;
    if (e.key === "Escape") close();
    else if (e.key === "ArrowDown" || e.key === "j") { e.preventDefault(); step(1); }
    else if (e.key === "ArrowUp" || e.key === "k") { e.preventDefault(); step(-1); }
  });
}

const confirmBox = document.getElementById("confirm");
if (confirmBox) {
  let pending = null;
  const ok = confirmBox.querySelector("[data-confirm-ok]");
  const okLabel = ok.textContent;
  for (const form of document.querySelectorAll("form[data-confirm]")) {
    form.addEventListener("submit", (e) => {
      e.preventDefault();
      pending = form;
      confirmBox.querySelector("[data-confirm-text]").textContent = form.dataset.confirm;
      ok.textContent = form.dataset.confirmOk || okLabel;
      confirmBox.showModal();
    });
  }
  confirmBox.querySelector("[data-confirm-cancel]").addEventListener("click", () => confirmBox.close());
  ok.addEventListener("click", () => { confirmBox.close(); pending?.submit(); });
}

document.querySelector("[data-print]")?.addEventListener("click", () => window.print());

for (const input of document.querySelectorAll("input[data-swatch]")) {
  const swatch = document.getElementById(input.dataset.swatch);
  input.addEventListener("input", () => {
    if (/^#[0-9a-f]{6}$/i.test(input.value.trim())) swatch.style.background = input.value.trim();
  });
}
