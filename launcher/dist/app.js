// Anti-Horizon Launcher: the window. Rust does the heavy work (downloads, Java, launching, sign-in); this draws and asks.
"use strict";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// The system this copy runs on, for its own words (Recycle Bin or Trash) and its own window buttons on a Mac.
const OS = /Mac/.test(navigator.userAgent) ? "mac" : /Linux|X11/.test(navigator.userAgent) ? "linux" : "windows";
const BIN = OS === "windows" ? "Recycle Bin" : "Trash";
const FILES = OS === "mac" ? "Finder" : OS === "windows" ? "Explorer" : "the file manager";
document.documentElement.classList.add(`os-${OS}`);

const $ = (sel, root = document) => root.querySelector(sel);
const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];

// Elements are built from text nodes, never innerHTML, so names and descriptions from mod sites can't inject
// markup into a window that can call the launcher's commands.
function h(tag, props, ...kids) {
  const el = document.createElement(tag);
  for (const [key, value] of Object.entries(props || {})) {
    if (value == null || value === false) continue;
    if (key === "class") el.className = value;
    else if (key.startsWith("on")) el.addEventListener(key.slice(2), value);
    else el.setAttribute(key, value === true ? "" : value);
  }
  for (const kid of kids.flat()) {
    if (kid != null && kid !== false) el.append(kid instanceof Node ? kid : String(kid));
  }
  return el;
}

function icon(name, extra) {
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("class", extra ? `i ${extra}` : "i");
  svg.setAttribute("aria-hidden", "true");
  const use = document.createElementNS(ns, "use");
  use.setAttribute("href", `#i-${name}`);
  svg.append(use);
  return svg;
}

const LOADER = { vanilla: "Vanilla", fabric: "Fabric", quilt: "Quilt", forge: "Forge", neoforge: "NeoForge" };
const KIND = { mod: "mods", resourcepack: "resource packs", shader: "shader packs", modpack: "modpacks" };
const FOLDER = { mod: "mods", resourcepack: "resourcepacks", shader: "shaderpacks", modpack: "" };
const SOURCE = { modrinth: "Modrinth", curseforge: "CurseForge" };
const compact = new Intl.NumberFormat("en", { notation: "compact", maximumFractionDigits: 1 });
const relative = new Intl.RelativeTimeFormat("en", { numeric: "auto" });

const state = {
  // What this build of the launcher can do (CurseForge needs the launcher's own key, built in at release).
  features: { curseforge: false },
  view: "play",
  instances: [],
  // The account menu's picture of the selected skin's face (a data: URL), once loaded.
  face: "",
  selected: remember("selected") || "",
  versions: null,
  accounts: [],
  settings: null,
  ram: 0,
  version: "",
  busy: new Map(),
  mods: {
    instance: "", source: "modrinth", kind: "mod", tab: "browse", query: "", hits: [], total: 0, loading: false, error: "", installed: [], installing: new Set(),
    // Installed: which files have a newer version (file -> version), for which instance, and what is being updated.
    filter: "all", updates: new Map(), updatesFor: "", checking: false, checkError: "", updating: new Set(),
    // Browse: how results are ordered and which Modrinth category they're from.
    sort: "", category: "",
  },
};

function remember(key, value) {
  try {
    if (value === undefined) return localStorage.getItem(`ah.${key}`);
    localStorage.setItem(`ah.${key}`, value);
  } catch {
    // Storage can be unavailable; it only remembers which instance was selected.
  }
  return null;
}

// ---------------------------------------------------------------- helpers

function ago(seconds) {
  if (!seconds) return "Never played";
  let d = Math.round(seconds - Date.now() / 1000);
  if (Math.abs(d) < 60) return "Played just now";
  d = Math.round(d / 60);
  for (const [n, unit] of [[60, "minute"], [24, "hour"], [7, "day"], [4.35, "week"], [12, "month"]]) {
    if (Math.abs(d) < n) return `Played ${relative.format(d, unit)}`;
    d = Math.round(d / n);
  }
  return `Played ${relative.format(d, "year")}`;
}

function playtime(seconds) {
  if (seconds < 3600) return `${Math.max(1, Math.round(seconds / 60))} min played`;
  const hours = seconds / 3600;
  return `${hours < 10 ? hours.toFixed(1) : Math.round(hours)} h played`;
}

const versionLine = (inst) =>
  inst.loader === "vanilla" ? `Minecraft ${inst.version}` : `Minecraft ${inst.version} · ${LOADER[inst.loader]} ${inst.loader_version}`;
const day = (iso) => new Date(iso).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });

function pressed(group, button) {
  for (const b of $$(`${group} button`)) b.setAttribute("aria-pressed", String(b === button));
}

function toast(message, tone, action) {
  const el = h("div", { class: tone ? `toast ${tone}` : "toast", role: tone === "bad" ? "alert" : "status" }, h("span", {}, message));
  if (action) el.append(h("button", { class: "btn-text", onclick: () => { el.remove(); action.run(); } }, action.label));
  $("#toasts").append(el);
  const life = tone === "bad" ? 7000 : 4200;
  setTimeout(() => el.classList.add("out"), life);
  setTimeout(() => el.remove(), life + 400);
}

function fail(err) {
  const message = String(err);
  if (/Settings/.test(message)) toast(message, "bad", { label: "Settings", run: () => show("settings") });
  else if (/account/i.test(message)) toast(message, "bad", { label: "Accounts", run: () => openAcctMenu() });
  else toast(message, "bad");
}

function ask(title, body, yes) {
  const dialog = $("#dlg-confirm");
  $("#confirm-title").textContent = title;
  $("#confirm-body").textContent = body;
  $("#confirm-yes").textContent = yes;
  dialog.returnValue = "";
  dialog.showModal();
  return new Promise((resolve) => dialog.addEventListener("close", () => resolve(dialog.returnValue === "yes"), { once: true }));
}

function empty(text, label, run) {
  return h("div", { class: "empty" }, h("p", {}, text), label ? h("button", { class: "btn btn-line", onclick: run }, label) : null);
}

function thumb(src, name, size) {
  const cls = size ? `thumb ${size}` : "thumb";
  const letter = h("div", { class: `${cls} thumb-empty`, "aria-hidden": "true" }, (name || "?").trim().charAt(0).toUpperCase() || "?");
  if (!src || !(src.startsWith("https://") || src.startsWith("data:image/png;base64,"))) return letter;
  return h("img", { class: cls, src, alt: "", loading: "lazy", decoding: "async", referrerpolicy: "no-referrer", onerror: (ev) => ev.target.replaceWith(letter) });
}

const openFolder = (id, sub) => invoke("open_instance_folder", { id, sub }).catch(fail);

// The game's black holes. By default each section has its own; Settings can put one everywhere or shuffle them
// each time the launcher opens. Two layers take turns, so a change is a crossfade.
const BGS = [
  // Live: the Anti-Horizon game's title-screen sights (downloaded the first time they're used; a still until then).
  { id: "clip-galactic-core", name: "Galactic-core black hole", clip: "galactic-core" },
  { id: "clip-emission-nebula", name: "Emission nebula", clip: "emission-nebula" },
  { id: "clip-stellar-black-hole", name: "Stellar-origin black hole", clip: "stellar-black-hole" },
  { id: "clip-interstallar", name: "The Interstallar", clip: "interstallar" },
  { id: "clip-molecular-cloud", name: "Molecular cloud", clip: "molecular-cloud" },
  { id: "clip-massive-black-hole", name: "Massive black hole", clip: "massive-black-hole" },
  { id: "clip-singularity", name: "Singularity", clip: "singularity" },
  { id: "clip-galactic-nebula", name: "Galactic nebula", clip: "galactic-nebula" },
  { id: "clip-intermediate-black-hole", name: "Intermediate-mass black hole", clip: "intermediate-black-hole" },
  { id: "clip-ultramassive-black-hole", name: "Ultramassive black hole", clip: "ultramassive-black-hole" },
  { id: "home", name: "Event horizon" },
  { id: "mods", name: "Edge-on" },
  { id: "accounts", name: "Frost" },
  { id: "settings", name: "Interstellar" },
  { id: "torus", name: "Golden torus" },
  { id: "quasar", name: "Quasar crown" },
  { id: "core", name: "Galactic core" },
  { id: "ember", name: "Ember" },
  { id: "crimson", name: "Crimson" },
  { id: "emerald", name: "Emerald" },
  { id: "violet", name: "Violet" },
];
const PAGES = { play: "Play", mods: "Mods", skins: "Skins", cosmetics: "Cosmetics", settings: "Settings" };
const PER_PAGE = {
  play: "clip-interstallar", mods: "clip-galactic-core", skins: "clip-stellar-black-hole",
  cosmetics: "clip-ultramassive-black-hole", settings: "clip-massive-black-hole",
};
let shuffled = null;
let backdropLayer = 0;

function backdrops() {
  const mode = remember("bg-mode") || "pages";
  if (mode === "one") {
    const id = BGS.some((b) => b.id === remember("bg-pick")) ? remember("bg-pick") : "home";
    return { play: id, mods: id, skins: id, cosmetics: id, settings: id };
  }
  if (mode === "shuffle") {
    if (!shuffled) {
      const ids = BGS.map((b) => b.id);
      for (let i = ids.length - 1; i > 0; i--) {
        const j = Math.floor(Math.random() * (i + 1));
        [ids[i], ids[j]] = [ids[j], ids[i]];
      }
      shuffled = { play: ids[0], mods: ids[1], skins: ids[2], cosmetics: ids[3], settings: ids[4] };
    }
    return shuffled;
  }
  return PER_PAGE;
}

function renderBgPicker() {
  const mode = remember("bg-mode") || "pages";
  const used = backdrops();
  for (const b of $$("#bg-mode button")) b.setAttribute("aria-pressed", String(b.dataset.m === mode));
  $("#bg-note").textContent = {
    pages: "Each page has its own. Pick one to use it everywhere.",
    one: "The one you picked is on every page.",
    shuffle: "A different one on each page, mixed up each time the launcher opens.",
  }[mode];
  $("#bg-grid").replaceChildren(...BGS.map((b) => {
    const where = Object.keys(PAGES).filter((v) => used[v] === b.id);
    return h("button", {
      class: "bg-pick", type: "button", title: b.name, "aria-pressed": String(mode === "one" && where.length > 0),
      onclick: () => {
        remember("bg-mode", "one");
        remember("bg-pick", b.id);
        setBackground(state.view);
        renderBgPicker();
      },
    },
      h("img", { src: `img/thumb-${b.id}.jpg`, alt: "", loading: "lazy", decoding: "async" }),
      b.clip ? h("span", { class: "live-tag" }, "Live") : null,
      mode !== "one" && where.length ? h("span", { class: "where" }, ...where.map((v) => h("span", {}, PAGES[v]))) : null,
      b.name);
  }));
}

// A live clip's video, from the launcher (cached after the first download), as a URL a <video> can play.
const clipUrls = new Map();
function clipUrl(clip) {
  if (!clipUrls.has(clip)) {
    clipUrls.set(clip, invoke("clip", { id: clip })
      .then((bytes) => URL.createObjectURL(new Blob([bytes], { type: "video/mp4" })))
      .catch((err) => { clipUrls.delete(clip); throw err; }));
  }
  return clipUrls.get(clip);
}
const stillMotion = () => matchMedia("(prefers-reduced-motion: reduce)").matches;
const bgStill = (id) => (BGS.find((b) => b.id === id)?.clip ? `img/${id}.jpg` : `img/bg-${id}.jpg`);

function setBackground(view) {
  const bg = $("#bg");
  const id = backdrops()[view] || "home";
  const live = BGS.find((b) => b.id === id)?.clip;
  bg.classList.toggle("home", view === "play");
  bg.classList.toggle("dim", view !== "play");
  const layers = $$(".bg-img", bg);
  if (layers[backdropLayer].dataset.bg === id) return;
  backdropLayer ^= 1;
  const layer = layers[backdropLayer], old = layers[backdropLayer ^ 1];
  layer.dataset.bg = id;
  layer.classList.toggle("live", !!live);
  layer.style.backgroundImage = `url("${bgStill(id)}")`;
  layer.querySelector("video")?.remove();
  if (live && !stillMotion()) {
    // The still shows at once; the clip fades in over it once it's ready.
    const video = document.createElement("video");
    Object.assign(video, { muted: true, loop: true, playsInline: true, autoplay: true });
    video.setAttribute("aria-hidden", "true");
    video.addEventListener("playing", () => video.classList.add("on"), { once: true });
    layer.append(video);
    clipUrl(live).then((src) => {
      if (layer.dataset.bg !== id || !video.isConnected) return;
      video.src = src;
      video.play().catch(() => {});
    }).catch(() => {});
  }
  layer.classList.add("on");
  old.classList.remove("on");
  // The old one stops once it has faded out.
  setTimeout(() => { if (!old.classList.contains("on")) old.querySelector("video")?.pause(); }, 1000);
}

// Nothing plays while the launcher can't be seen.
document.addEventListener("visibilitychange", () => {
  for (const v of $$("#bg video")) {
    if (document.hidden) v.pause();
    else if (v.closest(".bg-img.on")) v.play().catch(() => {});
  }
});

// ---------------------------------------------------------------- pages

// The glass lens under the current section, measured from its button so it fits any label. Moving it squashes it
// like liquid and sends a sheen across the bar; placing it without a change (start-up, resizing) does neither.
function moveLens(animate) {
  const nav = $(".tabs-nav");
  const lens = $(".tab-lens", nav);
  const active = $('.nav-item[aria-current="page"]', nav);
  if (!active) return;
  if (!animate) lens.style.transition = "none";
  lens.style.setProperty("--x", `${active.offsetLeft}px`);
  lens.style.setProperty("--w", `${active.offsetWidth}px`);
  if (!animate) {
    void lens.offsetWidth;
    lens.style.transition = "";
    return;
  }
  for (const [el, cls] of [[lens, "moving"], [nav, "sheen"]]) {
    el.classList.remove(cls);
    void el.offsetWidth;
    el.classList.add(cls);
  }
}

function show(view) {
  const changed = state.view !== view;
  state.view = view;
  setBackground(view);
  for (const b of $$(".nav-item")) {
    if (b.dataset.view === view) b.setAttribute("aria-current", "page");
    else b.removeAttribute("aria-current");
  }
  if (changed) moveLens(true);
  for (const v of $$(".view")) v.hidden = v.id !== `view-${view}`;
  $("main").scrollTop = 0;
  if (view === "play") renderPlay();
  else if (view === "mods") enterMods();
  else if (view === "skins") enterSkins();
  else if (view === "cosmetics") enterCosmetics();
  else if (view === "settings") renderSettings();
}

// ---------------------------------------------------------------- play

const current = () => state.instances.find((i) => i.id === state.selected) || state.instances[0];

async function refresh() {
  try {
    state.instances = await invoke("list_instances");
  } catch (err) {
    fail(err);
  }
  if (state.view === "play") renderPlay();
  status();
}

function select(id) {
  state.selected = id;
  remember("selected", id);
  renderPlay();
}

function renderPlay() {
  const inst = current();
  const hero = $("#hero");
  if (!inst) {
    hero.replaceChildren(
      h("p", { class: "eyebrow" }, "Minecraft: Java Edition"),
      h("h1", { class: "title" }, "No instances yet"),
      h("p", { class: "lede" }, "An instance is one Minecraft setup: a version, a mod loader if you want one, and its own worlds and mods."),
      h("div", { class: "hero-actions" }, h("button", { class: "btn btn-gold btn-xl", onclick: openNew }, icon("plus"), "New instance")),
    );
  } else {
    const busy = state.busy.get(inst.id);
    const main = inst.running
      ? [h("button", { class: "btn btn-xl btn-running", disabled: true }, h("span", { class: "dot" }), "Playing"),
         h("button", { class: "btn btn-line", onclick: () => forceClose(inst) }, icon("close"), "Force close")]
      : h("button", { class: "btn btn-gold btn-xl", disabled: !!busy, onclick: () => play(inst.id) }, icon("play"), busy ? "Getting ready" : "Play");
    hero.replaceChildren(
      h("p", { class: "eyebrow" }, versionLine(inst)),
      h("h1", { class: "title" }, inst.name),
      h("p", { class: "lede" }, inst.dev ? "Test mode until Microsoft sign-in is approved. Your mods and settings carry over."
        : inst.play_seconds ? `${ago(inst.last_played)} · ${playtime(inst.play_seconds)}` : ago(inst.last_played)),
      h("div", { class: "hero-actions" },
        main,
        h("button", { class: "btn btn-line", onclick: () => openInstance(inst.id) }, icon("cube"), "Content"),
        h("button", { class: "btn btn-line", onclick: () => openConsole(inst.id) }, icon("terminal"), "Console"),
        h("button", { class: "btn btn-line btn-icon", title: "Open folder", "aria-label": `Open ${inst.name}'s folder`, onclick: () => openFolder(inst.id, "") }, icon("folder"))),
    );
  }
  renderGrid();
}

function renderGrid() {
  const selected = current();
  const stop = (fn) => (ev) => {
    ev.stopPropagation();
    fn();
  };
  const cards = state.instances.map((inst) => {
    const busy = state.busy.get(inst.id);
    return h("article", {
        class: inst === selected ? "card is-selected" : "card",
        onclick: () => { select(inst.id); openInstance(inst.id); },
      },
      h("div", { class: "card-art", style: `background-image:url("img/${inst.loader}-sm.jpg")`, "aria-hidden": "true" }),
      inst.running || busy ? null : h("button", { class: "card-play", title: "Play", "aria-label": `Play ${inst.name}`, onclick: stop(() => play(inst.id)) }, icon("play")),
      h("div", { class: "card-top" },
        inst.dev ? h("span", { class: "tag tag-dev" }, "Test mode")
          : h("span", { class: inst.loader === "vanilla" ? "tag tag-mint" : "tag" }, LOADER[inst.loader] || inst.loader),
        inst.running ? h("span", { class: "live" }, h("span", { class: "dot" }), "Playing")
          : busy ? h("span", { class: "live live-gold" }, h("span", { class: "dot" }), "Getting ready") : null),
      h("div", { class: "card-body" },
        h("h3", { class: "card-name" }, h("button", { class: "linkish", onclick: stop(() => { select(inst.id); openInstance(inst.id); }) }, inst.name)),
        h("p", { class: "card-meta" }, inst.loader === "vanilla" ? inst.version : `${inst.version} · ${inst.loader_version}`),
        h("p", { class: "card-foot" }, inst.play_seconds ? `${ago(inst.last_played)} · ${playtime(inst.play_seconds)}` : ago(inst.last_played))),
      h("div", { class: "card-tools" },
        h("button", { class: "tool", title: "Console", "aria-label": `${inst.name}'s console`, onclick: stop(() => openConsole(inst.id)) }, icon("terminal")),
        h("button", { class: "tool", title: "Open folder", "aria-label": `Open ${inst.name}'s folder`, onclick: stop(() => openFolder(inst.id, "")) }, icon("folder")),
        h("button", { class: "tool tool-bad", title: "Delete", "aria-label": `Delete ${inst.name}`, onclick: stop(() => removeInstance(inst)) }, icon("trash"))),
    );
  });
  $("#instances").replaceChildren(...cards, h("button", { class: "card card-new", onclick: openNew }, icon("plus"), "New instance"));
  $("#count").textContent = state.instances.length ? String(state.instances.length) : "";
}

async function play(id) {
  const inst = state.instances.find((i) => i.id === id);
  if (!inst || inst.running || state.busy.has(id)) return;
  state.busy.set(id, { id, stage: "Getting ready", done: 0, total: 0 });
  renderPlay();
  status();
  if (remember("console") !== "off") openConsole(id, true);
  try {
    await invoke("launch_instance", { id });
  } catch (err) {
    state.busy.delete(id);
    fail(err);
  }
  await refresh();
}

async function removeInstance(inst) {
  const yes = await ask(`Delete ${inst.name}?`, `Its worlds, mods and settings go to the ${BIN}. Game files shared with your other instances stay.`, "Delete");
  if (!yes) return;
  try {
    await invoke("delete_instance", { id: inst.id });
    toast(`${inst.name} is in the ${BIN}.`);
  } catch (err) {
    fail(err);
  }
  await refresh();
}

async function forceClose(inst) {
  const yes = await ask(`Force close ${inst.name}?`, "Minecraft stops straight away, so anything since its last autosave is lost. Quitting from the game's own menu is safer.", "Force close");
  if (!yes) return;
  try {
    await invoke("stop_instance", { id: inst.id });
  } catch (err) {
    fail(err);
  }
}

function status() {
  const el = $("#status");
  const bar = $("#bar");
  const jobs = [...state.busy.values()];
  const p = jobs[jobs.length - 1];
  const playing = state.instances.filter((i) => i.running);
  const right = $("#status-right");
  right.textContent = playing.length ? `Playing ${playing.map((i) => i.name).join(", ")}` : state.version ? `v${state.version}` : "";
  right.classList.toggle("is-playing", playing.length > 0);
  if (!p) {
    el.classList.remove("is-busy", "is-indeterminate");
    bar.style.transform = "scaleX(0)";
    $("#status-text").textContent = "Ready";
    return;
  }
  const inst = state.instances.find((i) => i.id === p.id);
  const count = p.total ? `   ${p.done.toLocaleString()} / ${p.total.toLocaleString()}` : "";
  $("#status-text").textContent = `${inst ? `${inst.name}: ` : ""}${p.stage}${count}`;
  el.classList.add("is-busy");
  el.classList.toggle("is-indeterminate", !p.total);
  bar.style.transform = `scaleX(${p.total ? p.done / p.total : 0})`;
}

// ---------------------------------------------------------------- new instance

const draft = { filter: "release", loader: "vanilla", version: "", query: "", visible: [] };
let loaderTimer;
let loaderTicket = 0;

async function openNew() {
  $("#new-name").value = "";
  $("#new-search").value = "";
  $("#new-error").textContent = "";
  draft.query = "";
  setLoader("vanilla");
  $("#dlg-new").showModal();
  if (!state.versions) {
    $("#new-versions").replaceChildren(h("p", { class: "empty" }, "Loading Mojang's version list"));
    try {
      state.versions = await invoke("get_versions");
    } catch (err) {
      $("#new-versions").replaceChildren(h("p", { class: "empty" }, String(err)));
      return;
    }
  }
  if (!draft.version) draft.version = state.versions.latest_release;
  $("#new-picked").textContent = draft.version;
  renderVersions();
}

function renderVersions() {
  if (!state.versions) return;
  const q = draft.query.trim().toLowerCase();
  const { latest_release: release, latest_snapshot: snapshot } = state.versions;
  const list = state.versions.versions.filter(
    (v) => (draft.filter === "old" ? v.type.startsWith("old_") : v.type === draft.filter) && (!q || v.id.toLowerCase().includes(q)),
  );
  draft.visible = list.map((v) => v.id);
  const box = $("#new-versions");
  box.replaceChildren(...list.map((v) => h("div", { class: "vrow", role: "option", "aria-selected": String(v.id === draft.version), "data-id": v.id, onclick: () => pickVersion(v.id) },
    h("span", { class: "vid" }, v.id),
    v.id === release || v.id === snapshot ? h("span", { class: "tag" }, "Latest") : null,
    h("span", { class: "vdate" }, day(v.releaseTime)))));
  if (!list.length) box.append(h("p", { class: "empty" }, "No versions match."));
  box.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
}

function pickVersion(id, scroll) {
  draft.version = id;
  $("#new-picked").textContent = id;
  for (const row of $$("#new-versions .vrow")) row.setAttribute("aria-selected", String(row.dataset.id === id));
  if (scroll) $('#new-versions [aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
  clearTimeout(loaderTimer);
  loaderTimer = setTimeout(loadLoaderVersions, 250);
}

function setLoader(loader) {
  draft.loader = loader;
  for (const b of $$("#new-loader button")) b.setAttribute("aria-pressed", String(b.dataset.l === loader));
  $("#new-lv-field").hidden = loader === "vanilla";
  loadLoaderVersions();
}

async function loadLoaderVersions() {
  if (draft.loader === "vanilla" || !draft.version) return;
  const select = $("#new-lv");
  const ticket = ++loaderTicket;
  select.disabled = true;
  select.replaceChildren(h("option", { value: "" }, `Finding ${LOADER[draft.loader]} builds`));
  try {
    const list = await invoke("get_loader_versions", { loader: draft.loader, mcVersion: draft.version });
    if (ticket !== loaderTicket) return;
    select.replaceChildren(...(list.length
      ? list.map((v, i) => h("option", { value: v }, i ? v : `${v} (newest)`))
      : [h("option", { value: "" }, `No ${LOADER[draft.loader]} for ${draft.version} yet`)]));
    select.disabled = !list.length;
  } catch (err) {
    if (ticket === loaderTicket) select.replaceChildren(h("option", { value: "" }, String(err)));
  }
}

async function createInstance() {
  const error = $("#new-error");
  error.textContent = "";
  if (!draft.version) return (error.textContent = "Pick a Minecraft version first.");
  const loaderVersion = draft.loader === "vanilla" ? "" : $("#new-lv").value;
  if (draft.loader !== "vanilla" && !loaderVersion) return (error.textContent = `Pick a ${LOADER[draft.loader]} version first.`);
  try {
    const inst = await invoke("create_instance", { name: $("#new-name").value, version: draft.version, loader: draft.loader, loaderVersion });
    $("#dlg-new").close();
    state.selected = inst.id;
    remember("selected", inst.id);
    await refresh();
    show("play");
    toast(`${inst.name} is ready. Press Play and it downloads what it needs.`, "good");
  } catch (err) {
    error.textContent = String(err);
  }
}

// ---------------------------------------------------------------- mods

const modsInstance = () => state.instances.find((i) => i.id === state.mods.instance);
let searchTicket = 0;

async function enterMods() {
  const m = state.mods;
  if (!modsInstance()) m.instance = current()?.id || "";
  const label = (i) => (i.name.includes(i.version) ? i.name : `${i.name} (${LOADER[i.loader]} ${i.version})`);
  $("#mods-instance").replaceChildren(...state.instances.map((i) => h("option", { value: i.id, selected: i.id === m.instance }, label(i))));
  $("#mods-instance").disabled = !state.instances.length;
  await loadInstalled();
  drawMods(true);
}

// Modrinth's own category slugs, the useful ones for each kind of content.
const CATEGORIES = {
  mod: ["optimization", "utility", "adventure", "technology", "magic", "decoration", "storage", "equipment", "mobs", "worldgen", "food", "game-mechanics", "transportation", "social", "library"],
  resourcepack: ["vanilla-like", "realistic", "simplistic", "themed", "combat", "gui", "tweaks", "audio", "fonts", "models"],
  shader: ["vanilla-like", "semi-realistic", "realistic", "fantasy", "cartoon", "potato", "screenshot"],
  modpack: ["optimization", "adventure", "technology", "magic", "multiplayer", "quests", "lightweight", "kitchen-sink", "challenging", "combat"],
};
const LABELS = { gui: "GUI", pbr: "PBR", "vanilla-like": "Vanilla-like", "semi-realistic": "Semi-realistic", "colored-lighting": "Coloured lighting" };
const label = (slug) => LABELS[slug] || slug.charAt(0).toUpperCase() + slug.slice(1).replaceAll("-", " ");

function since(iso) {
  const t = Date.parse(iso);
  if (!t) return "";
  let d = Math.round((t - Date.now()) / 1000 / 60);
  if (Math.abs(d) < 60) return "Updated just now";
  for (const [n, unit] of [[60, "minute"], [24, "hour"], [7, "day"], [4.35, "week"], [12, "month"]]) {
    if (Math.abs(d) < n) return `Updated ${relative.format(d, unit)}`;
    d = Math.round(d / n);
  }
  return `Updated ${relative.format(d, "year")}`;
}

function renderCats() {
  const m = state.mods;
  const box = $("#mods-cats");
  const show = m.tab === "browse" && m.source === "modrinth";
  box.hidden = !show;
  if (!show) return;
  const chip = (slug, text) => h("button", { class: "chip", "aria-pressed": String(m.category === slug), onclick: () => {
    m.category = slug;
    renderCats();
    search(true);
  } }, text);
  box.replaceChildren(chip("", "All"), ...(CATEGORIES[m.kind] || []).map((c) => chip(c, label(c))));
}

function drawMods(fresh) {
  const m = state.mods;
  $("#mods-folder").disabled = m.kind === "modpack";
  $("#view-mods .inline").hidden = m.kind === "modpack";
  $("#mods-q").placeholder = m.tab === "installed" ? `Search what's installed` : `Search ${SOURCE[m.source]}`;
  $("#mods-source").hidden = m.tab === "installed" || !state.features.curseforge;
  $("#browse-bar").hidden = m.tab === "installed";
  renderCats();
  if (m.tab === "installed") {
    renderInstalled();
    if (m.updatesFor !== m.instance && m.kind !== "modpack") checkModUpdates(false);
  } else {
    $("#inst-bar").hidden = true;
    if (fresh) search(true);
    else renderResults();
  }
}

function blocked() {
  const m = state.mods;
  const inst = modsInstance();
  if (m.source === "curseforge" && !state.features.curseforge) {
    return empty("CurseForge isn't available in this version yet. Modrinth has most of the same mods.", "Use Modrinth", () => document.querySelector("[data-s=modrinth]").click());
  }
  // A modpack brings its own version and loader, so it needs no instance first.
  if (m.kind === "modpack") return null;
  if (!inst) return empty("Make an instance first, then add mods to it.", "New instance", openNew);
  if (m.kind === "mod" && inst.loader === "vanilla") {
    return empty(`${inst.name} has no mod loader. Mods need a Fabric, Quilt, Forge or NeoForge instance.`, "New instance", openNew);
  }
  return null;
}

async function search(reset) {
  const m = state.mods;
  if (m.tab !== "browse") return;
  if (reset) {
    m.hits = [];
    m.total = 0;
    m.error = "";
  }
  if (blocked()) return renderResults();
  const inst = modsInstance();
  const pack = m.kind === "modpack";
  const ticket = ++searchTicket;
  m.loading = true;
  renderResults();
  try {
    const page = await invoke("search_mods", {
      source: m.source, query: m.query, kind: m.kind,
      mcVersion: pack ? "" : inst.version, loader: pack ? "" : inst.loader, offset: m.hits.length,
      sort: m.sort, category: m.source === "modrinth" ? m.category : "",
    });
    if (ticket !== searchTicket) return;
    m.hits = m.hits.concat(page.hits);
    m.total = page.total;
    m.error = "";
  } catch (err) {
    if (ticket !== searchTicket) return;
    m.error = String(err);
  }
  m.loading = false;
  renderResults();
}

function renderResults() {
  const m = state.mods;
  const box = $("#mods-results");
  const more = $("#mods-more");
  more.hidden = true;
  const stop = blocked();
  if (stop) return box.replaceChildren(stop);
  if (m.error) return box.replaceChildren(empty(m.error));
  if (!m.hits.length) {
    return box.replaceChildren(...(m.loading ? Array.from({ length: 6 }, () => h("div", { class: "skeleton" })) : [empty("Nothing found. Try another search.")]));
  }
  box.replaceChildren(...m.hits.map(hitRow));
  more.hidden = m.hits.length >= m.total;
  more.disabled = m.loading;
  more.textContent = m.loading ? "Loading" : "Load more";
}

function hitRow(hit) {
  const m = state.mods;
  const key = `${hit.source}:${hit.id}`;
  const have = m.kind !== "modpack" && m.installed.some((x) => x.source === hit.source && x.project === hit.id);
  const busy = m.installing.has(key);
  return h("article", { class: "hit is-link", onclick: (ev) => { if (!ev.target.closest("button")) openProject(hit); } },
    thumb(hit.icon, hit.title),
    h("div", { class: "hit-body" },
      h("h3", {},
        h("button", { class: "linkish", title: "More about it", onclick: () => openProject(hit) }, hit.title),
        hit.author ? h("span", { class: "by" }, `  by ${hit.author}`) : null),
      h("p", {}, hit.description),
      (hit.categories?.length || hit.updated)
        ? h("div", { class: "hit-meta" },
            hit.categories?.length ? h("span", { class: "cat" }, hit.categories.map(label).join("  ·  ")) : null,
            hit.updated ? h("span", {}, since(hit.updated)) : null)
        : null),
    h("div", { class: "hit-side" },
      h("span", { class: "dl", title: `${hit.downloads.toLocaleString()} downloads` }, icon("download", "i-sm"), compact.format(hit.downloads)),
      have ? h("span", { class: "ok" }, icon("check", "i-sm"), "Installed")
        : h("button", { class: "btn btn-line btn-sm", disabled: busy, onclick: () => (m.kind === "modpack" ? installPack(hit) : installHit(hit)) }, busy ? "Installing" : "Install")));
}

// ---------------------------------------------------------------- a project's page

// A mod's description is Markdown with HTML mixed in (Modrinth) or HTML (CurseForge). It is turned into HTML text,
// parsed into an inert document, and copied into the page one element at a time, keeping only harmless tags,
// https links (opened in the browser) and https images. Nothing from a description can run.
const KEEP_TAGS = new Set(["p", "br", "hr", "h1", "h2", "h3", "h4", "h5", "h6", "ul", "ol", "li", "strong", "b", "em", "i", "u", "s",
  "del", "code", "pre", "blockquote", "a", "img", "table", "thead", "tbody", "tr", "th", "td", "details", "summary", "div", "span",
  "center", "sup", "sub", "kbd", "mark", "figure", "figcaption"]);
const DROP_TAGS = new Set(["script", "style", "iframe", "object", "embed", "form", "input", "button", "textarea", "select", "link",
  "meta", "svg", "math", "template", "noscript", "frame", "frameset", "audio", "video", "source", "canvas", "base"]);

function safeHtml(html) {
  const doc = new DOMParser().parseFromString(`<body>${html}</body>`, "text/html");
  const out = document.createDocumentFragment();
  const copy = (from, into) => {
    for (const n of from.childNodes) {
      if (n.nodeType === Node.TEXT_NODE) {
        into.append(n.textContent);
        continue;
      }
      if (n.nodeType !== Node.ELEMENT_NODE) continue;
      const tag = n.tagName.toLowerCase();
      if (DROP_TAGS.has(tag)) continue;
      if (!KEEP_TAGS.has(tag)) {
        copy(n, into); // an unknown wrapper: keep what's inside it
        continue;
      }
      const el = document.createElement(tag === "center" ? "div" : tag);
      if (tag === "center" || (n.getAttribute("align") || "").toLowerCase() === "center") el.className = "md-center";
      if (tag === "a") {
        const href = (n.getAttribute("href") || "").trim();
        if (/^https:\/\//i.test(href)) {
          el.dataset.url = href;
          el.title = href;
          el.tabIndex = 0;
        }
      }
      if (tag === "img") {
        const src = (n.getAttribute("src") || "").trim();
        if (!/^https:\/\//i.test(src)) continue;
        el.src = src;
        el.alt = n.getAttribute("alt") || "";
        el.loading = "lazy";
        el.decoding = "async";
        el.referrerPolicy = "no-referrer";
        const w = n.getAttribute("width");
        if (w && /^\d{1,4}$/.test(w)) el.style.width = `${w}px`;
      }
      if ((tag === "td" || tag === "th") && /^\d{1,2}$/.test(n.getAttribute("colspan") || "")) el.colSpan = Number(n.getAttribute("colspan"));
      if (tag === "details" && n.hasAttribute("open")) el.open = true;
      into.append(el);
      copy(n, el);
    }
  };
  copy(doc.body, out);
  return out;
}

// Enough Markdown for mod pages: headings, lists, quotes, code, tables, rules, links, images, emphasis. HTML in the
// text is left for safeHtml() to clean.
function markdown(src) {
  const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  const inline = (s) => {
    const codes = [];
    s = s.replace(/`([^`]+)`/g, (_, c) => `\u0000${codes.push(esc(c)) - 1}\u0000`);
    s = s
      .replace(/!\[([^\]]*)\]\(\s*<?([^)\s>]+)>?(?:\s+"[^"]*")?\s*\)/g, (_, alt, url) => `<img alt="${esc(alt)}" src="${url}">`)
      .replace(/\[([^\]]+)\]\(\s*<?([^)\s>]+)>?(?:\s+"[^"]*")?\s*\)/g, '<a href="$2">$1</a>')
      .replace(/\*\*([^*]+)\*\*|__([^_]+)__/g, (_, a, b) => `<strong>${a ?? b}</strong>`)
      .replace(/(^|[^*\w])\*([^*\n]+)\*(?![\w*])/g, "$1<em>$2</em>")
      .replace(/(^|[\s(>])_([^_\n]+)_(?=$|[\s).,;:!?<])/g, "$1<em>$2</em>")
      .replace(/~~([^~]+)~~/g, "<del>$1</del>")
      .replace(/(^|[\s(])(https:\/\/[^\s<)"]*[^\s<)".,;:!?'])/g, '$1<a href="$2">$2</a>');
    return s.replace(/\u0000(\d+)\u0000/g, (_, i) => `<code>${codes[Number(i)]}</code>`);
  };
  const lines = src.replace(/\r\n?/g, "\n").split("\n");
  const listItem = /^\s*([-*+]|\d+[.)])\s+/;
  const blockStart = (l) => /^\s*(#{1,6}\s|```|>|<|\|)/.test(l) || listItem.test(l);
  let html = "";
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (!line.trim()) { i++; continue; }
    if (/^\s*```/.test(line)) {
      const code = [];
      for (i++; i < lines.length && !/^\s*```/.test(lines[i]); i++) code.push(lines[i]);
      i++;
      html += `<pre><code>${esc(code.join("\n"))}</code></pre>`;
      continue;
    }
    const heading = line.match(/^\s*(#{1,6})\s+(.*?)\s*#*\s*$/);
    if (heading) { html += `<h${heading[1].length}>${inline(heading[2])}</h${heading[1].length}>`; i++; continue; }
    if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) { html += "<hr>"; i++; continue; }
    if (/^\s*>/.test(line)) {
      const inner = [];
      for (; i < lines.length && /^\s*>/.test(lines[i]); i++) inner.push(lines[i].replace(/^\s*>\s?/, ""));
      html += `<blockquote>${markdown(inner.join("\n"))}</blockquote>`;
      continue;
    }
    if (listItem.test(line)) {
      const ordered = /^\s*\d/.test(line);
      const items = [];
      for (; i < lines.length && (listItem.test(lines[i]) || (/^\s{2,}\S/.test(lines[i]) && items.length)); i++) {
        if (listItem.test(lines[i])) items.push(lines[i].replace(listItem, ""));
        else items[items.length - 1] += ` ${lines[i].trim()}`;
      }
      html += `<${ordered ? "ol" : "ul"}>${items.map((x) => `<li>${inline(x)}</li>`).join("")}</${ordered ? "ol" : "ul"}>`;
      continue;
    }
    if (/^\s*\|/.test(line) && /^\s*\|?\s*:?-{2,}/.test(lines[i + 1] || "")) {
      const cells = (l) => l.trim().replace(/^\||\|$/g, "").split("|").map((c) => inline(c.trim()));
      const head = cells(line);
      const rows = [];
      for (i += 2; i < lines.length && /^\s*\|/.test(lines[i]); i++) rows.push(cells(lines[i]));
      html += `<table><thead><tr>${head.map((c) => `<th>${c}</th>`).join("")}</tr></thead><tbody>${rows.map((r) => `<tr>${r.map((c) => `<td>${c}</td>`).join("")}</tr>`).join("")}</tbody></table>`;
      continue;
    }
    if (/^\s*</.test(line)) {
      const block = [];
      for (; i < lines.length && lines[i].trim(); i++) block.push(lines[i]);
      html += inline(block.join("\n"));
      continue;
    }
    const para = [];
    for (; i < lines.length && lines[i].trim() && (!para.length || !blockStart(lines[i])); i++) para.push(lines[i]);
    html += `<p>${para.map((l, n) => inline(l.trim()) + (n < para.length - 1 ? (/\s{2,}$/.test(l) ? "<br>" : " ") : "")).join("")}</p>`;
  }
  return html;
}

const SIDE = { required: "Needed", optional: "Optional", unsupported: "Not needed" };

async function openProject(hit) {
  const dialog = $("#dlg-mod");
  const page = $("#mod-page");
  $("#mod-lightbox").hidden = true;
  page.replaceChildren(h("div", { class: "mod-loading" }, h("span", { class: "spinner", "aria-hidden": "true" }), " Loading"));
  if (!dialog.open) dialog.showModal();
  let d;
  try {
    d = await invoke("project_details", { source: hit.source, id: hit.id });
  } catch (err) {
    page.replaceChildren(h("div", { class: "mod-loading" }, empty(String(err), "Close", () => dialog.close())));
    return;
  }
  const m = state.mods;
  const key = `${hit.source}:${hit.id}`;
  const pack = m.kind === "modpack";
  const have = !pack && m.installed.some((x) => x.source === hit.source && x.project === hit.id);
  const busy = m.installing.has(key);
  const install = have ? h("span", { class: "ok" }, icon("check", "i-sm"), "Installed")
    : h("button", { class: "btn btn-gold", disabled: busy || !!blocked(), onclick: async (ev) => {
        ev.currentTarget.disabled = true;
        ev.currentTarget.textContent = "Installing";
        await (pack ? installPack(hit) : installHit(hit));
        if ($("#dlg-mod").open) openProject(hit);
      } }, icon("download"), busy ? "Installing" : pack ? "Install as instance" : `Install to ${modsInstance()?.name || "instance"}`);
  const stats = [
    h("span", { title: `${d.downloads.toLocaleString()} downloads` }, icon("download"), `${compact.format(d.downloads)} downloads`),
    d.follows ? h("span", {}, `${compact.format(d.follows)} ${d.source === "curseforge" ? "likes" : "followers"}`) : null,
    d.updated ? h("span", {}, since(d.updated)) : null,
    d.license ? h("span", {}, d.license) : null,
  ];
  const body = h("div", { class: "md" });
  body.append(safeHtml(d.markdown ? markdown(d.body) : d.body));
  const tags = [
    ...d.categories.map((c) => h("span", {}, label(c))),
    d.client && d.client !== "unknown" ? h("span", { class: "side" }, `Client: ${SIDE[d.client] || d.client}`) : null,
    d.server && d.server !== "unknown" ? h("span", { class: "side" }, `Server: ${SIDE[d.server] || d.server}`) : null,
  ].filter(Boolean);
  page.replaceChildren(
    h("header", { class: "mod-head" },
      thumb(d.icon || hit.icon, d.title),
      h("div", {},
        h("h2", { id: "mod-title" }, d.title),
        d.authors ? h("span", { class: "by" }, `by ${d.authors}`) : null,
        h("div", { class: "mod-stats" }, ...stats)),
      h("div", { class: "mod-actions" },
        install,
        d.url ? h("button", { class: "btn btn-line", "data-url": d.url, title: d.url }, SOURCE[d.source]) : null,
        h("button", { class: "tool", type: "button", "data-close": "", "aria-label": "Close" }, icon("close")))),
    h("div", { class: "mod-scroll" },
      h("p", { class: "mod-summary" }, d.summary),
      tags.length ? h("div", { class: "mod-tags" }, ...tags) : null,
      d.gallery.length ? h("div", { class: "gallery" }, ...d.gallery.map((g) =>
        h("button", { type: "button", title: g.title || "Screenshot", onclick: () => {
          const box = $("#mod-lightbox");
          $("img", box).src = g.full || g.url;
          box.hidden = false;
        } }, h("img", { src: g.url, alt: g.title || "", loading: "lazy", decoding: "async", referrerpolicy: "no-referrer" })))) : null,
      d.links.length ? h("div", { class: "mod-links" }, ...d.links.map((l) => h("button", { class: "btn btn-line btn-sm", "data-url": l.url, title: l.url }, l.label))) : null,
      body));
}

async function installHit(hit) {
  const m = state.mods;
  const inst = modsInstance();
  const key = `${hit.source}:${hit.id}`;
  if (!inst) return;
  m.installing.add(key);
  renderResults();
  try {
    const files = await invoke("install_mod", { instance: inst.id, source: hit.source, project: hit.id, kind: m.kind, title: hit.title, icon: hit.icon });
    // What the mod itself says it needs, beyond what Modrinth lists.
    const more = await invoke("fix_dependencies", { instance: inst.id }).catch(() => null);
    if (more?.added?.length) files.push(...more.added);
    const extra = files.length - 1;
    toast(extra > 0 ? `Installed ${hit.title} and ${extra} mod${extra === 1 ? "" : "s"} it needs.` : `Installed ${hit.title}.`, "good");
    await loadInstalled();
  } catch (err) {
    fail(err);
  }
  m.installing.delete(key);
  if (m.tab === "browse") renderResults();
}

async function installPack(hit) {
  const m = state.mods;
  const key = `${hit.source}:${hit.id}`;
  m.installing.add(key);
  renderResults();
  try {
    const result = await invoke("install_modpack", { source: hit.source, project: hit.id, title: hit.title });
    state.selected = result.instance.id;
    remember("selected", result.instance.id);
    await refresh();
    toast(`${hit.title} is ready as its own instance.`, "good", { label: "Go to Play", run: () => show("play") });
    const n = result.skipped.length;
    if (n) {
      const some = result.skipped.slice(0, 3).join(", ");
      toast(`${n} file${n === 1 ? "" : "s"} can only be downloaded from CurseForge's website: ${some}${n > 3 ? " and more" : ""}.`, "bad");
    }
  } catch (err) {
    fail(err);
  }
  m.installing.delete(key);
  if (m.tab === "browse") renderResults();
}

async function loadInstalled() {
  const inst = modsInstance();
  state.mods.installed = inst ? await invoke("list_mods", { instance: inst.id }).catch(() => []) : [];
  const n = state.mods.installed.length;
  $("#installed-count").textContent = n ? String(n) : "";
}

function instBar() {
  const m = state.mods;
  const inst = modsInstance();
  const show = m.tab === "installed" && m.kind !== "modpack" && !!inst;
  $("#inst-bar").hidden = !show;
  if (!show) return;
  const mine = m.installed.filter((x) => x.kind === m.kind);
  const ups = mine.filter((x) => m.updates.has(x.file));
  const count = (n) => (n ? String(n) : "");
  $("#n-all").textContent = count(mine.length);
  $("#n-on").textContent = count(mine.filter((x) => x.enabled).length);
  $("#n-off").textContent = count(mine.filter((x) => !x.enabled).length);
  $("#n-updates").textContent = count(ups.length);
  const checked = m.updatesFor === inst.id;
  $("#inst-status").textContent = m.checking ? "Checking Modrinth for newer versions"
    : m.checkError && checked ? "Couldn't check for updates just now."
    : !checked ? ""
    : ups.length ? `${ups.length} ${ups.length === 1 ? "update" : "updates"} for ${inst.version}`
    : mine.length ? "Everything is up to date." : "";
  const button = $("#inst-check");
  const busy = m.updating.size > 0;
  if (ups.length) {
    button.className = "btn btn-gold";
    button.replaceChildren(icon("download"), busy ? "Updating" : `Update all ${ups.length}`);
    button.disabled = busy || inst.running;
    button.title = inst.running ? "Close Minecraft to update" : "";
    button.onclick = () => updateMods(ups.map((x) => x.file));
  } else {
    button.className = "btn btn-line";
    button.replaceChildren(m.checking ? "Checking" : "Check for updates");
    button.disabled = m.checking || !mine.length;
    button.title = "";
    button.onclick = () => checkModUpdates(true);
  }
}

function renderInstalled() {
  const m = state.mods;
  const box = $("#mods-results");
  $("#mods-more").hidden = true;
  instBar();
  if (m.kind === "modpack") {
    return box.replaceChildren(empty("A modpack installs as its own instance, so you'll find it on the Play page.", "Go to Play", () => show("play")));
  }
  if (!modsInstance()) return box.replaceChildren(empty("Make an instance first, then add mods to it.", "New instance", openNew));
  const q = m.query.trim().toLowerCase();
  const mine = m.installed.filter((x) => x.kind === m.kind);
  const list = mine.filter((x) =>
    (!q || `${x.title} ${x.description} ${x.authors} ${x.file}`.toLowerCase().includes(q)) &&
    (m.filter === "all" || (m.filter === "on" && x.enabled) || (m.filter === "off" && !x.enabled) || (m.filter === "updates" && m.updates.has(x.file))));
  if (!list.length) {
    const why = !mine.length ? `No ${KIND[m.kind]} in this instance yet.`
      : q ? "Nothing installed matches."
      : m.filter === "updates" ? (m.checking ? "Checking for updates." : "No updates. Everything is on its newest version.")
      : m.filter === "off" ? "Nothing is switched off." : "Nothing is switched on.";
    return box.replaceChildren(empty(why, !mine.length ? "Browse" : null, () => document.querySelector("[data-tab=browse]").click()));
  }
  box.replaceChildren(h("div", { class: "ilist" }, ...list.map(installedRow)));
}

function installedRow(x) {
  const m = state.mods;
  const inst = modsInstance();
  const next = m.updates.get(x.file);
  const busy = m.updating.has(x.file);
  const known = (x.source === "modrinth" || x.source === "curseforge") && x.project;
  const sub = [x.description, x.authors ? `by ${x.authors}` : ""].filter(Boolean).join("  ·  ");
  return h("div", { class: x.enabled ? "irow" : "irow is-off", title: x.file },
    thumb(x.icon, x.title, "thumb-sm"),
    h("div", { class: "irow-main" },
      h("div", { class: "irow-title" },
        known
          ? h("button", { class: "linkish", title: "More about it", onclick: () => openProject({ source: x.source, id: x.project, title: x.title, icon: x.icon }) }, x.title)
          : h("span", {}, x.title),
        x.version ? h("span", { class: "ver" }, x.version) : null),
      sub ? h("div", { class: "irow-sub" }, sub) : null),
    h("div", { class: "irow-side" },
      next ? h("button", {
        class: "btn btn-sm btn-update", disabled: busy || inst?.running,
        title: inst?.running ? "Close Minecraft to update" : `Update to ${next}`, onclick: () => updateMods([x.file]),
      }, icon("download", "i-sm"), busy ? "Updating" : "Update") : null,
      x.kind === "mod"
        ? h("label", { class: "switch", title: x.enabled ? "On" : "Off" },
            h("input", { type: "checkbox", checked: x.enabled, "aria-label": `${x.title} on`, onchange: () => toggleMod(x) }), h("span"))
        : null,
      h("button", { class: "tool tool-bad", title: "Remove", "aria-label": `Remove ${x.title}`, onclick: () => removeMod(x) }, icon("trash"))));
}

// Asks Modrinth about every installed file by its fingerprint: files it knows get their proper name and page, and
// files with a newer version for this instance's Minecraft and loader are marked. Runs on its own when Installed
// opens, once per instance.
async function checkModUpdates(manual) {
  const m = state.mods;
  const inst = modsInstance();
  if (!inst || m.checking) return;
  m.checking = true;
  m.checkError = "";
  instBar();
  try {
    const list = await invoke("check_updates", { instance: inst.id });
    if (m.instance === inst.id) {
      m.updates = new Map(list.map((u) => [u.file, u.version]));
      await loadInstalled();
      if (manual && !list.length) toast("Everything is up to date.");
    }
  } catch (err) {
    m.checkError = String(err);
    if (manual) fail(err);
  } finally {
    m.checking = false;
    if (m.instance === inst.id) m.updatesFor = inst.id;
  }
  if (m.tab === "installed" && state.view === "mods") renderInstalled();
}

async function updateMods(files) {
  const m = state.mods;
  const inst = modsInstance();
  if (!inst || !files.length) return;
  if (inst.running) return toast("Close Minecraft first: its mods are in use while it runs.", "bad");
  for (const f of files) m.updating.add(f);
  renderInstalled();
  try {
    const n = await invoke("update_mods", { instance: inst.id, files });
    toast(n === 1 ? "Updated 1 file to its newest version." : `Updated ${n} files to their newest versions.`, "good");
  } catch (err) {
    fail(err);
  }
  m.updating.clear();
  m.updatesFor = "";
  await loadInstalled();
  await checkModUpdates(false);
}

async function toggleMod(x) {
  try {
    await invoke("toggle_mod", { instance: state.mods.instance, kind: x.kind, file: x.file });
  } catch (err) {
    fail(err);
  }
  await loadInstalled();
  renderInstalled();
}

async function removeMod(x) {
  if (!(await ask(`Remove ${x.title}?`, "The file is deleted from this instance. You can install it again any time.", "Remove"))) return;
  try {
    await invoke("remove_mod", { instance: state.mods.instance, kind: x.kind, file: x.file });
    toast(`Removed ${x.title}.`);
  } catch (err) {
    fail(err);
  }
  await loadInstalled();
  renderInstalled();
}

// ---------------------------------------------------------------- accounts (the menu on the account button)

async function loadAccounts() {
  try {
    state.accounts = await invoke("list_accounts");
  } catch (err) {
    fail(err);
  }
  renderWho();
  renderAcctMenu();
}

function openAcctMenu() {
  renderAcctMenu();
  $("#acct-menu").hidden = false;
  $("#who").setAttribute("aria-expanded", "true");
}

function closeAcctMenu() {
  $("#acct-menu").hidden = true;
  $("#who").setAttribute("aria-expanded", "false");
}

function renderAcctMenu() {
  const rows = state.accounts.map((a) => h("div", { class: a.selected ? "acct-row is-selected" : "acct-row", role: "menuitem" },
    h("button", { class: "acct-pick", type: "button", onclick: () => (a.selected ? null : useAccount(a.uuid)) },
      a.selected && state.face ? h("img", { class: "face", src: state.face, alt: "" }) : h("span", { class: "ring ring-sm", "aria-hidden": "true" }, a.name.charAt(0).toUpperCase()),
      h("span", { class: "acct-name" }, h("strong", {}, a.name), h("small", {}, a.selected ? "Plays when you press Play" : "Switch to this account"))),
    a.selected ? h("span", { class: "ok" }, icon("check", "i-sm")) : null,
    h("button", { class: "tool tool-bad", type: "button", title: "Sign out", "aria-label": `Sign out ${a.name}`, onclick: () => signOut(a) }, icon("close"))));
  const signedIn = state.accounts.some((a) => a.selected);
  $("#acct-menu").replaceChildren(...[
    ...(rows.length ? rows : [h("p", { class: "acct-none" }, "No account yet. Minecraft needs the Microsoft account that owns Java Edition.")]),
    h("div", { class: "menu-line" }),
    signedIn ? h("button", { class: "menu-item", type: "button", role: "menuitem", onclick: openSkin }, icon("user"), "Change skin") : null,
    h("button", { class: "menu-item", type: "button", role: "menuitem", onclick: () => { closeAcctMenu(); addAccount(); } }, icon("plus"), "Add account"),
  ].filter(Boolean));
}

async function useAccount(uuid) {
  try {
    state.accounts = await invoke("select_account", { uuid });
  } catch (err) {
    fail(err);
  }
  state.face = "";
  renderWho();
  renderAcctMenu();
  loadFace();
  if (state.view === "skins") enterSkins();
}

async function signOut(a) {
  closeAcctMenu();
  if (!(await ask(`Sign out ${a.name}?`, "The launcher forgets this account. You can add it again any time.", "Sign out"))) return;
  try {
    state.accounts = await invoke("remove_account", { uuid: a.uuid });
  } catch (err) {
    fail(err);
  }
  state.face = "";
  renderWho();
  loadFace();
  if (state.view === "skins") enterSkins();
}

/** The selected account's face, cut from its skin, for the account button. Quietly skipped when offline. */
async function loadFace() {
  if (!state.accounts.some((a) => a.selected)) return;
  try {
    const p = await invoke("get_profile");
    state.face = p.skin ? await faceOf(p.skin) : "";
    renderWho();
  } catch {
    // no face: the initial stays
  }
}

async function addAccount() {
  let code;
  try {
    code = await invoke("login_start");
  } catch (err) {
    return fail(err);
  }
  const dialog = $("#dlg-login");
  dialog.classList.remove("is-error");
  $("#login-code").textContent = code.user_code;
  $("#login-state").textContent = "Waiting for you to sign in";
  $("#login-open").onclick = async () => {
    try {
      await navigator.clipboard.writeText(code.user_code);
    } catch {
      // The code is on screen to type in by hand.
    }
    invoke("open_url", { url: code.verification_uri }).catch(fail);
  };
  dialog.showModal();
  try {
    state.accounts = await invoke("login_poll", { deviceCode: code.device_code, interval: code.interval, expiresIn: code.expires_in });
    dialog.close();
    const who = state.accounts.find((a) => a.selected);
    toast(who ? `Signed in as ${who.name}.` : "Signed in.", "good");
    renderWho();
    renderAcctMenu();
    loadFace();
    if (state.view === "skins") enterSkins();
  } catch (err) {
    if (String(err) === "cancelled") return;
    dialog.classList.add("is-error");
    $("#login-state").textContent = String(err);
  }
}

function renderWho() {
  const a = state.accounts.find((x) => x.selected);
  $("#who").replaceChildren(
    a && state.face ? h("img", { class: "face", src: state.face, alt: "" }) : h("div", { class: "ring ring-sm", "aria-hidden": "true" }, a ? a.name.charAt(0).toUpperCase() : "?"),
    h("div", { class: "who-text" }, h("strong", {}, a ? a.name : "Not signed in"), h("span", {}, a ? "Microsoft account" : "Add an account")),
  );
}

// ---------------------------------------------------------------- settings

let saveTimer;

function renderSettings() {
  const s = state.settings;
  if (!s) return;
  // The slider stops 2 GB short of this computer's memory, so the system always keeps some.
  if (state.ram) {
    $("#s-mem").max = String(Math.min(32768, Math.max(2048, Math.floor((state.ram - 2048) / 512) * 512)));
    $("#s-mem-ram").textContent = ` This PC has ${Math.round(state.ram / 1024)} GB.`;
  }
  $("#s-mem").value = s.memory_mb;
  $("#s-gc").checked = s.tuned_gc;
  $("#s-min").checked = s.minimize_on_launch;
  $("#s-jei").checked = s.quiet_jei;
  $("#s-client").checked = s.client_everywhere;
  renderBgPicker();
  $("#s-console").checked = remember("console") !== "off";
  $("#s-intro").checked = remember("intro") !== "off";
  $("#s-java").value = s.java_path;
  $("#s-jvm").value = s.jvm_args;
  memLabel();
}

function memLabel() {
  const mb = Number($("#s-mem").value);
  $("#s-mem-out").textContent = `${(mb / 1024).toFixed(mb % 1024 ? 1 : 0)} GB`;
}

function saveSettings() {
  const s = state.settings;
  if (!s) return;
  Object.assign(s, {
    memory_mb: Number($("#s-mem").value),
    tuned_gc: $("#s-gc").checked,
    minimize_on_launch: $("#s-min").checked,
    quiet_jei: $("#s-jei").checked,
    client_everywhere: $("#s-client").checked,
    java_path: $("#s-java").value.trim(),
    jvm_args: $("#s-jvm").value.trim(),
  });
  memLabel();
  clearTimeout(saveTimer);
  saveTimer = setTimeout(async () => {
    try {
      await invoke("save_settings", { settings: s });
      const saved = $("#saved");
      saved.textContent = "Saved";
      clearTimeout(saved.timer);
      saved.timer = setTimeout(() => (saved.textContent = ""), 1600);
    } catch (err) {
      fail(err);
    }
  }, 400);
}

// ---------------------------------------------------------------- wiring

function wire() {
  const win = window.__TAURI__.window.getCurrentWindow();
  $("#wc-min").addEventListener("click", () => win.minimize().catch(fail));
  $("#wc-max").addEventListener("click", () => win.toggleMaximize().catch(fail));
  $("#wc-close").addEventListener("click", () => win.close().catch(fail));
  for (const b of $$(".nav-item")) b.addEventListener("click", () => show(b.dataset.view));
  $("#who").addEventListener("click", (ev) => {
    ev.stopPropagation();
    if ($("#acct-menu").hidden) openAcctMenu();
    else closeAcctMenu();
  });
  $("#acct-menu").addEventListener("click", (ev) => ev.stopPropagation());
  document.addEventListener("click", () => closeAcctMenu());
  document.addEventListener("keydown", (ev) => { if (ev.key === "Escape") closeAcctMenu(); });
  $("#import-pack").addEventListener("click", () => $("#import-file").click());
  $("#import-file").addEventListener("change", importPack);
  $("#skin-file").addEventListener("change", pickSkin);
  $("#skin-pack-file").addEventListener("change", importSkinPack);
  for (const b of $$("#inst-tabs button")) b.addEventListener("click", () => { ins.tab = b.dataset.t; renderInst(); });
  const nav = $(".tabs-nav");
  nav.addEventListener("pointermove", (ev) => {
    const r = nav.getBoundingClientRect();
    nav.style.setProperty("--mx", `${ev.clientX - r.left}px`);
    nav.style.setProperty("--my", `${ev.clientY - r.top}px`);
  });
  document.fonts.ready.then(() => moveLens(false));
  addEventListener("resize", () => moveLens(false));
  document.addEventListener("click", (ev) => {
    const link = ev.target.closest("[data-url]");
    if (link) invoke("open_url", { url: link.dataset.url }).catch(fail);
    const close = ev.target.closest("[data-close]");
    if (close) close.closest("dialog").close();
  });

  $("#new-filter").addEventListener("click", (ev) => {
    const b = ev.target.closest("button");
    if (!b) return;
    draft.filter = b.dataset.f;
    pressed("#new-filter", b);
    renderVersions();
  });
  $("#new-search").addEventListener("input", (ev) => {
    draft.query = ev.target.value;
    renderVersions();
  });
  $("#new-versions").addEventListener("keydown", (ev) => {
    if (ev.key !== "ArrowDown" && ev.key !== "ArrowUp") return;
    ev.preventDefault();
    const i = draft.visible.indexOf(draft.version);
    const next = draft.visible[Math.min(draft.visible.length - 1, Math.max(0, i + (ev.key === "ArrowDown" ? 1 : -1)))];
    if (next) pickVersion(next, true);
  });
  $("#new-loader").addEventListener("click", (ev) => {
    const b = ev.target.closest("button");
    if (b) setLoader(b.dataset.l);
  });
  $("#new-create").addEventListener("click", createInstance);

  $("#mods-instance").addEventListener("change", (ev) => {
    state.mods.instance = ev.target.value;
    state.mods.updates = new Map();
    state.mods.updatesFor = "";
    enterMods();
  });
  $("#inst-filter").addEventListener("click", (ev) => {
    const b = ev.target.closest("button");
    if (!b) return;
    state.mods.filter = b.dataset.f;
    pressed("#inst-filter", b);
    renderInstalled();
  });
  for (const tab of $$("#view-mods [role=tab]")) {
    tab.addEventListener("click", () => {
      state.mods.tab = tab.dataset.tab;
      for (const t of $$("#view-mods [role=tab]")) t.setAttribute("aria-selected", String(t === tab));
      drawMods(state.mods.tab === "browse" && !state.mods.hits.length);
    });
  }
  $("#mods-kind").addEventListener("click", (ev) => {
    const b = ev.target.closest("button");
    if (!b) return;
    state.mods.kind = b.dataset.k;
    state.mods.category = "";
    pressed("#mods-kind", b);
    drawMods(true);
  });
  $("#mods-source").addEventListener("click", (ev) => {
    const b = ev.target.closest("button");
    if (!b) return;
    state.mods.source = b.dataset.s;
    pressed("#mods-source", b);
    drawMods(true);
  });
  let queryTimer;
  $("#mods-q").addEventListener("input", (ev) => {
    state.mods.query = ev.target.value;
    clearTimeout(queryTimer);
    queryTimer = setTimeout(() => drawMods(true), 320);
  });
  $("#mods-more").addEventListener("click", () => search(false));
  $("#mod-lightbox").addEventListener("click", (ev) => (ev.currentTarget.hidden = true));
  // Links in a description work from the keyboard too.
  $("#mod-page").addEventListener("keydown", (ev) => {
    const link = ev.target.closest(".md a[data-url]");
    if (link && (ev.key === "Enter" || ev.key === " ")) {
      ev.preventDefault();
      link.click();
    }
  });
  $("#mods-sort").addEventListener("change", (ev) => {
    state.mods.sort = ev.target.value;
    search(true);
  });
  // The chip row scrolls sideways with an ordinary mouse wheel.
  $("#mods-cats").addEventListener("wheel", (ev) => {
    if (Math.abs(ev.deltaY) <= Math.abs(ev.deltaX)) return;
    ev.preventDefault();
    ev.currentTarget.scrollLeft += ev.deltaY;
  }, { passive: false });
  $("#mods-folder").addEventListener("click", () => {
    const inst = modsInstance();
    if (inst) openFolder(inst.id, FOLDER[state.mods.kind]);
  });

  $("#dlg-login").addEventListener("close", () => invoke("login_cancel").catch(() => {}));

  for (const id of ["s-mem", "s-java", "s-jvm"]) $(`#${id}`).addEventListener("input", saveSettings);
  for (const id of ["s-gc", "s-min", "s-jei", "s-client"]) $(`#${id}`).addEventListener("change", saveSettings);
  $("#s-open").addEventListener("click", () => invoke("open_data_folder").catch(fail));
  // A per-PC preference, so it lives with the window rather than in the launcher's settings file.
  $("#s-console").addEventListener("change", (ev) => remember("console", ev.target.checked ? "on" : "off"));
  $("#bg-mode").addEventListener("click", (ev) => {
    const b = ev.target.closest("button");
    if (!b) return;
    remember("bg-mode", b.dataset.m);
    if (b.dataset.m === "shuffle") shuffled = null; // a fresh shuffle, so the change shows at once
    setBackground(state.view);
    renderBgPicker();
  });
  $("#s-intro").addEventListener("change", (ev) => remember("intro", ev.target.checked ? "on" : "off"));

}

let fetchingNew = false;

listen("progress", ({ payload }) => {
  // A modpack's new instance reports progress before the list knows it; fetch the list once to show its name.
  if (!fetchingNew && !state.instances.some((i) => i.id === payload.id)) {
    fetchingNew = true;
    refresh().finally(() => (fetchingNew = false));
  }
  const before = state.busy.get(payload.id)?.stage;
  if (["Ready", "Stopped", "Playing"].includes(payload.stage)) state.busy.delete(payload.id);
  else state.busy.set(payload.id, payload);
  status();
  if (state.view === "play" && before !== state.busy.get(payload.id)?.stage) renderPlay();
});

listen("game", async ({ payload }) => {
  await refresh();
  if (!payload.running && payload.code && !payload.forced) {
    toast(`Minecraft closed with an error (code ${payload.code}).`, "bad", { label: "Console", run: () => openConsole(payload.id) });
  }
  if (consoleView.id === payload.id) consoleHead();
});

// ---------------------------------------------------------------- the console

// What each game printed, kept here as it arrives so the console opens instantly. The launcher keeps the same lines,
// so a console opened later (or after a reload) fetches them once.
const KEEP_LINES = 5000;
const logs = new Map();
const consoleView = { id: null, filter: "all", query: "" };

function levelOf(line) {
  if (/\/(ERROR|FATAL)\]|^\s+at |^Caused by:|^[\w.$]+(Exception|Error)\b|BUILD FAILED|^FAILURE:|^> Task .*FAILED/.test(line)) return "error";
  if (/\/WARN\]|^warning:/i.test(line)) return "warn";
  if (/^\[[\d:]+\] \[Launcher\] /.test(line)) return "launcher";
  if (/^> Task |^> Configure|^BUILD SUCCESSFUL|actionable task/.test(line)) return "quiet";
  return "";
}

function shows(line) {
  const level = levelOf(line);
  if (consoleView.filter === "error" && level !== "error") return false;
  if (consoleView.filter === "warn" && level !== "error" && level !== "warn") return false;
  return !consoleView.query || line.toLowerCase().includes(consoleView.query);
}

function lineEl(line) {
  const level = levelOf(line);
  return h("div", level ? { class: `ln-${level}` } : {}, line);
}

function consoleHead() {
  const inst = state.instances.find((i) => i.id === consoleView.id);
  $("#console-of").textContent = inst ? inst.name : "";
  const busy = state.busy.get(consoleView.id);
  const el = $("#console-state");
  el.className = inst?.running ? "live" : busy ? "live live-gold" : "live live-off";
  el.replaceChildren(...(inst?.running || busy ? [h("span", { class: "dot" })] : []), inst?.running ? "Running" : busy ? busy.stage : "Not running");
}

function renderConsole() {
  const box = $("#console-lines");
  const lines = (logs.get(consoleView.id) || []).filter(shows);
  if (!lines.length) {
    const any = (logs.get(consoleView.id) || []).length;
    box.replaceChildren(h("div", { class: "console-empty" }, any ? "Nothing matches." : "Nothing yet. Press Play and the game's output shows up here as it happens."));
  } else {
    box.replaceChildren(...lines.map(lineEl));
  }
  box.scrollTop = box.scrollHeight;
  $("#console-latest").hidden = true;
}

async function openConsole(id, starting) {
  consoleView.id = id;
  if (!starting && !logs.has(id)) {
    try {
      logs.set(id, await invoke("get_game_log", { id }));
    } catch {
      logs.set(id, []);
    }
  }
  if (starting) logs.set(id, []);
  consoleHead();
  renderConsole();
  const dialog = $("#dlg-console");
  if (!dialog.open) dialog.showModal();
}

listen("game-log", ({ payload }) => {
  const list = payload.reset ? [] : logs.get(payload.id) || [];
  list.push(...payload.lines);
  if (list.length > KEEP_LINES) list.splice(0, list.length - KEEP_LINES);
  logs.set(payload.id, list);
  if (consoleView.id !== payload.id || !$("#dlg-console").open) return;
  if (payload.reset) return renderConsole();
  const box = $("#console-lines");
  const atBottom = box.scrollHeight - box.scrollTop - box.clientHeight < 40;
  const fresh = payload.lines.filter(shows);
  if (!fresh.length) return;
  box.querySelector(".console-empty")?.remove();
  box.append(...fresh.map(lineEl));
  while (box.childElementCount > KEEP_LINES) box.firstElementChild.remove();
  if (atBottom) box.scrollTop = box.scrollHeight;
  else $("#console-latest").hidden = false;
  consoleHead();
});

function wireConsole() {
  const box = $("#console-lines");
  box.addEventListener("scroll", () => {
    if (box.scrollHeight - box.scrollTop - box.clientHeight < 40) $("#console-latest").hidden = true;
  });
  $("#console-latest").addEventListener("click", () => {
    box.scrollTop = box.scrollHeight;
    $("#console-latest").hidden = true;
  });
  $("#console-filter").addEventListener("click", (ev) => {
    const b = ev.target.closest("button");
    if (!b) return;
    pressed("#console-filter", b);
    consoleView.filter = b.dataset.f;
    renderConsole();
  });
  let timer;
  $("#console-search").addEventListener("input", (ev) => {
    clearTimeout(timer);
    timer = setTimeout(() => {
      consoleView.query = ev.target.value.trim().toLowerCase();
      renderConsole();
    }, 120);
  });
  $("#console-copy").addEventListener("click", async () => {
    const lines = (logs.get(consoleView.id) || []).filter(shows);
    try {
      await navigator.clipboard.writeText(lines.join("\n"));
      toast(`Copied ${lines.length.toLocaleString()} ${lines.length === 1 ? "line" : "lines"}.`);
    } catch (err) {
      fail(err);
    }
  });
  $("#console-folder").addEventListener("click", () => openFolder(consoleView.id, "logs"));
}

// ---------------------------------------------------------------- the opening

// CSS plays it from the very first frame. This places the sparks, lets a click or key skip it, and tidies up.
function intro() {
  const el = $("#opening");
  const done = () => {
    el.remove();
    document.body.classList.remove("booting");
  };
  if (remember("intro") === "off" || matchMedia("(prefers-reduced-motion: reduce)").matches) return done();
  // performance.now() counts from when the page began, which is when the CSS animations started.
  const since = performance.now();
  let seed = 11;
  const rand = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
  const sparks = $("#sparks");
  for (let i = 0; i < 16; i++) {
    const angle = (i / 16) * Math.PI * 2 + rand() * 0.35;
    const reach = 110 + rand() * 180;
    const delay = Math.max(0, 560 + rand() * 90 - since);
    // Squashed vertically to follow the tilt of the black hole's disk.
    sparks.append(h("span", {
      class: "spark",
      style: `--dx:${(Math.cos(angle) * reach).toFixed(1)}px;--dy:${(Math.sin(angle) * reach * 0.62).toFixed(1)}px;--s:${(2 + rand() * 2.4).toFixed(1)}px;animation-delay:${delay.toFixed(0)}ms`,
    }));
  }
  el.addEventListener("pointerdown", done, { once: true });
  const key = () => done();
  addEventListener("keydown", key, { once: true });
  setTimeout(() => {
    el.remove();
    removeEventListener("keydown", key);
  }, Math.max(0, 2800 - since));
  setTimeout(() => document.body.classList.remove("booting"), Math.max(0, 3400 - since));
}

async function init() {
  intro();
  setBackground("play");
  wire();
  wireConsole();
  try {
    [state.settings, state.accounts, state.ram, state.version, state.features] = await Promise.all([
      invoke("get_settings"), invoke("list_accounts"), invoke("system_memory_mb"), window.__TAURI__.app.getVersion(), invoke("features"),
    ]);
  } catch (err) {
    fail(err);
  }
  // CurseForge appears once this version of the launcher carries its own CurseForge key; until then, Modrinth only.
  if (!state.features?.curseforge) {
    state.features = { curseforge: false };
    state.mods.source = "modrinth";
    $("#mods-sources").textContent = "Modrinth";
  }
  renderWho();
  loadFace();
  await refresh();
  $("#s-update").addEventListener("click", () => (update.ready ? installUpdate() : checkUpdate(true)));
  $("#update-pill").addEventListener("click", installUpdate);
  // While installing, the card can't be dismissed: the launcher is about to close for the installer.
  $("#dlg-update").addEventListener("cancel", (ev) => ev.preventDefault());
  setTimeout(() => checkUpdate(false), 3000);
  setInterval(() => { if (!update.ready) checkUpdate(false); }, EVERY_FEW_HOURS);
}

// ---------------------------------------------------------------- launcher updates

// The launcher looks for a newer version when it opens and every few hours after, and downloads it in the
// background. Once it's ready, a small Update button appears at the top right; pressing it shows a small Updating card
// while the new version installs out of sight, then the launcher reopens on it. Nothing installs by itself, so a game
// or a download is never cut off.
const update = { checking: false, ready: null, pct: 0 };
const EVERY_FEW_HOURS = 6 * 60 * 60 * 1000;

function updateRow() {
  const text = $("#s-update-text");
  const button = $("#s-update");
  if (update.ready) {
    text.textContent = `Version ${update.ready.version} is downloaded and ready.`;
    button.textContent = "Restart to update";
    button.classList.add("btn-gold");
    button.classList.remove("btn-line");
  } else if (update.checking) {
    text.textContent = update.pct ? `Downloading the new version: ${update.pct}%` : "Checking for a new version";
    button.textContent = "Checking";
  } else {
    text.textContent = update.note || "New versions download in the background. When one is ready, an Update button appears at the top right.";
    button.textContent = "Check now";
  }
  button.disabled = update.checking;
}

async function checkUpdate(manual) {
  if (update.checking) return;
  update.checking = true;
  update.pct = 0;
  updateRow();
  try {
    update.ready = await invoke("update_check");
    update.note = update.ready ? "" : `You have the newest version (v${state.version}).`;
    if (!update.ready && manual) toast("The launcher is up to date.");
  } catch (err) {
    update.note = "Couldn't reach the update server. It tries again next time the launcher opens.";
    if (manual) fail(err);
  }
  update.checking = false;
  updateRow();
  updatePill();
  if (update.ready && manual) toast(`Version ${update.ready.version} is ready. Press Update at the top right.`);
}

function updatePill() {
  const pill = $("#update-pill");
  pill.hidden = !update.ready;
  if (update.ready) pill.title = `Version ${update.ready.version} is ready to install`;
}

async function installUpdate() {
  if (!update.ready) return;
  if (state.instances.some((i) => i.running)) {
    const yes = await ask("Update while playing?", "The game keeps running, but the launcher closes for a moment, so its console stops showing this game's output.", "Update now");
    if (!yes) return;
  }
  const card = $("#dlg-update");
  $("#updating-version").textContent = `Version ${update.ready.version}`;
  $("#updating-notes").textContent = update.ready.notes || "";
  $("#updating-state").textContent = "Installing. The launcher reopens in a moment.";
  card.showModal();
  // Long enough to read, then the installer takes over; the launcher closes for it and reopens when it's done.
  await new Promise((r) => setTimeout(r, 1400));
  try {
    await invoke("update_install");
  } catch (err) {
    card.close();
    update.ready = null;
    updateRow();
    updatePill();
    fail(err);
  }
}

listen("update", ({ payload }) => {
  update.pct = payload.total ? Math.floor((payload.done / payload.total) * 100) : 0;
  updateRow();
});

init();

// ---------------------------------------------------------------- the instance screen (double-click an instance)

const IKIND = { mod: "Mods", resourcepack: "Resource packs", shader: "Shaders", datapack: "Data packs" };
const PAGE = 20;
const ins = { id: "", tab: "content", kind: "all", query: "", sort: "az", page: 0, items: [], rel: "", exportConfig: true, exporting: false };
const insInst = () => state.instances.find((i) => i.id === ins.id);

function since(seconds) {
  if (!seconds) return "";
  let d = Math.round(seconds - Date.now() / 1000);
  if (Math.abs(d) < 60) return "just now";
  d = Math.round(d / 60);
  for (const [n, unit] of [[60, "minute"], [24, "hour"], [7, "day"], [4.35, "week"], [12, "month"]]) {
    if (Math.abs(d) < n) return relative.format(d, unit);
    d = Math.round(d / n);
  }
  return relative.format(d, "year");
}

function size(n) {
  if (n < 1024) return `${n} B`;
  if (n < 1048576) return `${(n / 1024).toFixed(n < 10240 ? 1 : 0)} KB`;
  return `${(n / 1048576).toFixed(n < 10485760 ? 1 : 0)} MB`;
}

async function openInstance(id) {
  Object.assign(ins, { id, tab: "content", kind: "all", query: "", page: 0, rel: "", items: [] });
  $("#dlg-inst").showModal();
  renderInst();
  await loadInstContent();
  if (ins.tab === "content") renderInst();
}

async function loadInstContent() {
  const [mods, packs] = await Promise.all([
    invoke("list_mods", { instance: ins.id }).catch(() => []),
    invoke("list_datapacks", { instance: ins.id }).catch(() => []),
  ]);
  ins.items = [...mods, ...packs];
}

function renderInst() {
  const inst = insInst();
  if (!inst) return $("#dlg-inst").close();
  const busy = state.busy.get(inst.id);
  $("#inst-head").replaceChildren(
    h("div", { class: "inst-art", style: `background-image:url("img/${inst.loader}-sm.jpg")`, "aria-hidden": "true" }),
    h("div", { class: "inst-name" }, h("h2", { id: "inst-title" }, inst.name), h("p", {}, versionLine(inst))),
    h("div", { class: "inst-actions" },
      inst.running ? h("span", { class: "live" }, h("span", { class: "dot" }), "Playing")
        : h("button", { class: "btn btn-gold", disabled: !!busy, onclick: () => play(inst.id) }, icon("play"), busy ? "Getting ready" : "Play"),
      h("button", { class: "tool", title: "Open folder", "aria-label": "Open folder", onclick: () => openFolder(inst.id, "") }, icon("folder")),
      h("button", { class: "tool", title: "Close", "aria-label": "Close", onclick: () => $("#dlg-inst").close() }, icon("close"))));
  for (const b of $$("#inst-tabs button")) b.setAttribute("aria-current", String(b.dataset.t === ins.tab));
  const box = $("#inst-body");
  if (ins.tab === "content") renderContent(box);
  else if (ins.tab === "files") renderFiles(box);
  else if (ins.tab === "worlds") renderWorlds(box);
  else renderShare(box);
}

// ---- Content: everything installed, searchable, filtered by kind, a page at a time

function renderContent(box) {
  const q = ins.query.trim().toLowerCase();
  const count = (k) => ins.items.filter((x) => k === "all" || x.kind === k).length;
  let list = ins.items.filter((x) => (ins.kind === "all" || x.kind === ins.kind) &&
    (!q || `${x.title} ${x.file} ${x.description} ${x.authors}`.toLowerCase().includes(q)));
  const by = (x) => x.title.toLowerCase();
  if (ins.sort === "az") list.sort((a, b) => by(a).localeCompare(by(b)));
  else if (ins.sort === "za") list.sort((a, b) => by(b).localeCompare(by(a)));
  else list.sort((a, b) => Number(b.enabled) - Number(a.enabled) || by(a).localeCompare(by(b)));
  const pages = Math.max(1, Math.ceil(list.length / PAGE));
  ins.page = Math.min(ins.page, pages - 1);
  const shown = list.slice(ins.page * PAGE, ins.page * PAGE + PAGE);

  const search = h("label", { class: "search" }, icon("search"),
    h("input", { type: "search", value: ins.query, placeholder: `Search ${ins.items.length} projects...`, "aria-label": "Search", autocomplete: "off", spellcheck: "false",
      oninput: (ev) => { ins.query = ev.target.value; ins.page = 0; renderContent(box); setTimeout(() => { const i = $("#inst-body input[type=search]"); if (i) { i.focus(); i.setSelectionRange(i.value.length, i.value.length); } }); } }));
  const sort = h("label", { class: "inline glass sort" }, h("span", {}, "Sort"),
    h("select", { "aria-label": "Sort", onchange: (ev) => { ins.sort = ev.target.value; renderContent(box); } },
      ...[["az", "Name (A-Z)"], ["za", "Name (Z-A)"], ["on", "Switched on first"]].map(([v, t]) => h("option", { value: v, selected: ins.sort === v }, t))));
  const add = h("button", { class: "btn btn-line", onclick: () => { $("#dlg-inst").close(); state.mods.instance = ins.id; show("mods"); } }, icon("plus"), "Add content");
  const chips = h("div", { class: "ichips", role: "group", "aria-label": "Show" },
    ...["all", "mod", "resourcepack", "shader", "datapack"].map((k) => h("button", {
      class: "chip", type: "button", "aria-pressed": String(ins.kind === k), onclick: () => { ins.kind = k; ins.page = 0; renderContent(box); },
    }, k === "all" ? "All" : IKIND[k], h("span", { class: "n" }, String(count(k))))));

  const rows = shown.length
    ? h("div", { class: "ilist" }, ...shown.map(contentRow))
    : empty(!ins.items.length ? "Nothing installed in this instance yet." : q ? "Nothing matches." : `No ${(IKIND[ins.kind] || "content").toLowerCase()} in this instance.`,
        !ins.items.length ? "Add content" : null, () => add.click());
  box.replaceChildren(...[h("div", { class: "ibar" }, search, sort, add), chips, rows, pager(pages, box)].filter(Boolean));
}

function contentRow(x) {
  const sub = [x.description, x.authors ? `by ${x.authors}` : ""].filter(Boolean).join("  ·  ");
  return h("div", { class: x.enabled ? "irow" : "irow is-off", title: x.file },
    thumb(x.icon, x.title, "thumb-sm"),
    h("div", { class: "irow-main" },
      h("div", { class: "irow-title" }, h("span", {}, x.title), x.version ? h("span", { class: "ver" }, x.version) : null, h("span", { class: "kind" }, IKIND[x.kind] || "")),
      sub ? h("div", { class: "irow-sub" }, sub) : null),
    h("div", { class: "irow-side" },
      h("label", { class: "switch", title: x.enabled ? "On" : "Off" },
        h("input", { type: "checkbox", checked: x.enabled, "aria-label": `${x.title} on`, onchange: () => toggleContent(x) }), h("span")),
      h("button", { class: "tool tool-bad", title: "Remove", "aria-label": `Remove ${x.title}`, onclick: () => removeContent(x) }, icon("trash"))));
}

function pager(pages, box) {
  if (pages <= 1) return null;
  const go = (p) => { ins.page = p; renderContent(box); $("#inst-body").scrollTop = 0; };
  const nums = [];
  for (let p = 0; p < pages; p++) {
    if (p === 0 || p === pages - 1 || Math.abs(p - ins.page) <= 1) nums.push(p);
    else if (nums[nums.length - 1] !== -1) nums.push(-1);
  }
  return h("nav", { class: "pager", "aria-label": "Pages" },
    h("button", { class: "tool", disabled: ins.page === 0, "aria-label": "Previous page", onclick: () => go(ins.page - 1) }, "‹"),
    ...nums.map((p) => (p < 0 ? h("span", { class: "gap" }, "…")
      : h("button", { class: "pg", "aria-current": String(p === ins.page), onclick: () => go(p) }, String(p + 1)))),
    h("button", { class: "tool", disabled: ins.page === pages - 1, "aria-label": "Next page", onclick: () => go(ins.page + 1) }, "›"));
}

async function toggleContent(x) {
  try {
    if (x.kind === "datapack") await invoke("toggle_datapack", { instance: ins.id, file: x.file });
    else await invoke("toggle_mod", { instance: ins.id, kind: x.kind, file: x.file });
  } catch (err) {
    fail(err);
  }
  await loadInstContent();
  renderInst();
}

async function removeContent(x) {
  if (!(await ask(`Remove ${x.title}?`, "The file is deleted from this instance. You can install it again any time.", "Remove"))) return;
  try {
    if (x.kind === "datapack") await invoke("remove_datapack", { instance: ins.id, file: x.file });
    else await invoke("remove_mod", { instance: ins.id, kind: x.kind, file: x.file });
    toast(`Removed ${x.title}.`);
  } catch (err) {
    fail(err);
  }
  await loadInstContent();
  renderInst();
}

// ---- Files: the instance's game folder, one folder at a time

async function renderFiles(box) {
  const rel = ins.rel;
  let list = [];
  try {
    list = await invoke("list_dir", { instance: ins.id, rel });
  } catch (err) {
    ins.rel = "";
    return box.replaceChildren(empty(String(err), "Back to the top", () => renderFiles(box)));
  }
  if (ins.tab !== "files" || ins.rel !== rel) return;
  const parts = rel ? rel.split("/") : [];
  const crumbs = h("div", { class: "crumbs" },
    h("button", { class: "linkish", onclick: () => { ins.rel = ""; renderFiles(box); } }, insInst()?.name || "Game folder"),
    ...parts.flatMap((p, i) => [h("span", { class: "sep" }, "/"),
      h("button", { class: "linkish", onclick: () => { ins.rel = parts.slice(0, i + 1).join("/"); renderFiles(box); } }, p)]));
  const open = h("button", { class: "btn btn-line", onclick: () => invoke("open_dir", { instance: ins.id, rel }).catch(fail) }, icon("folder"), `Open in ${FILES}`);
  const rows = list.length
    ? h("div", { class: "ilist flist" }, ...list.map((f) => h("div", {
        class: f.dir ? "frow is-dir" : "frow",
        onclick: f.dir ? () => { ins.rel = rel ? `${rel}/${f.name}` : f.name; renderFiles(box); } : null,
      }, icon(f.dir ? "folder" : "file"), h("span", { class: "fname" }, f.name),
        h("span", { class: "fmeta" }, f.dir ? "" : size(f.size)), h("span", { class: "fmeta" }, since(f.modified)))))
    : empty("This folder is empty.");
  box.replaceChildren(h("div", { class: "ibar" }, crumbs, open), rows);
}

// ---- Worlds: the instance's saves

async function renderWorlds(box) {
  let list = [];
  try {
    list = await invoke("list_worlds", { instance: ins.id });
  } catch (err) {
    return box.replaceChildren(empty(String(err)));
  }
  if (ins.tab !== "worlds") return;
  if (!list.length) return box.replaceChildren(empty("No worlds yet. Make one in Singleplayer and it shows up here."));
  box.replaceChildren(h("div", { class: "wgrid" }, ...list.map((w) => h("article", { class: "world" },
    thumb(w.icon, w.id, "thumb-lg"),
    h("div", { class: "world-main" }, h("h3", {}, w.id),
      h("p", {}, [w.modified ? `Played ${since(w.modified)}` : "", w.packs ? `${w.packs} data pack${w.packs === 1 ? "" : "s"}` : ""].filter(Boolean).join(" · "))),
    h("div", { class: "world-tools" },
      h("button", { class: "tool", title: "Open folder", "aria-label": `Open ${w.id}'s folder`, onclick: () => invoke("open_world", { instance: ins.id, id: w.id }).catch(fail) }, icon("folder")),
      h("button", { class: "tool tool-bad", title: "Delete", "aria-label": `Delete ${w.id}`, onclick: () => deleteWorld(w, box) }, icon("trash")))))));
}

async function deleteWorld(w, box) {
  if (!(await ask(`Delete ${w.id}?`, `The world goes to the ${BIN}, so you can still get it back from there.`, "Delete"))) return;
  try {
    await invoke("delete_world", { instance: ins.id, id: w.id });
    toast(`${w.id} is in the ${BIN}.`);
  } catch (err) {
    fail(err);
  }
  renderWorlds(box);
}

// ---- Share: the instance as a .mrpack file for a friend

function renderShare(box) {
  const inst = insInst();
  const mods = ins.items.filter((x) => x.kind === "mod" && x.enabled).length;
  box.replaceChildren(h("div", { class: "share" },
    h("div", { class: "share-icon", "aria-hidden": "true" }, icon("share")),
    h("h3", {}, `Share ${inst.name}`),
    h("p", { class: "lede" }, `Saves this instance as a .mrpack file in your Downloads: ${versionLine(inst)}${mods ? `, with its ${mods} mod${mods === 1 ? "" : "s"}` : ""}. Send the file to a friend; in the Anti Horizon Launcher they press Import .mrpack on the Play page (the Modrinth app opens it too) and get the same game.`),
    h("label", { class: "check" }, h("input", { type: "checkbox", checked: ins.exportConfig, onchange: (ev) => (ins.exportConfig = ev.target.checked) }),
      h("span", {}, "Include settings: the config folder, video options and server list")),
    h("button", { class: "btn btn-gold btn-lg", disabled: ins.exporting, onclick: () => exportPack(box) }, icon("download"), ins.exporting ? "Saving" : "Export .mrpack"),
    h("p", { class: "share-note" }, "Worlds aren't included. Mods from Modrinth are listed by their download link; anything else goes inside the file.")));
}

async function exportPack(box) {
  ins.exporting = true;
  renderShare(box);
  try {
    const path = await invoke("export_mrpack", { instance: ins.id, withConfig: ins.exportConfig });
    toast(`Saved ${path.split(/[\\/]/).pop()} in Downloads.`, "good");
  } catch (err) {
    fail(err);
  }
  ins.exporting = false;
  if (ins.tab === "share") renderShare(box);
}

async function importPack(ev) {
  const file = ev.target.files[0];
  ev.target.value = "";
  if (!file) return;
  toast(`Setting up ${file.name.replace(/\.(mrpack|zip)$/, "")}...`);
  try {
    const result = await invoke("import_mrpack", new Uint8Array(await file.arrayBuffer()));
    state.selected = result.instance.id;
    remember("selected", result.instance.id);
    await refresh();
    toast(`${result.instance.name} is ready. Minecraft downloads the first time you press Play.`, "good");
  } catch (err) {
    fail(err);
  }
}

// ---------------------------------------------------------------- the Cosmetics tab

// A 3D preview on the left (drag to turn it); on the right, the capes, wings and head pieces to pick from. Nothing
// changes for other players until Save, which keeps it with the Anti-Horizon cosmetics service.
const cos = {
  catalog: null, saved: {}, look: {}, name: null, tab: "cape", busy: false, error: "", skin: "", slim: false, viewer: null,
};
const COS_TABS = [["cape", "Capes"], ["wings", "Wings"], ["head", "Hats and halos"]];
const COS_ABOUT = {
  accretion: "A black hole's disk, turning around your head",
  halo: "A glowing white ring above your head",
  angel: "White feathers that stand tall, fold and spread as you move",
  gold: "Gold feathers, jointed like a real wing. Elytra replace them while worn",
  void: "Dark feathers, gold tips, full of stars",
};
const sameLook = (a, b) => ["cape", "wings", "head"].every((k) => (a[k] || null) === (b[k] || null));

async function enterCosmetics() {
  Object.assign(cos, { busy: true, error: "" });
  renderCosmetics();
  const [catalog, mine, profile] = await Promise.allSettled([
    cos.catalog ? Promise.resolve(cos.catalog) : fetch("cosmetics/catalog.json").then((r) => r.json()),
    invoke("cosmetics_get"),
    state.accounts.some((a) => a.selected) ? invoke("get_profile") : Promise.reject(new Error("no account")),
  ]);
  cos.catalog = catalog.status === "fulfilled" ? catalog.value : { cape: [], wings: [], head: [] };
  if (mine.status === "fulfilled") {
    cos.name = mine.value.name;
    cos.saved = { ...mine.value.look };
    cos.look = { ...mine.value.look };
  } else {
    cos.name = state.accounts.find((a) => a.selected)?.name || null;
    cos.error = String(mine.reason);
  }
  if (profile.status === "fulfilled") {
    cos.skin = profile.value.skin;
    cos.slim = profile.value.variant === "slim";
  } else {
    cos.skin = Preview3D.mannequin();
    cos.slim = false;
  }
  cos.busy = false;
  renderCosmetics();
}

function cosPick(kind, id) {
  cos.look = { ...cos.look, [kind]: id || null };
  cos.viewer?.set(cos.skin, cos.slim, cos.look);
  renderCosmetics(true);
}

async function saveCosmetics() {
  if (!cos.name) return addAccount();
  cos.busy = true;
  cos.error = "";
  renderCosmetics(true);
  try {
    cos.saved = { ...(await invoke("cosmetics_set", { look: { cape: cos.look.cape || null, wings: cos.look.wings || null, head: cos.look.head || null } })) };
    cos.look = { ...cos.saved };
    toast("Saved. Everyone with the Anti-Horizon client sees them on you now.");
  } catch (err) {
    cos.error = String(err);
  }
  cos.busy = false;
  renderCosmetics(true);
}

/** `keepStage` leaves the 3D preview running instead of making a new one. */
function renderCosmetics(keepStage) {
  const body = $("#cos-body");
  if (!cos.catalog) return body.replaceChildren(h("p", { class: "wait" }, h("span", { class: "spinner", "aria-hidden": "true" }), "Getting your cosmetics"));

  let stage = body.querySelector(".cos-stage");
  if (!keepStage || !stage) {
    const canvas = h("canvas", { class: "cos-preview", width: "560", height: "880", "aria-label": "Preview: drag to turn" });
    stage = h("div", { class: "skin-stage cos-stage" }, canvas, h("span", { class: "cos-hint" }, "Drag to turn"));
    try {
      cos.viewer = new Preview3D.Viewer(canvas, {
        flows: Object.fromEntries(cos.catalog.cape.filter((c) => c.flow).map((c) => [c.id, c.flow])),
        clips: Object.fromEntries(cos.catalog.cape.filter((c) => c.clip).map((c) => [c.id, c.clip])),
        video: clipUrl,
      });
      cos.viewer.set(cos.skin, cos.slim, cos.look);
      cos.viewer.run();
    } catch {
      stage.replaceChildren(empty("The 3D preview needs WebGL, which this PC's graphics driver turned off."));
    }
  }
  const changed = !sameLook(cos.look, cos.saved);
  const wearing = ["cape", "wings", "head"].map((k) => cos.catalog[k].find((c) => c.id === cos.look[k])?.name).filter(Boolean);
  const left = h("div", { class: "skin-left" },
    stage,
    h("p", { class: "skin-who" }, cos.name ? h("span", {}, cos.name, h("small", {}, changed ? "Not saved yet" : "Wearing now")) : h("span", {}, "Preview", h("small", {}, "Add an account to wear them"))),
    h("p", { class: "cos-wearing" }, wearing.length ? wearing.join(" · ") : "Nothing on"),
    cos.error ? h("p", { class: "form-error", role: "alert" }, cos.error) : null,
    cos.name
      ? h("button", { class: "btn btn-gold btn-lg", disabled: !changed || cos.busy, onclick: saveCosmetics }, icon("check"), cos.busy ? "Saving" : "Save cosmetics")
      : h("button", { class: "btn btn-gold btn-lg", onclick: addAccount }, icon("plus"), "Add account"),
    changed && !cos.busy ? h("button", { class: "btn-text", onclick: () => { cos.look = { ...cos.saved }; cos.viewer?.set(cos.skin, cos.slim, cos.look); renderCosmetics(true); } }, "Undo changes") : null);

  const tabs = h("nav", { class: "glass itabs skin-tabs", "aria-label": "Cosmetics" },
    ...COS_TABS.map(([k, t]) => h("button", { type: "button", "aria-current": String(cos.tab === k), onclick: () => { cos.tab = k; renderCosmetics(true); } }, t)));
  const items = cos.catalog[cos.tab] || [];
  const card = (id, name, about, picture) => h("div", { class: (cos.look[cos.tab] || null) === id ? "cos-card is-picked" : "cos-card" },
    h("button", { class: "cos-pick", type: "button", "aria-pressed": String((cos.look[cos.tab] || null) === id), onclick: () => cosPick(cos.tab, id) },
      h("span", { class: `cos-pic cos-pic-${cos.tab}` }, picture, cos.catalog[cos.tab]?.find((c) => c.id === id)?.clip ? h("span", { class: "live-tag" }, "Live") : null),
      h("strong", {}, name),
      about ? h("small", {}, about) : null));
  const picture = (c) => cos.tab === "cape" ? h("img", { src: `cosmetics/cape/${c.id}-face-far.png`, alt: "" })
    : cos.tab === "wings" ? h("img", { src: `cosmetics/thumb/wings-${c.id}.png`, alt: "" })
    : h("img", { src: `cosmetics/thumb/head-${c.id}.png`, alt: "" });
  const grid = h("div", { class: `cos-grid cos-grid-${cos.tab}` },
    card(null, "None", "", h("span", { class: "cos-none", "aria-hidden": "true" }, icon("close"))),
    ...items.map((c) => card(c.id, c.name, COS_ABOUT[c.id] || "", picture(c))));
  const note = h("p", { class: "cos-note" }, "Free. Seen by everyone who plays with the Anti-Horizon client, on any server.");
  body.replaceChildren(left, h("div", { class: "skin-right" }, tabs, h("div", { class: "skin-pane" }, grid), note));
}

// ---------------------------------------------------------------- the Skins tab

// The preview on the left shows whatever is picked; the right picks it: Minecraft's default skins, the player's skin
// packs, or a PNG of their own. Nothing changes on the account until Save.
const skin = {
  profile: null, url: "", bytes: null, variant: "classic", busy: false, error: "",
  tab: "default", defaults: null, packs: [], picked: "", fromUpload: false, keep: true,
};

const ascii = (s) => s.replace(/[^\x20-\x7E]/g, "_");

function openSkin() {
  closeAcctMenu();
  show("skins");
}

/** Loads the selected account's skin and the library each time the tab opens (it may have changed elsewhere). */
async function enterSkins() {
  if (!state.accounts.some((a) => a.selected)) {
    skin.profile = null;
    return $("#skin-body").replaceChildren(empty("Sign in first: skins belong to your Microsoft account.", "Add account", addAccount));
  }
  Object.assign(skin, { profile: null, url: "", bytes: null, busy: true, error: "", picked: "", fromUpload: false });
  renderSkin();
  const [p, defaults, packs] = await Promise.allSettled([
    invoke("get_profile"),
    skin.defaults ? Promise.resolve(skin.defaults) : invoke("default_skins"),
    invoke("skin_packs"),
  ]);
  if (p.status === "fulfilled") {
    skin.profile = p.value;
    skin.url = p.value.skin;
    skin.variant = p.value.variant;
  } else {
    skin.error = String(p.reason);
  }
  skin.defaults = defaults.status === "fulfilled" ? defaults.value : [];
  skin.packs = packs.status === "fulfilled" ? packs.value : [];
  skin.busy = false;
  renderSkin();
}

function renderSkin() {
  const body = $("#skin-body");
  if (skin.error && !skin.profile) return body.replaceChildren(empty(skin.error));
  if (!skin.profile) return body.replaceChildren(h("p", { class: "wait" }, h("span", { class: "spinner", "aria-hidden": "true" }), "Getting your skin"));

  // ---- left: the preview and what to do with it
  const canvas = h("canvas", { class: "skin-preview", width: "224", height: "448", "aria-label": "Skin preview" });
  if (skin.url) drawSkin(canvas, skin.url, skin.variant === "slim");
  const changed = !!skin.bytes || skin.variant !== skin.profile.variant;
  const capes = skin.profile.capes;
  const left = h("div", { class: "skin-left" },
    h("div", { class: "skin-stage" }, canvas),
    h("p", { class: "skin-who" }, skin.picked ? h("span", {}, skin.picked) : h("span", {}, skin.profile.name, h("small", {}, "Wearing now"))),
    h("div", { class: "seg seg-wide", role: "group", "aria-label": "Arms" },
      ...[["classic", "Classic arms"], ["slim", "Slim arms"]].map(([v, t]) => h("button", { type: "button", "aria-pressed": String(skin.variant === v), onclick: () => setArms(v) }, t))),
    capes.length ? h("div", { class: "cape-list", role: "group", "aria-label": "Cape" },
      h("button", { class: "chip", type: "button", "aria-pressed": String(!capes.some((c) => c.active)), disabled: skin.busy, onclick: () => wearCape("") }, "No cape"),
      ...capes.map((c) => h("button", { class: "chip", type: "button", "aria-pressed": String(c.active), disabled: skin.busy, onclick: () => wearCape(c.id) }, c.alias))) : null,
    skin.error ? h("p", { class: "form-error", role: "alert" }, skin.error) : null,
    h("button", { class: "btn btn-gold btn-lg", disabled: !changed || skin.busy, onclick: saveSkin }, icon("check"), skin.busy ? "Saving" : "Save skin"),
    h("button", { class: "btn-text", disabled: skin.busy, onclick: resetSkin }, "Back to the default skin"));

  // ---- right: where to pick a skin from
  const tabs = h("nav", { class: "glass itabs skin-tabs", "aria-label": "Skins" },
    ...[["default", "Default skins"], ["packs", "Skin packs"], ["upload", "Upload"]].map(([k, t]) =>
      h("button", { type: "button", "aria-current": String(skin.tab === k), onclick: () => { skin.tab = k; renderSkin(); } }, t)));
  let pane;
  if (skin.tab === "default") {
    const style = skin.variant === "slim" ? "slim" : "classic";
    const list = (skin.defaults || []).filter((d) => d.variant === style);
    pane = list.length
      ? h("div", { class: "skin-grid" }, ...list.map((d) => skinCard(d, () => choose(d.data, d.variant, d.name))))
      : empty("The default skins come from Minecraft itself. Press Play on any instance once and they show up here.");
  } else if (skin.tab === "packs") {
    pane = h("div", { class: "packs" },
      h("button", { class: "btn btn-line", onclick: () => $("#skin-pack-file").click() }, icon("plus"), "Import a skin pack (.zip)"),
      ...(skin.packs.length ? skin.packs.map((p) => h("section", { class: "pack" },
        h("div", { class: "pack-head" }, h("h3", {}, p.name), h("span", { class: "count" }, String(p.skins.length)),
          h("button", { class: "tool tool-bad", title: "Delete pack", "aria-label": `Delete ${p.name}`, onclick: () => deletePack(p) }, icon("trash"))),
        h("div", { class: "skin-grid" }, ...p.skins.map((s) => skinCard(s, () => choose(s.data, "", s.name), () => deleteLibSkin(p, s))))))
        : [empty("No skin packs yet. Import a .zip of skins, or upload one: uploads are kept in My skins.")]));
  } else {
    pane = h("div", { class: "upload" },
      h("button", { class: "drop", type: "button", onclick: () => $("#skin-file").click() },
        icon("download"), h("strong", {}, "Choose a skin"), h("span", {}, "A PNG, 64 by 64 pixels: the kind every skin site gives you")),
      h("label", { class: "check" }, h("input", { type: "checkbox", checked: skin.keep, onchange: (ev) => (skin.keep = ev.target.checked) }),
        h("span", {}, "Keep it in My skins, so it's one click away next time")));
  }
  body.replaceChildren(left, h("div", { class: "skin-right" }, tabs, h("div", { class: "skin-pane" }, pane)));
}

function skinCard(s, onPick, onDelete) {
  const c = h("canvas", { class: "skin-thumb", width: "64", height: "128", "aria-hidden": "true" });
  (s.variant ? Promise.resolve(s.variant === "slim") : slimOf(s.data)).then((slim) => drawSkin(c, s.data, slim));
  return h("div", { class: skin.url === s.data ? "skin-card is-picked" : "skin-card" },
    h("button", { class: "skin-pick", type: "button", title: s.name, onclick: onPick }, c, h("span", {}, s.name)),
    onDelete ? h("button", { class: "skin-x", type: "button", title: "Remove", "aria-label": `Remove ${s.name}`, onclick: onDelete }, icon("close", "i-sm")) : null);
}

async function choose(dataUrl, variant, name, fromUpload = false) {
  skin.url = dataUrl;
  skin.bytes = dataUrlBytes(dataUrl);
  skin.picked = name;
  skin.fromUpload = fromUpload;
  skin.variant = variant || ((await slimOf(dataUrl)) ? "slim" : "classic");
  skin.error = "";
  renderSkin();
}

/** Switching arms on a default skin switches to that character's other version. */
function setArms(v) {
  skin.variant = v;
  const d = (skin.defaults || []).find((x) => x.data === skin.url);
  const twin = d && (skin.defaults || []).find((x) => x.name === d.name && x.variant === v);
  if (twin) return choose(twin.data, twin.variant, twin.name);
  renderSkin();
}

/** A skin with slim arms leaves the outer column of its right arm empty. */
async function slimOf(url) {
  try {
    const img = await loadImage(url);
    if (img.height !== 64) return false;
    const c = document.createElement("canvas");
    c.width = c.height = 64;
    const g = c.getContext("2d");
    g.drawImage(img, 0, 0);
    return g.getImageData(54, 20, 1, 1).data[3] === 0;
  } catch {
    return false;
  }
}

async function pickSkin(ev) {
  const file = ev.target.files[0];
  ev.target.value = "";
  if (!file) return;
  const url = await new Promise((resolve) => { const r = new FileReader(); r.onload = () => resolve(r.result); r.readAsDataURL(file); });
  await choose(url, "", file.name.replace(/\.png$/i, ""), true);
}

function dataUrlBytes(url) {
  const bin = atob(url.split(",")[1] || "");
  return Uint8Array.from(bin, (c) => c.charCodeAt(0));
}

async function saveSkin() {
  const bytes = skin.bytes || (skin.url.startsWith("data:") ? dataUrlBytes(skin.url) : null);
  if (!bytes) return;
  skin.busy = true;
  skin.error = "";
  renderSkin();
  try {
    const keep = skin.fromUpload && skin.keep ? { pack: "My skins", name: skin.picked || "Skin" } : null;
    skin.profile = await invoke("upload_skin", bytes, { headers: { "x-variant": skin.variant } });
    if (keep) skin.packs = await invoke("add_to_library", bytes, { headers: { "x-pack": keep.pack, "x-name": ascii(keep.name) } }).catch(() => skin.packs);
    skin.url = skin.profile.skin;
    skin.variant = skin.profile.variant;
    Object.assign(skin, { bytes: null, picked: "", fromUpload: false });
    state.face = skin.url ? await faceOf(skin.url) : "";
    renderWho();
    toast("Skin saved. It shows in game the next time you join a world or server.", "good");
  } catch (err) {
    skin.error = String(err);
  }
  skin.busy = false;
  renderSkin();
}

async function resetSkin() {
  if (!(await ask("Go back to the default skin?", "Your current skin is replaced by one of Minecraft's default skins.", "Reset"))) return;
  skin.busy = true;
  renderSkin();
  try {
    skin.profile = await invoke("reset_skin");
    skin.url = skin.profile.skin;
    skin.variant = skin.profile.variant;
    Object.assign(skin, { bytes: null, picked: "", fromUpload: false });
    state.face = skin.url ? await faceOf(skin.url) : "";
    renderWho();
  } catch (err) {
    skin.error = String(err);
  }
  skin.busy = false;
  renderSkin();
}

async function wearCape(id) {
  skin.busy = true;
  renderSkin();
  try {
    skin.profile = await invoke("set_cape", { id });
  } catch (err) {
    skin.error = String(err);
  }
  skin.busy = false;
  renderSkin();
}

async function importSkinPack(ev) {
  const file = ev.target.files[0];
  ev.target.value = "";
  if (!file) return;
  try {
    skin.packs = await invoke("import_skin_pack", new Uint8Array(await file.arrayBuffer()), { headers: { "x-name": ascii(file.name.replace(/\.zip$/i, "")) } });
    toast(`Added ${file.name.replace(/\.zip$/i, "")} to your skin packs.`, "good");
  } catch (err) {
    fail(err);
  }
  renderSkin();
}

async function deleteLibSkin(p, s) {
  try {
    skin.packs = await invoke("delete_library_skin", { pack: p.name, id: s.id });
  } catch (err) {
    fail(err);
  }
  renderSkin();
}

async function deletePack(p) {
  if (!(await ask(`Delete ${p.name}?`, `The pack's skins go to the ${BIN}. Skins you're wearing stay on your account.`, "Delete"))) return;
  try {
    skin.packs = await invoke("delete_skin_pack", { pack: p.name });
  } catch (err) {
    fail(err);
  }
  renderSkin();
}

function loadImage(url) {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = reject;
    img.src = url;
  });
}

/** The front of a skin, drawn flat: head, body, arms and legs, each with its outer layer. */
async function drawSkin(canvas, url, slim) {
  let img;
  try {
    img = await loadImage(url);
  } catch {
    return;
  }
  const g = canvas.getContext("2d");
  g.imageSmoothingEnabled = false;
  g.clearRect(0, 0, canvas.width, canvas.height);
  const k = canvas.width / 16;
  const old = img.height === 32;
  const arm = slim ? 3 : 4;
  const part = (sx, sy, w, ht, dx, dy, flip) => {
    g.save();
    if (flip) {
      g.translate((dx + w) * k, 0);
      g.scale(-1, 1);
      g.drawImage(img, sx, sy, w, ht, 0, dy * k, w * k, ht * k);
    } else {
      g.drawImage(img, sx, sy, w, ht, dx * k, dy * k, w * k, ht * k);
    }
    g.restore();
  };
  // [base x, base y, outer layer x, outer layer y (null on old 64x32 skins), width, height, where, mirrored from the right side]
  const pieces = [
    [4, 20, 4, 36, 4, 12, [4, 20]],
    old ? [4, 20, null, null, 4, 12, [8, 20], true] : [20, 52, 4, 52, 4, 12, [8, 20]],
    [20, 20, 20, 36, 8, 12, [4, 8]],
    [44, 20, 44, 36, arm, 12, [4 - arm, 8]],
    old ? [44, 20, null, null, arm, 12, [12, 8], true] : [36, 52, 52, 52, arm, 12, [12, 8]],
    [8, 8, null, null, 8, 8, [4, 0]],
  ];
  for (const [bx, by, ox, oy, w, ht, [dx, dy], flip] of pieces) {
    part(bx, by, w, ht, dx, dy, flip);
    if (ox !== null && !old) part(ox, oy, w, ht, dx, dy, flip);
  }
  part(40, 8, 8, 8, 4, 0);
}

/** The head of a skin (face and hat), as a small picture for the account button. */
async function faceOf(url) {
  try {
    const img = await loadImage(url);
    const c = document.createElement("canvas");
    c.width = c.height = 64;
    const g = c.getContext("2d");
    g.imageSmoothingEnabled = false;
    g.drawImage(img, 8, 8, 8, 8, 0, 0, 64, 64);
    g.drawImage(img, 40, 8, 8, 8, 0, 0, 64, 64);
    return c.toDataURL();
  } catch {
    return "";
  }
}
