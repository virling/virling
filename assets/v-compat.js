/*! Virling compatibility alias, AGPL-3.0. Load after v.js. */
((w, d, L) => {
  var v = w.virling, vid = v?.vid?.(), q = w.datafast?.q || [];
  if (!v?.vid) return;
  // Compatibility alias: goals sent through the other global, including calls queued before load.
  w.datafast = (g) => g && v("goal", "" + g);
  q.forEach((a) => w.datafast(a[0]));
  d.addEventListener("click", (e) => w.datafast(e.target.closest?.("[data-fast-goal]")?.getAttribute("data-fast-goal")), true);
  // Compatibility alias: the visitor ID under the other cookie name, so checkout code that reads it keeps working.
  if (!vid) return;
  var c = "datafast_visitor_id=" + vid, a = ";path=/;SameSite=Lax;max-age=31536000" + (L.protocol == "https:" ? ";Secure" : "");
  for (var p = L.hostname.split("."), i = p.length - 1; i-- > 0 && (d.cookie = c + a + ";domain=" + p.slice(i).join("."), !d.cookie.includes(c)););
  i < 0 && (d.cookie = c + a);
})(window, document, location);
