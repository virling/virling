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

for (const select of document.querySelectorAll("[data-select]")) {
  const trigger = select.querySelector("button[aria-haspopup]");
  const menu = select.querySelector(".menu");
  const search = menu.querySelector("input[type=search]");
  const hidden = select.querySelector("input[type=hidden]");
  const label = select.querySelector("[data-label]");
  const options = [...menu.querySelectorAll("[role=option]")];
  const visible = () => options.filter((o) => !o.hidden);
  const chosen = options.find((o) => o.getAttribute("aria-selected") === "true");
  if (chosen) label.textContent = chosen.textContent.trim();
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
  const panels = [...box.querySelectorAll("[role=tabpanel]")];
  tabs.forEach((tab, i) => {
    tab.addEventListener("click", () => {
      tabs.forEach((t, j) => {
        t.setAttribute("aria-selected", String(i === j));
        t.setAttribute("tabindex", i === j ? "0" : "-1");
      });
      panels.forEach((p, j) => {
        p.hidden = i !== j;
      });
    });
  });
}

const polarCalc = document.getElementById("polar-fee-calc");
if (polarCalc) {
  const amountInput = polarCalc.querySelector("#polar-amount");
  const currencySelect = polarCalc.querySelector("#polar-currency");
  const typeSelect = polarCalc.querySelector("#polar-type");
  const outFee = polarCalc.querySelector("#polar-out-fee");
  const outPayout = polarCalc.querySelector("#polar-out-payout");
  const outEffective = polarCalc.querySelector("#polar-out-effective");

  const compute = () => {
    const amount = parseFloat(amountInput.value) || 0;
    const currency = currencySelect.value;
    const isDigital = typeSelect.value === "digital";

    // Polar pricing structure (published as of Feb 2025):
    // Polar acts as Merchant of Record. Flat platform fee + payment processing.
    // Published rate for Polar: 4% flat fee on transactions.
    const feeRate = 0.04;
    const fee = amount * feeRate;
    const payout = Math.max(0, amount - fee);
    const effective = amount > 0 ? (fee / amount) * 100 : 0;

    const fmt = (val) => {
      return new Intl.NumberFormat("en-US", { style: "currency", currency: currency }).format(val);
    };

    outFee.textContent = fmt(fee);
    outPayout.textContent = fmt(payout);
    outEffective.textContent = effective.toFixed(2) + "%";
  };

  [amountInput, currencySelect, typeSelect].forEach((el) => {
    el.addEventListener("input", compute);
    el.addEventListener("change", compute);
  });
  compute();
}