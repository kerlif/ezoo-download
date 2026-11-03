// Runs content.js with a fake DOM and a fake chrome API, and checks the
// capture pill + item rows: counter badge, name + size, no URL in any row,
// Preview (new tab) and Copy link buttons. H5 adds the panel-setting
// scenarios: disabled at load, toggled mid-page, app unreachable.
// node extension/test/content.sim.js
const fs = require("fs"), path = require("path"), vm = require("vm"), assert = require("assert");
const dir = path.join(__dirname, "..");

// ------------------------------------------------------------ fake DOM
class FakeElement {
  constructor(tag) {
    this.tagName = String(tag).toUpperCase();
    this.children = [];
    this.parentNode = null;
    this.attributes = {};
    this.dataset = {};
    this._text = "";
    this._class = "";
    this.hidden = false;
    this.style = { setProperty: (k, v) => { this.style[k] = v; } };
    this._handlers = {};
  }
  get id() { return this.attributes.id || ""; }
  set id(v) { this.attributes.id = v; }
  get className() { return this._class; }
  set className(v) { this._class = v; this.attributes.class = v; }
  get classList() {
    const el = this;
    const add = (c) => { if (!el._class.split(" ").includes(c)) el._class = (el._class + " " + c).trim(); };
    const remove = (c) => { el._class = el._class.split(" ").filter((x) => x !== c).join(" "); };
    return {
      add,
      remove,
      toggle: (c, f) => { const has = el._class.split(" ").includes(c); if (f === undefined) f = !has; if (f) add(c); else remove(c); },
      contains: (c) => el._class.split(" ").includes(c),
    };
  }
  get textContent() {
    return this._text + this.children.map((c) => c.textContent).join("");
  }
  set textContent(v) { this._text = String(v); this.children = []; }
  setAttribute(k, v) { this.attributes[k] = String(v); if (k === "id") this.id = v; if (k === "class") this._class = v; }
  getAttribute(k) { return this.attributes[k] ?? null; }
  hasAttribute(k) { return k in this.attributes; }
  append(...kids) { for (const k of kids) this.appendChild(k); return this; }
  appendChild(k) { k.parentNode = this; this.children.push(k); return k; }
  remove() { if (this.parentNode) this.parentNode.children = this.parentNode.children.filter((c) => c !== this); this.parentNode = null; }
  replaceChildren(...kids) { this.children = []; this._text = ""; this.append(...kids); }
  addEventListener(type, fn) { (this._handlers[type] ||= []).push(fn); }
  // click with bubbling, so stopPropagation() can be checked
  click() {
    const ev = { type: "click", button: 0, _stopped: false, stopPropagation() { this._stopped = true; }, preventDefault() {} };
    let el = this;
    while (el && !ev._stopped) {
      for (const fn of el._handlers.click || []) fn(ev);
      el = el.parentNode;
    }
  }
  getBoundingClientRect() {
    return this._rect || { width: 170, height: 22, top: 0, left: 0, right: 170, bottom: 22 };
  }
  querySelector(sel) {
    const hit = this._find(sel);
    return hit;
  }
  _find(sel) {
    for (const c of this.children) {
      if (matches(c, sel)) return c;
      const d = c._find(sel);
      if (d) return d;
    }
    return null;
  }
  querySelectorAll(sel) {
    const out = [];
    const walk = (el) => { for (const c of el.children) { if (matches(c, sel)) out.push(c); walk(c); } };
    walk(this);
    return out;
  }
  getElementById(id) {
    if (this.attributes.id === id) return this;
    for (const c of this.children) { const d = c.getElementById(id); if (d) return d; }
    return null;
  }
  attachShadow() {
    const root = new FakeElement("#shadow-root");
    this.shadowRoot = root;
    return root;
  }
  select() {}
}
function matches(el, sel) {
  if (sel.startsWith("#")) return el.attributes.id === sel.slice(1);
  if (sel.startsWith(".")) return el._class.split(" ").includes(sel.slice(1));
  return el.tagName.toLowerCase() === sel.toLowerCase();
}

const items = [
  { label: "1080p · MP4", sub: "143.1 MB", yt: { quality: "1080p", tag: "Full HD", codec: "", ext: "mp4", duration: 300, kbps: 3800, size: 150000000, sizeSource: "filesize", videoUrl: "https://gv/137", audioUrl: "https://gv/140", key: "1080p|MP4" } },
  { label: "Audio only · M4A 128k", sub: "3.8 MB", yt: { quality: "audio", ext: "m4a", duration: 300, kbps: 128, size: 4000000, videoUrl: "https://gv/140", key: "audio|M4A" } },
  { label: "Audio · MP3 192k", sub: "", yt: { quality: "mp3", ext: "mp3", duration: 300, kbps: 192, size: 7200000, videoUrl: "https://gv/140", key: "audio|MP3" } },
  { label: "720p · MP4 · stream", sub: "", url: "https://cdn.example/pl/720/index.m3u8", ext: "ts", quality: "720p", qualityKey: "720p" },
  { label: "Video stream (DASH) · choose a quality", sub: "", resolve: "https://cdn.example/v/manifest.mpd", ext: "mp4" },
  { label: "Getting video qualities…", sub: "This takes a few seconds", disabled: true },
];

// One fresh page load of content.js: fresh DOM, fresh chrome fakes.
// `config` is what the background answers to swiftget-config (null = the
// app is unreachable); `storedPanel` is the last-known panel setting
// already in chrome.storage.local.
function makeHarness({ config = { enabled: true, skipSites: [] }, storedPanel = null } = {}) {
  let configResponse = config;
  const localStore = {};
  if (storedPanel) localStore.panel = storedPanel;

  const video = new FakeElement("video");
  video._rect = { width: 1280, height: 720, top: 0, left: 0, right: 1280, bottom: 720 };
  video.paused = false; video.ended = false; video.duration = 300;
  video.currentSrc = "https://example.com/v.mp4"; video.src = ""; video.isConnected = true;

  const docEl = new FakeElement("html");
  const bodyEl = new FakeElement("body");
  docEl.appendChild(bodyEl);

  const openedTabs = []; const copied = []; let clipboardFails = false;
  const sentToBg = [];
  let onMsg = null;
  const timers = [];
  const chrome = {
    storage: {
      local: {
        get: (d, cb) => { const o = { ...(typeof d === "object" ? d : {}), ...localStore }; if (cb) cb(o); return Promise.resolve(o); },
        set: async (o) => Object.assign(localStore, o),
      },
      onChanged: { addListener: () => {} },
    },
    runtime: {
      lastError: undefined,
      onMessage: { addListener: (f) => { onMsg = f; } },
      sendMessage: (msg, cb) => {
        sentToBg.push(msg);
        let res;
        if (msg.type === "swiftget-config") res = configResponse;
        else if (msg.type === "swiftget-media-request") res = items;
        else if (msg.type === "swiftget-link-rules") res = null;
        else if (msg.type === "swiftget-download") res = { ok: true };
        if (cb) cb(res);
        return { catch: () => {} };
      },
    },
  };

  const document = {
    title: "Cool video - YouTube",
    documentElement: docEl,
    body: bodyEl,
    createElement: (tag) => new FakeElement(tag),
    createElementNS: (_ns, tag) => new FakeElement(tag),
    querySelectorAll: (sel) => (sel === "video" ? [video] : []),
    addEventListener: () => {},
    execCommand: () => true,
  };
  const windowObj = {
    top: null, innerWidth: 1920, innerHeight: 1080,
    open: (url, target) => { openedTabs.push({ url, target }); return null; },
    addEventListener: () => {},
    getSelection: () => ({ rangeCount: 0 }),
  };
  windowObj.top = windowObj;
  const ctx = {
    window: windowObj, document, location: { href: "https://www.youtube.com/watch?v=AAAAAAAAAAA", hostname: "www.youtube.com", search: "?v=AAAAAAAAAAA" },
    innerWidth: 1920, innerHeight: 1080,
    navigator: { clipboard: { writeText: async (t) => { if (clipboardFails) throw new Error("denied"); copied.push(t); } }, userAgent: "TestUA" },
    chrome, console, URL, setTimeout, clearTimeout,
    setInterval: (fn, ms) => { timers.push({ fn, ms }); return timers.length; },
    addEventListener: () => {},
    getSelection: () => ({ rangeCount: 0 }),
  };
  ctx.window.window = windowObj;
  vm.createContext(ctx);
  // a clock the test can move: the 30 s panel refresh is driven by it
  vm.runInContext("var __now = Date.now(); Date.now = () => __now; globalThis.__advanceNow = (ms) => { __now += ms; };", ctx);
  vm.runInContext(fs.readFileSync(path.join(dir, "content.js"), "utf8"), ctx);

  const tick = () => new Promise((r) => setTimeout(r, 10));
  return {
    ctx, docEl, video, timers, localStore, sentToBg, openedTabs, copied, onMsg, tick,
    bubbleHosts: () => docEl.children.filter((c) => c.shadowRoot),
    setConfig: (c) => { configResponse = c; },
    setClipboardFails: (v) => { clipboardFails = v; },
    advance: (ms) => ctx.__advanceNow(ms),
    runInterval: (ms) => { for (const t of timers) if (t.ms === ms) t.fn(); },
  };
}

(async () => {
  // ---- scenario 1: the panel is on — pill, rows, Preview and Copy link
  {
    const h = makeHarness();
    const { docEl, sentToBg, openedTabs, copied, onMsg, tick } = h;
    await tick();
    // one bubble on the video
    const hosts = docEl.children.filter((c) => c.shadowRoot);
    assert.strictEqual(hosts.length, 1, "one bubble host");
    const root = hosts[0].shadowRoot;
    assert(root, "closed shadow root");

    // the new icon: gradient tile + white arrow + amber dot
    const svg = root.querySelector("svg");
    assert(svg, "icon svg present");
    const tile = svg.children.find((c) => c.tagName === "RECT");
    assert(tile && tile.getAttribute("fill") === "url(#ezg)", "brand gradient tile");
    const arrow = svg.children.find((c) => c.tagName === "PATH");
    assert(arrow && arrow.getAttribute("fill") === "#fff", "white download arrow");
    const dot = svg.children.find((c) => c.tagName === "CIRCLE");
    assert(dot && dot.getAttribute("fill") === "#F5A524", "amber accent dot");

    // the counter-badge pill: 5 usable items (one row is disabled)
    assert.strictEqual(root.getElementById("label").textContent, "Download video (5)");
    const badge = root.getElementById("count");
    assert.strictEqual(badge.hidden, false);
    assert.strictEqual(badge.textContent, "5");

    // the rows: name + size, no URL anywhere
    // (the list only shows once the bubble is opened, as for a real click)
    onMsg({ type: "swiftget-open-panel" }, {}, () => {});
    await tick();
    const list = root.getElementById("list");
    assert.strictEqual(list.hidden, false);
    const rows = list.querySelectorAll(".item");
    assert.strictEqual(rows.length, 6, "5 usable rows + 1 disabled row");
    for (const row of rows) assert(!/https?:\/\//.test(row.textContent), "no row shows a URL: " + row.textContent);

    const rowText = (row) => row.textContent;
    assert(rowText(rows[0]).includes("MP4 file · 1080p"), "row 1 name");
    assert(rowText(rows[0]).includes("143.1 MB"), "row 1 size");
    assert(rowText(rows[0]).includes("Full HD"), "row 1 tag");
    assert(rowText(rows[1]).includes("3.8 MB"), "row 2 size");
    // the MP3 size is computed, so it is marked estimated
    assert(rowText(rows[2]).includes("6.9 MB"), "row 3 size");
    assert(rowText(rows[2]).includes("estimated"), "row 3 estimated mark");
    assert(rowText(rows[3]).includes("720p"), "row 4 name");
    assert(!rowText(rows[3]).includes("MB"), "row 4 has no size to show");

    const buttons = (row) => row.querySelectorAll(".mini").map((b) => b.textContent);
    assert.deepStrictEqual(buttons(rows[0]), ["Preview", "Copy link"]);
    assert.deepStrictEqual(buttons(rows[3]), ["Preview", "Copy link"]);
    // a "choose a quality" row has nothing to preview, only a link to copy
    assert.deepStrictEqual(buttons(rows[4]), ["Copy link"]);
    // the disabled row has no buttons
    assert.deepStrictEqual(buttons(rows[5]), []);

    // Preview opens the stream in a new tab (and does not start a download)
    const beforeDl = sentToBg.filter((m) => m.type === "swiftget-download").length;
    rows[0].querySelectorAll(".mini")[0].click();
    assert.deepStrictEqual(openedTabs, [{ url: "https://gv/137", target: "_blank" }]);
    assert.strictEqual(sentToBg.filter((m) => m.type === "swiftget-download").length, beforeDl, "Preview is not a download");

    // Copy link puts the download address on the clipboard
    rows[0].querySelectorAll(".mini")[1].click();
    await tick();
    assert.deepStrictEqual(copied, ["https://gv/137"]);
    assert.strictEqual(rows[0].querySelectorAll(".mini")[1].textContent, "Copied ✔");
    rows[4].querySelectorAll(".mini")[0].click();
    await tick();
    assert.strictEqual(copied[1], "https://cdn.example/v/manifest.mpd", "a stream row copies its manifest address");

    // clipboard denied: the textarea fallback still copies
    h.setClipboardFails(true);
    copied.length = 0;
    rows[3].querySelectorAll(".mini")[1].click();
    await tick();
    assert.strictEqual(rows[3].querySelectorAll(".mini")[1].textContent, "Copied ✔", "fallback copy works");
    h.setClipboardFails(false);

    // clicking the row itself still downloads it
    rows[0].click();
    await tick();
    const dl = sentToBg.filter((m) => m.type === "swiftget-download");
    assert.strictEqual(dl.length, beforeDl + 1);
    assert.strictEqual(dl[dl.length - 1].item.yt.videoUrl, "https://gv/137");

    // one item left: the pill shows no count
    onMsg({ type: "swiftget-media", items: [items[0]] }, {}, () => {});
    await tick();
    assert.strictEqual(root.getElementById("label").textContent, "Download video");
    assert.strictEqual(root.getElementById("count").hidden, true);

    console.log("content simulation: pill, rows, Preview and Copy link all check out");
  }

  // ---- scenario 2 (H5): panel disabled at load — no bubble on any path
  {
    const h = makeHarness({ config: { enabled: false, skipSites: [] } });
    await h.tick();
    assert.strictEqual(h.bubbleHosts().length, 0, "disabled at load: no bubble yet");
    // the fetch persisted the last-known setting for when the app is closed
    assert.deepStrictEqual(h.localStore.panel, { enabled: false, skipSites: [] }, "disabled setting is remembered");
    h.onMsg({ type: "swiftget-media", items }, {}, () => {});
    await h.tick();
    assert.strictEqual(h.bubbleHosts().length, 0, "disabled: media does not show a bubble");
    h.onMsg({ type: "swiftget-open-panel" }, {}, () => {});
    await h.tick();
    assert.strictEqual(h.bubbleHosts().length, 0, "disabled: open-panel does not show a bubble");
    h.runInterval(700);
    assert.strictEqual(h.bubbleHosts().length, 0, "disabled: the place loop does not show a bubble");
    console.log("panel disabled at load: no bubble on place, update or open-panel");
  }

  // ---- scenario 3 (H5): setting toggled mid-page — hidden within the refresh
  {
    const h = makeHarness();
    await h.tick();
    assert.strictEqual(h.bubbleHosts().length, 1, "one bubble while the panel is on");
    // the user turns the panel off in Options: the next refresh hides it
    h.setConfig({ enabled: false, skipSites: [] });
    h.advance(31000);
    h.runInterval(700);
    assert.strictEqual(h.bubbleHosts().length, 0, "toggled off: the refresh hides the bubble");
    assert.strictEqual(h.localStore.panel.enabled, false, "the toggled-off setting is remembered");
    // still off: media and open-panel show nothing
    h.onMsg({ type: "swiftget-media", items }, {}, () => {});
    await h.tick();
    assert.strictEqual(h.bubbleHosts().length, 0, "off: media does not bring the bubble back");
    h.onMsg({ type: "swiftget-open-panel" }, {}, () => {});
    await h.tick();
    assert.strictEqual(h.bubbleHosts().length, 0, "off: open-panel does not bring the bubble back");
    // and turning it back on brings the bubble back on the next refresh
    h.setConfig({ enabled: true, skipSites: [] });
    h.advance(31000);
    h.runInterval(700);
    h.onMsg({ type: "swiftget-media", items }, {}, () => {});
    await h.tick();
    assert.strictEqual(h.bubbleHosts().length, 1, "on again: the bubble comes back");
    console.log("panel toggled mid-page: hidden within the refresh interval, back when on again");
  }

  // ---- scenario 4 (H5): app unreachable — the last-known setting wins
  {
    // the app is closed: the config fetch answers nothing, the stored
    // setting from the last successful fetch decides
    const h = makeHarness({ config: null, storedPanel: { enabled: false, skipSites: [] } });
    await h.tick();
    h.onMsg({ type: "swiftget-media", items }, {}, () => {});
    await h.tick();
    assert.strictEqual(h.bubbleHosts().length, 0, "app closed, last-known off: no bubble on media");
    h.onMsg({ type: "swiftget-open-panel" }, {}, () => {});
    await h.tick();
    assert.strictEqual(h.bubbleHosts().length, 0, "app closed, last-known off: no bubble on open-panel");
    console.log("app unreachable: last-known panel setting (off) wins");
    // ...and when the last-known setting is on, the panel shows
    const h2 = makeHarness({ config: null, storedPanel: { enabled: true, skipSites: [] } });
    await h2.tick();
    assert.strictEqual(h2.bubbleHosts().length, 1, "app closed, last-known on: the bubble shows");
    console.log("app unreachable: last-known panel setting (on) wins");
  }
})().catch((e) => { console.error(e); process.exit(1); });
