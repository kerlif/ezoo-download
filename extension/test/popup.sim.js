// Runs popup.js with a fake DOM and a fake chrome API, and checks the
// media rows: name + size, no URL in any row, Preview (new tab) and
// Copy link buttons next to Download.
// node extension/test/popup.sim.js
const fs = require("fs"), path = require("path"), vm = require("vm"), assert = require("assert");
const dir = path.join(__dirname, "..");

class FakeElement {
  constructor(tag) {
    this.tagName = String(tag).toUpperCase();
    this.children = [];
    this.parentNode = null;
    this.attributes = {};
    this._text = "";
    this._class = "";
    this.checked = false;
    this.value = "";
    this.disabled = false;
    this._handlers = {};
  }
  get className() { return this._class; }
  set className(v) { this._class = v; }
  get classList() {
    const el = this;
    const add = (c) => { if (!el._class.split(" ").includes(c)) el._class = (el._class + " " + c).trim(); };
    const remove = (c) => { el._class = el._class.split(" ").filter((x) => x !== c).join(" "); };
    return { add, remove, toggle: (c, f) => { const has = el._class.split(" ").includes(c); if (f === undefined) f = !has; if (f) add(c); else remove(c); }, contains: (c) => el._class.split(" ").includes(c) };
  }
  get textContent() { return this._text + this.children.map((c) => c.textContent).join(""); }
  set textContent(v) { this._text = String(v); this.children = []; }
  setAttribute(k, v) { this.attributes[k] = String(v); }
  getAttribute(k) { return this.attributes[k] ?? null; }
  append(...kids) { for (const k of kids) this.appendChild(k); return this; }
  appendChild(k) { k.parentNode = this; this.children.push(k); return k; }
  replaceChildren(...kids) { this.children = []; this._text = ""; this.append(...kids); }
  replaceWith(...kids) {
    if (!this.parentNode) return;
    const i = this.parentNode.children.indexOf(this);
    this.parentNode.children.splice(i, 1, ...kids);
    for (const k of kids) k.parentNode = this.parentNode;
    this.parentNode = null;
  }
  addEventListener(type, fn) { (this._handlers[type] ||= []).push(fn); }
  click() {
    const ev = { stopPropagation() {}, preventDefault() {} };
    for (const fn of this._handlers.click || []) fn(ev);
  }
  querySelectorAll(sel) {
    const out = [];
    const match = (c) => (sel.startsWith(".") ? c._class.split(" ").includes(sel.slice(1)) : c.tagName.toLowerCase() === sel.toLowerCase());
    const walk = (el) => { for (const c of el.children) { if (match(c)) out.push(c); walk(c); } };
    walk(this);
    return out;
  }
  select() {}
}

const ids = ["settings", "enabled", "onText", "onHint", "look", "solid", "solidText", "fullscreen", "scale", "scaleText", "scaleReset", "status", "media"];
const byId = {};
for (const id of ids) byId[id] = new FakeElement(id === "media" ? "div" : "span");
byId.settings = new FakeElement("button");
byId.enabled = new FakeElement("input");
byId.look = new FakeElement("select");
byId.solid = new FakeElement("input");
byId.fullscreen = new FakeElement("input");
byId.scale = new FakeElement("input");
byId.scaleReset = new FakeElement("button");
byId.status = new FakeElement("span");
const bodyEl = new FakeElement("body");
const sectionEl = new FakeElement("section");
bodyEl.appendChild(sectionEl);

const openedTabs = []; const copied = []; const sentToBg = [];
const items = [
  { label: "1080p · MP4", sub: "94 MB", yt: { quality: "1080p", tag: "Full HD", ext: "mp4", duration: 300, kbps: 3800, size: 94e6, videoUrl: "https://gv/137", audioUrl: "https://gv/140", key: "1080p|MP4" } },
  { label: "Audio · MP3 192k", sub: "", yt: { quality: "mp3", ext: "mp3", duration: 300, kbps: 192, size: 7200000, videoUrl: "https://gv/140", key: "audio|MP3" } },
  { label: "720p · MP4 · stream", sub: "", url: "https://cdn.example/pl/720/index.m3u8", ext: "ts", quality: "720p", qualityKey: "720p" },
  { label: "Video stream (DASH) · choose a quality", sub: "", resolve: "https://cdn.example/v/manifest.mpd", ext: "mp4" },
  { label: "Getting video qualities…", sub: "This takes a few seconds", disabled: true },
];
const resolved = [
  { label: "1080p · MP4", sub: "94 MB", yt: { quality: "1080p", ext: "mp4", duration: 300, size: 94e6, videoUrl: "https://gv/137", key: "1080p|MP4" } },
  { label: "720p · MP4", sub: "45 MB", yt: { quality: "720p", ext: "mp4", duration: 300, size: 45e6, videoUrl: "https://gv/136", key: "720p|MP4" } },
];

const chrome = {
  storage: { local: { get: (d, cb) => { const o = { ...(typeof d === "object" ? d : {}) }; if (cb) cb(o); return Promise.resolve(o); }, set: async () => {} } },
  runtime: {
    openOptionsPage: () => {},
    sendMessage: (msg, cb) => {
      sentToBg.push(msg);
      let res;
      if (msg.type === "swiftget-status") res = { ok: true };
      else if (msg.type === "swiftget-get-media") res = items;
      else if (msg.type === "swiftget-resolve-video") res = { items: resolved };
      else if (msg.type === "swiftget-download") res = { ok: true };
      if (cb) cb(res);
    },
  },
  tabs: {
    query: async () => [{ id: 7, url: "https://www.youtube.com/watch?v=AAAAAAAAAAA", title: "Cool video - YouTube" }],
    create: async (o) => { openedTabs.push(o); },
  },
};
const document = {
  body: bodyEl,
  createElement: (tag) => new FakeElement(tag),
  getElementById: (id) => byId[id] || null,
  querySelector: (sel) => (sel === "section" ? sectionEl : null),
  execCommand: () => true,
};
const ctx = {
  document, chrome, console,
  navigator: { clipboard: { writeText: async (t) => { copied.push(t); } } },
  window: { close: () => {} },
  setTimeout, clearTimeout,
};
vm.createContext(ctx);
vm.runInContext(fs.readFileSync(path.join(dir, "popup.js"), "utf8"), ctx);

const tick = () => new Promise((r) => setTimeout(r, 10));

(async () => {
  await tick(); await tick();
  const box = byId.media;
  const rows = box.children.filter((c) => c._class.split(" ").includes("media"));
  assert.strictEqual(rows.length, 5, "5 media rows");
  for (const row of rows) assert(!/https?:\/\//.test(row.textContent), "no row shows a URL: " + row.textContent);

  // row 1: name + the app's own size, Preview + Copy link + Download
  assert(rows[0].textContent.includes("1080p · MP4"), "row 1 name");
  assert(rows[0].textContent.includes("94 MB"), "row 1 size (the app's number)");
  const btns = (row) => row.querySelectorAll("button").map((b) => b.textContent);
  assert.deepStrictEqual(btns(rows[0]), ["Preview", "Copy link", "Download"]);
  // row 2: MP3 size is computed, so it is marked estimated
  assert(rows[1].textContent.includes("6.9 MB"), "row 2 size");
  assert(rows[1].textContent.includes("estimated"), "row 2 estimated mark");
  // row 3: stream, size unknown, still Preview + Copy link
  assert.deepStrictEqual(btns(rows[2]), ["Preview", "Copy link", "Download"]);
  // row 4: "choose a quality" — no Preview, only Copy link + Download
  assert.deepStrictEqual(btns(rows[3]), ["Copy link", "Download"]);
  // row 5: disabled — no buttons at all
  assert.deepStrictEqual(btns(rows[4]), []);
  assert.strictEqual(rows[4].querySelectorAll("button").length, 0);

  // Preview opens a new tab; Copy link copies the address
  rows[0].querySelectorAll("button")[0].click();
  await tick();
  assert.strictEqual(openedTabs.length, 1);
  assert.strictEqual(openedTabs[0].url, "https://gv/137");
  rows[0].querySelectorAll("button")[1].click();
  await tick();
  assert.deepStrictEqual(copied, ["https://gv/137"]);
  assert.strictEqual(rows[0].querySelectorAll("button")[1].textContent, "Copied ✔");

  // Download on a "choose a quality" row: the qualities replace it in place
  rows[3].querySelectorAll("button")[1].click();
  await tick();
  const after = box.children.filter((c) => c._class.split(" ").includes("media"));
  assert.strictEqual(after.length, 6, "the stream row is replaced by its qualities");
  assert(after[3].textContent.includes("1080p · MP4"));
  assert(after[4].textContent.includes("720p · MP4"));
  assert.deepStrictEqual(btns(after[3]), ["Preview", "Copy link", "Download"]);

  // Download sends the item to the app
  const before = sentToBg.filter((m) => m.type === "swiftget-download").length;
  after[3].querySelectorAll("button")[2].click();
  await tick();
  assert.strictEqual(sentToBg.filter((m) => m.type === "swiftget-download").length, before + 1);
  assert.strictEqual(after[3].querySelectorAll("button")[2].textContent, "Sent ✔");

  console.log("popup simulation: rows, Preview and Copy link all check out");
})().catch((e) => { console.error(e); process.exit(1); });
