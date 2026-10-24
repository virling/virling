(() => {
  const root = document.getElementById("globe-root");
  if (!root) return;
  const canvas = root.querySelector("canvas");
  const tip = root.querySelector(".gl-tip");
  const replayBox = root.querySelector(".gl-replay");
  const idleNote = root.querySelector(".gl-idle");
  const chips = root.querySelector(".gl-chips");
  const reduce = matchMedia("(prefers-reduced-motion: reduce)");
  const RAD = Math.PI / 180;
  const usd = new Intl.NumberFormat("en-US", { style: "currency", currency: "USD" });

  let data = JSON.parse(document.getElementById("globe-data").textContent);
  let outline = null;
  let lon0 = 10 * RAD, lat0 = 22 * RAD, zoom = 1;
  let mode = "visits";
  let selected = "";
  let glide = null;
  let drag = null;
  let lastInput = 0;
  let onscreen = true;
  let replay = null;
  let gl = null;
  let placed = [];
  const seenPays = new Set();
  const colors = {};

  const h = (tag, cls, text) => {
    const el = document.createElement(tag);
    if (cls) el.className = cls;
    if (text != null) el.textContent = text;
    return el;
  };
  const readColors = () => {
    const s = getComputedStyle(document.documentElement);
    for (const n of ["soft", "rule", "rule-2", "paper", "ink", "ink-3", "visitors", "revenue", "sans"]) {
      colors[n] = s.getPropertyValue("--" + n).trim();
    }
  };
  const wake = () => gl?.wake();

  const prepare = (o) => {
    outline = { pts: o.pts, shapes: [] };
    for (const [cc, rings] of o.shapes) {
      const out = [];
      for (const flat of rings) {
        const n = flat.length / 2;
        const lon = new Float32Array(n), lat = new Float32Array(n);
        let x = 0, y = 0;
        for (let i = 0; i < n; i++) {
          x += flat[i * 2]; y += flat[i * 2 + 1];
          lat[i] = y / 10;
          const l = x / 10;
          lon[i] = i && Math.abs(l - lon[i - 1]) > 180 ? l + (l < lon[i - 1] ? 360 : -360) : l;
        }
        out.push({ n, lon, lat });
      }
      outline.shapes.push({ cc, rings: out });
    }
  };

  const place = () => {
    const seen = new Map();
    placed = data.visitors.map((v) => {
      const pt = outline?.pts[v.cc];
      if (!pt) return null;
      const k = seen.get(v.cc) || 0;
      seen.set(v.cc, k + 1);
      const rr = 1.4 * Math.sqrt(k), a = k * 2.4;
      const lat = pt[1] + rr * Math.sin(a);
      return { v, lon: pt[0] + (rr * Math.cos(a)) / Math.max(Math.cos(lat * RAD), 0.3), lat };
    });
  };

  const shade = () => {
    const by = new Map();
    let max = 0;
    for (const c of data.countries) {
      const val = mode === "revenue" ? c.rpv : c.visitors;
      if (val > 0) by.set(c.cc, val);
      if (val > max) max = val;
    }
    return { by, max };
  };

  const goTo = (lon, lat) => {
    const to = { lon: lon * RAD, lat: Math.max(-1.2, Math.min(1.2, lat * RAD * 0.8)) };
    if (reduce.matches) {
      lon0 = to.lon; lat0 = to.lat;
      return wake();
    }
    let d = to.lon - lon0;
    while (d > Math.PI) d -= 2 * Math.PI;
    while (d < -Math.PI) d += 2 * Math.PI;
    glide = { t0: performance.now(), from: { lon: lon0, lat: lat0 }, dl: d, dp: to.lat - lat0 };
    wake();
  };
  const burst = (cc, label, delay = 0) => {
    const pt = outline?.pts[cc];
    if (pt) gl?.burst(pt, label, performance.now() + delay);
  };
  const zoomBy = (k) => {
    zoom = Math.max(0.8, Math.min(3, zoom * k));
    wake();
  };

  // The globe is three.js, loaded only on this page. Without WebGL the lists stay and the globe is left out.
  const flat = () => {
    root.classList.add("gl-flat");
    gl = null;
  };
  const init3d = async () => {
    const THREE = await import(root.dataset.three);
    const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: true, powerPreference: "low-power" });
    canvas.addEventListener("webglcontextlost", (e) => { e.preventDefault(); flat(); });
    const scene = new THREE.Scene();
    const cam = new THREE.OrthographicCamera(-1, 1, 1, -1, 0.1, 20);
    cam.position.z = 6;
    const globe = new THREE.Group();
    scene.add(globe);

    const TW = 2048, TH = 1024;
    const tc = document.createElement("canvas");
    tc.width = TW; tc.height = TH;
    const t2 = tc.getContext("2d");
    const tex = new THREE.CanvasTexture(tc);
    tex.colorSpace = THREE.SRGBColorSpace;
    tex.anisotropy = Math.min(8, renderer.capabilities.getMaxAnisotropy());
    globe.add(new THREE.Mesh(new THREE.SphereGeometry(1, 96, 64), new THREE.MeshBasicMaterial({ map: tex })));
    const rim = new THREE.Mesh(new THREE.RingGeometry(0.995, 1.005, 160), new THREE.MeshBasicMaterial({ color: 0xffffff, transparent: true }));
    rim.position.z = 1.2;
    scene.add(rim);

    const vec = (lon, lat) => {
      const la = lat * RAD, lo = lon * RAD;
      return new THREE.Vector3(Math.cos(la) * Math.cos(lo), Math.sin(la), -Math.cos(la) * Math.sin(lo));
    };
    const px = (lon) => ((lon + 180) / 360) * TW;
    const py = (lat) => ((90 - lat) / 180) * TH;

    const paint = () => {
      t2.globalAlpha = 1;
      t2.fillStyle = colors.soft;
      t2.fillRect(0, 0, TW, TH);
      t2.strokeStyle = colors["rule-2"];
      t2.lineWidth = 2;
      t2.beginPath();
      for (let lo = -150; lo <= 180; lo += 30) { t2.moveTo(px(lo), 0); t2.lineTo(px(lo), TH); }
      for (let la = -60; la <= 60; la += 30) { t2.moveTo(0, py(la)); t2.lineTo(TW, py(la)); }
      t2.stroke();
      if (outline) {
        const { by, max } = shade();
        const accent = mode === "revenue" ? colors.revenue : colors.visitors;
        t2.lineWidth = 1.4;
        t2.strokeStyle = colors.paper;
        for (const s of outline.shapes) {
          const val = by.get(s.cc);
          for (const off of [-TW, 0, TW]) {
            t2.beginPath();
            for (const g of s.rings) {
              for (let i = 0; i < g.n; i++) {
                const x = px(g.lon[i]) + off, y = py(g.lat[i]);
                if (i) t2.lineTo(x, y); else t2.moveTo(x, y);
              }
              t2.closePath();
            }
            t2.globalAlpha = 0.11;
            t2.fillStyle = colors.ink;
            t2.fill();
            if (val) {
              t2.globalAlpha = 0.18 + 0.62 * Math.sqrt(val / max);
              t2.fillStyle = accent;
              t2.fill();
            }
            t2.globalAlpha = 1;
            t2.stroke();
          }
        }
      }
      tex.needsUpdate = true;
      rim.material.color.set(colors.rule);
    };

    // Visitors are sprites carrying their animal face.
    const faces = new Map();
    const faceImg = (i) => {
      const svg = data.faces[i];
      let img = faces.get(svg);
      if (!img) {
        img = new Image();
        img.onload = () => markers();
        img.src = "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg.replace("<svg ", '<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40" '));
        faces.set(svg, img);
      }
      return img;
    };
    const avatar = (v, on) => {
      const c = document.createElement("canvas");
      c.width = c.height = 96;
      const x = c.getContext("2d");
      x.beginPath();
      x.arc(48, 48, 46, 0, 2 * Math.PI);
      x.fillStyle = on ? colors.visitors : v.paid ? colors.revenue : colors.paper;
      x.fill();
      x.beginPath();
      x.arc(48, 48, 39, 0, 2 * Math.PI);
      x.clip();
      const img = faceImg(v.face);
      if (img.complete && img.naturalWidth) x.drawImage(img, 9, 9, 78, 78);
      const t = new THREE.CanvasTexture(c);
      t.colorSpace = THREE.SRGBColorSpace;
      return t;
    };
    let sprites = [];
    const markers = () => {
      for (const s of sprites) {
        globe.remove(s.sprite);
        s.sprite.material.map.dispose();
        s.sprite.material.dispose();
      }
      sprites = [];
      for (const p of placed) {
        if (!p) continue;
        const sprite = new THREE.Sprite(new THREE.SpriteMaterial({ map: avatar(p.v, p.v.id === selected), transparent: true }));
        const n = vec(p.lon, p.lat);
        sprite.position.copy(n).multiplyScalar(1.02);
        globe.add(sprite);
        sprites.push({ sprite, n, v: p.v });
      }
      wake();
    };

    // Rings pulse where a payment lands; soft grey ones drift by while the site is idle.
    const ringGeo = new THREE.RingGeometry(0.9, 1, 64);
    const dotGeo = new THREE.CircleGeometry(1, 20);
    const bursts = [];
    const flatZ = new THREE.Vector3(0, 0, 1);
    const mkRing = (n, color) => {
      const m = new THREE.Mesh(ringGeo, new THREE.MeshBasicMaterial({ color, transparent: true, depthWrite: false, side: THREE.DoubleSide }));
      m.position.copy(n).multiplyScalar(1.006);
      m.quaternion.setFromUnitVectors(flatZ, n);
      m.visible = false;
      globe.add(m);
      return m;
    };
    const drop = (b) => {
      for (const m of b.meshes) {
        globe.remove(m);
        m.material.dispose();
      }
      b.chip?.remove();
    };
    const still = () => reduce.matches;
    let W = 1, H = 1;
    const tmp = new THREE.Vector3();
    const toScreen = (n) => {
      tmp.copy(n).applyMatrix4(globe.matrixWorld);
      const facing = tmp.z;
      tmp.project(cam);
      return { x: (tmp.x * 0.5 + 0.5) * W, y: (-tmp.y * 0.5 + 0.5) * H, facing };
    };
    const addBurst = (pt, label, t0) => {
      if (still()) bursts.splice(0).forEach(drop);
      const n = vec(pt[0], pt[1]);
      const b = { n, t0, idle: false, meshes: [mkRing(n, colors.revenue), mkRing(n, colors.revenue)] };
      const dot = new THREE.Mesh(dotGeo, new THREE.MeshBasicMaterial({ color: colors.revenue, transparent: true, depthWrite: false }));
      dot.position.copy(n).multiplyScalar(1.008);
      dot.quaternion.copy(b.meshes[0].quaternion);
      dot.scale.setScalar(0.012);
      dot.visible = false;
      globe.add(dot);
      b.meshes.push(dot);
      b.chip = h("div", "gl-chip", label);
      chips.append(b.chip);
      bursts.push(b);
      wake();
    };
    let nextAmbient = 0;
    const idling = () => !data.visitors.length && !still() && onscreen && outline && !document.hidden;
    const ambient = (now) => {
      if (!idling() || now < nextAmbient || bursts.filter((b) => b.idle).length >= 3) return;
      const shapes = outline.shapes;
      const pt = outline.pts[shapes[Math.floor(Math.random() * shapes.length)].cc];
      if (pt) {
        const n = vec(pt[0], pt[1]);
        bursts.push({ n, t0: now, idle: true, meshes: [mkRing(n, colors["ink-3"]), mkRing(n, colors["ink-3"])] });
      }
      nextAmbient = now + 2400;
    };

    const resize = () => {
      const box = canvas.parentElement.getBoundingClientRect();
      W = Math.max(1, box.width);
      H = Math.max(1, box.height);
      renderer.setPixelRatio(Math.min(2, window.devicePixelRatio || 1));
      renderer.setSize(W, H, false);
      wake();
    };
    const half = () => 1.12 / zoom;
    const view = () => {
      const a = W / H;
      const [hw, hh] = a >= 1 ? [half() * a, half()] : [half(), half() / a];
      Object.assign(cam, { left: -hw, right: hw, top: hh, bottom: -hh });
      cam.updateProjectionMatrix();
      globe.rotation.set(lat0, -Math.PI / 2 - lon0, 0);
      globe.updateMatrixWorld(true);
      return (2 * hh) / H;
    };

    const spinning = (ts) =>
      !still() && onscreen && !document.hidden && !drag && !replay && !selected && ts - lastInput > 2500;
    let raf = 0, lastTs = 0;
    const frame = (ts) => {
      raf = 0;
      const dt = Math.min(64, ts - lastTs || 16);
      lastTs = ts;
      if (glide) {
        const t = Math.min(1, (ts - glide.t0) / 1100);
        const e = t * t * (3 - 2 * t);
        lon0 = glide.from.lon + glide.dl * e;
        lat0 = glide.from.lat + glide.dp * e;
        if (t >= 1) glide = null;
      } else if (spinning(ts)) {
        lon0 += dt * (data.visitors.length ? 0.00007 : 0.00004);
      }
      ambient(ts);
      const upp = view();
      const k = 26 * upp * Math.min(1.5, Math.sqrt(zoom));
      for (const s of sprites) {
        s.sprite.scale.setScalar((s.v.id === selected ? 1.2 : 1) * k);
        const f = toScreen(s.n).facing;
        s.sprite.material.opacity = Math.max(0, Math.min(1, f * 5));
        s.sprite.visible = f > 0.02;
      }
      for (let i = bursts.length - 1; i >= 0; i--) {
        const b = bursts[i];
        const age = ts - b.t0;
        const life = b.idle ? 4600 : 8000;
        if (!still() && age > life) {
          drop(b);
          bursts.splice(i, 1);
          continue;
        }
        const [r1, r2, dot] = b.meshes;
        const s = toScreen(b.n);
        const vis = age >= 0 && s.facing > 0.03;
        const edge = Math.min(1, s.facing * 4);
        const ring = (m, t, a) => {
          m.visible = vis && t > 0 && t < 1;
          if (!m.visible) return;
          m.scale.setScalar((b.idle ? 0.1 : 0.07) * (1 - Math.pow(1 - t, 2)) + 0.012);
          m.material.opacity = a * (1 - t) * edge;
        };
        if (still() && !b.idle) {
          r1.visible = vis;
          r1.scale.setScalar(0.03);
          r1.material.opacity = 0.9 * edge;
          r2.visible = false;
        } else {
          const d = b.idle ? 3400 : 2400;
          ring(r1, age / d, b.idle ? 0.45 : 0.85);
          ring(r2, (age - 800) / d, b.idle ? 0.3 : 0.85);
        }
        const fade = still() ? 1 : Math.min(1, Math.max(0, (life - age) / 1000));
        if (dot) {
          dot.visible = vis;
          dot.material.opacity = fade * edge;
        }
        if (b.chip) {
          const show = vis && fade > 0;
          b.chip.style.display = show ? "" : "none";
          if (show) {
            b.chip.style.opacity = fade * edge;
            const w = b.chip.offsetWidth;
            const left = s.x + 12 + w > W - 4 ? s.x - 12 - w : s.x + 12;
            b.chip.style.transform = `translate(${left}px, ${s.y - 12}px)`;
          }
        }
      }
      renderer.render(scene, cam);
      if (glide || spinning(ts) || drag || (!still() && bursts.length) || idling()) wake();
      else lastTs = 0;
    };

    const hitAt = (x, y) => {
      view();
      let best = null, bd = Infinity;
      for (const s of sprites) {
        const p = toScreen(s.n);
        if (p.facing < 0.1) continue;
        const d = Math.hypot(p.x - x, p.y - y);
        if (d <= 17 && d < bd) {
          best = s.v;
          bd = d;
        }
      }
      return best;
    };

    gl = {
      wake: () => { if (!raf) raf = requestAnimationFrame(frame); },
      burst: addBurst,
      markers,
      paint: () => { paint(); gl.wake(); },
      hitAt,
      size: () => [W, H],
      pxPerRad: () => H / (2 * half()),
    };
    new ResizeObserver(resize).observe(canvas.parentElement);
    new IntersectionObserver((e) => { onscreen = e[0].isIntersecting; wake(); }).observe(canvas);
    resize();
    paint();
  };

  const local = (e) => {
    const b = canvas.getBoundingClientRect();
    return [e.clientX - b.left, e.clientY - b.top];
  };
  canvas.addEventListener("pointerdown", (e) => {
    canvas.setPointerCapture(e.pointerId);
    drag = { x: e.clientX, y: e.clientY, moved: 0 };
    glide = null;
    lastInput = performance.now();
  });
  canvas.addEventListener("pointermove", (e) => {
    if (!gl) return;
    if (drag) {
      const dx = e.clientX - drag.x, dy = e.clientY - drag.y;
      drag.x = e.clientX; drag.y = e.clientY;
      drag.moved += Math.abs(dx) + Math.abs(dy);
      const k = gl.pxPerRad();
      lon0 -= dx / k;
      lat0 = Math.max(-1.3, Math.min(1.3, lat0 + dy / k));
      lastInput = performance.now();
      wake();
      return;
    }
    const [x, y] = local(e);
    const v = gl.hitAt(x, y);
    canvas.style.cursor = v ? "pointer" : "grab";
    if (v) {
      const [W] = gl.size();
      tip.textContent = "";
      tip.append(h("b", "", v.name), h("span", "", [v.place, v.page].filter(Boolean).join(" · ")));
      tip.style.left = Math.min(W - 180, Math.max(4, x + 14)) + "px";
      tip.style.top = Math.max(4, y - 44) + "px";
    }
    tip.hidden = !v;
  });
  canvas.addEventListener("pointerup", (e) => {
    if (!drag) return;
    const click = drag.moved < 5;
    drag = null;
    lastInput = performance.now();
    if (click && gl) {
      const [x, y] = local(e);
      const v = gl.hitAt(x, y);
      if (v) pick(v.id);
      else if (selected) pick("");
    }
    wake();
  });
  canvas.addEventListener("pointercancel", () => { drag = null; });
  canvas.addEventListener("pointerleave", () => { tip.hidden = true; });
  canvas.addEventListener("keydown", (e) => {
    const step = 0.12 / zoom;
    const keys = {
      ArrowLeft: () => (lon0 -= step),
      ArrowRight: () => (lon0 += step),
      ArrowUp: () => (lat0 = Math.min(1.3, lat0 + step)),
      ArrowDown: () => (lat0 = Math.max(-1.3, lat0 - step)),
      "+": () => zoomBy(1.25),
      "=": () => zoomBy(1.25),
      "-": () => zoomBy(0.8),
    };
    if (!keys[e.key]) return;
    e.preventDefault();
    keys[e.key]();
    lastInput = performance.now();
    wake();
  });
  root.querySelector("[data-zoom-in]").addEventListener("click", () => zoomBy(1.25));
  root.querySelector("[data-zoom-out]").addEventListener("click", () => zoomBy(0.8));
  for (const b of root.querySelectorAll("[data-mode]")) {
    b.addEventListener("click", () => {
      mode = b.dataset.mode;
      for (const o of root.querySelectorAll("[data-mode]")) o.setAttribute("aria-pressed", String(o === b));
      legend();
      lists();
      gl?.paint();
    });
  }

  const legend = () => {
    root.querySelector(".gl-legend span").textContent =
      (mode === "revenue" ? "Shaded by revenue per visitor, " : "Shaded by visits, ") + data.label.toLowerCase();
    root.querySelector(".gl-legend i").className = mode === "revenue" ? "rev" : "";
  };

  const pick = (id) => {
    selected = id;
    const p = placed.find((x) => x?.v.id === id);
    if (p) goTo(p.lon, p.lat);
    if (id && data.journeys[id]) startReplay(id);
    else stopReplay();
    lists();
    gl?.markers();
  };

  const locate = (id) => {
    const v = data.visitors.find((x) => x.id === id);
    const pay = data.payments.find((x) => x.vid === id);
    const cc = v?.cc || pay?.cc;
    return { cc, pt: cc && outline?.pts[cc], who: v || pay, pay };
  };

  const STEP = { from: "Arrived from ", page: "Viewed ", goal: "Reached ", left: "Left for ", pay: "Paid " };
  const startReplay = (id) => {
    stopReplay();
    const where = locate(id);
    const steps = data.journeys[id];
    if (!steps || !where.who) return;
    if (where.pt) goTo(where.pt[0], where.pt[1]);
    replayBox.textContent = "";
    const head = h("div", "gl-rh");
    const av = h("span", "av");
    av.innerHTML = data.faces[where.who.face];
    const title = h("div", "gl-rt");
    title.append(h("b", "", where.who.name), h("span", "dim", where.who.place || "Journey"));
    const close = h("button", "gl-x", "Close");
    close.type = "button";
    close.addEventListener("click", () => pick(""));
    head.append(av, title, close);
    const ol = h("ol", "gl-steps");
    const items = steps.map(([k, label]) => {
      const li = h("li", k);
      li.append(h("span", "", STEP[k] || ""), h("b", "", label));
      ol.append(li);
      return li;
    });
    replayBox.append(head, ol);
    replayBox.hidden = false;
    replay = { id, i: -1, timer: 0 };
    const show = (i) => items.forEach((li, j) => {
      li.classList.toggle("done", j < i);
      li.classList.toggle("now", j === i);
      li.classList.toggle("todo", j > i);
    });
    const payLabel = (s) => s[1] + (where.pay?.src ? " · " + where.pay.src : "");
    const advance = () => {
      replay.i++;
      show(replay.i);
      const step = steps[replay.i];
      items[replay.i]?.scrollIntoView({ block: "nearest" });
      if (step?.[0] === "pay" && where.cc) burst(where.cc, payLabel(step));
      if (replay.i >= steps.length - 1) {
        clearInterval(replay.timer);
        replay.timer = 0;
      }
    };
    if (reduce.matches) {
      show(steps.length);
      const last = steps.at(-1);
      if (last?.[0] === "pay" && where.cc) burst(where.cc, payLabel(last));
      return;
    }
    show(-1);
    replay.timer = setInterval(advance, 1000);
    setTimeout(advance, 600);
  };
  const stopReplay = () => {
    if (replay) clearInterval(replay.timer);
    replay = null;
    replayBox.hidden = true;
  };

  const top = (items, n) => {
    const m = new Map();
    for (const x of items) if (x) m.set(x, (m.get(x) || 0) + 1);
    return [...m].sort((a, b) => b[1] - a[1]).slice(0, n);
  };
  const block = (title, rows) => {
    const sec = h("section", "gl-blk");
    sec.append(h("h3", "", title));
    if (!rows.length) sec.append(h("p", "dim", "Nothing yet"));
    for (const [name, val] of rows) {
      const r = h("div", "gl-kv");
      r.append(h("span", "", name), h("b", "", String(val)));
      sec.append(r);
    }
    return sec;
  };

  const lists = () => {
    const live = root.querySelector("#gl-live");
    live.textContent = "";
    root.querySelector("#gl-count").textContent = data.online ? `${data.online} on the site now` : "Nobody on the site right now";
    root.querySelector(".gl-sum").textContent = `${data.visitors.length} visitors shown on the globe`;
    idleNote.hidden = data.visitors.length > 0;
    if (!data.visitors.length) live.append(h("p", "gl-empty dim", "Visitors appear here the second they land on your site."));
    for (const v of data.visitors.slice(0, 12)) {
      const row = h("button", "gl-row" + (v.id === selected ? " on" : ""));
      row.type = "button";
      const av = h("span", "av");
      av.innerHTML = data.faces[v.face];
      const who = h("span", "gl-who");
      who.append(h("b", "", v.name), h("span", "dim", [v.place, v.page].filter(Boolean).join(" · ")));
      row.append(av, who);
      if (v.paid) row.append(h("span", "gl-tag", v.spent ? "Customer " + v.spent : "Customer"));
      row.addEventListener("click", () => pick(v.id === selected ? "" : v.id));
      live.append(row);
    }
    const places = root.querySelector("#gl-places");
    places.textContent = "";
    if (mode === "revenue") {
      const rows = data.countries.filter((c) => c.rpv > 0).sort((a, b) => b.rpv - a.rpv).slice(0, 4).map((c) => [c.name, usd.format(c.rpv)]);
      places.append(block("Revenue per visitor", rows));
    } else {
      places.append(block("Countries", top(data.visitors.map((v) => v.place), 4)));
    }
    places.append(block("Sources", top(data.visitors.map((v) => v.src), 4)), block("Devices", top(data.visitors.map((v) => v.dev), 3)));

    const feed = root.querySelector("#gl-pays");
    feed.textContent = "";
    if (!data.payments.length) feed.append(h("p", "gl-empty dim", "Payments show up here as they land."));
    for (const p of data.payments.slice(0, 8)) {
      const row = h("button", "gl-row gl-pay");
      row.type = "button";
      const av = h("span", "av");
      av.innerHTML = data.faces[p.face];
      const who = h("span", "gl-who");
      who.append(h("b", "", p.name), h("span", "dim", [p.src, p.place, p.ago].filter(Boolean).join(" · ")));
      row.append(av, who, h("span", "gl-amt", p.amt));
      row.addEventListener("click", () => {
        if (p.vid && data.journeys[p.vid]) {
          selected = p.vid;
          startReplay(p.vid);
          lists();
          gl?.markers();
        } else {
          const pt = outline?.pts[p.cc];
          if (pt) goTo(pt[0], pt[1]);
          burst(p.cc, p.amt + (p.src ? " · " + p.src : ""));
        }
      });
      feed.append(row);
    }
  };

  const apply = (next, first) => {
    data = next;
    place();
    const fresh = data.payments.filter((p) => !seenPays.has(p.id));
    data.payments.forEach((p) => seenPays.add(p.id));
    const show = first ? fresh.slice(0, 2) : fresh.slice(0, 4);
    show.forEach((p, i) => burst(p.cc, p.amt + (p.src ? " · " + p.src : ""), i * 500));
    const quiet = !drag && !selected && !replay && performance.now() - lastInput > 4000;
    if (show[0] && (first || quiet)) {
      const pt = outline?.pts[show[0].cc];
      if (pt) goTo(pt[0], pt[1]);
    }
    legend();
    lists();
    gl?.paint();
    gl?.markers();
  };

  let fetching = false;
  document.addEventListener("live", (e) => {
    e.preventDefault();
    if (fetching) return;
    fetching = true;
    fetch(location.pathname.replace(/\/$/, "") + "/data" + location.search, { headers: { accept: "application/json" } })
      .then((r) => (r.ok ? r.json() : Promise.reject()))
      .then((d) => apply(d, false))
      .catch(() => {})
      .finally(() => { fetching = false; });
  });

  const recolor = () => {
    readColors();
    gl?.paint();
  };
  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", recolor);
  new MutationObserver(recolor).observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
  reduce.addEventListener("change", wake);

  readColors();
  canvas.style.cursor = "grab";
  lists();
  legend();
  Promise.all([fetch(root.dataset.outline).then((r) => r.json()), init3d()])
    .then(([o]) => {
      prepare(o);
      apply(data, true);
    })
    .catch(flat);
})();
