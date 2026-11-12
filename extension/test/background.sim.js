// Runs background.js with a fake `chrome` API and a fake SwiftGet app.
// node extension/test/background.sim.js
const fs = require("fs"), path = require("path"), vm = require("vm"), assert = require("assert");
const dir = path.join(__dirname, "..");
const listeners = {}; const on = (n) => ({ addListener: (f) => (listeners[n] = f) });
const session = {}; const localStore = {}; let reloads = 0; let pingExt = null; const sent = []; const sentHeaders = []; const tabMsgs = []; let tabUrl = "https://www.youtube.com/watch?v=AAAAAAAAAAA";
const chrome = {
  storage: {
    onChanged: on("store"),
    local: { get: async (d) => (typeof d === "string" ? (d in localStore ? { [d]: localStore[d] } : {}) : { ...d, ...localStore }), set: async (o) => Object.assign(localStore, o) },
    session: {
      get: async (k) => { const ks = Array.isArray(k) ? k : [k]; const o = {}; for (const x of ks) if (x in session) o[x] = JSON.parse(JSON.stringify(session[x])); return o; },
      set: async (o) => Object.assign(session, JSON.parse(JSON.stringify(o))),
      remove: async (k) => { for (const x of [].concat(k)) delete session[x]; },
    },
  },
  downloads: { onCreated: on("dl"), onDeterminingFilename: on("det"), cancel: async (id) => { cancelled.push(id); }, erase: async () => {}, download: (o) => browserDl.push(o.url) },
  webRequest: { onHeadersReceived: on("headers"), onBeforeRequest: on("before"), onSendHeaders: on("sendHeaders") },
  tabs: { onUpdated: on("upd"), onRemoved: on("rm"), get: async (id) => ({ id, url: tabUrl }), query: async () => queryTabs,
          sendMessage: async (id, m) => tabMsgs.push(m) },
  action: { setBadgeBackgroundColor: () => {}, setBadgeText: async () => {} },
  runtime: { onInstalled: on("inst"), onStartup: on("start"), onMessage: on("msg"), getManifest: () => ({ version: "1.0.4" }), reload: () => { reloads++; } },
  contextMenus: { removeAll: () => {}, create: () => {}, onClicked: on("ctx") },
  cookies: { getAll: async () => [] },
};
const ytCalls = []; const browserDl = []; const cancelled = []; const suggested = []; let appDown = false;
const sentMeta = []; // { method, body } per POST in `sent`, to verify its signature
const checkCalls = []; const checkHeaders = []; const checkMeta = []; let checkOk = true; let checkFails = false; let queryTabs = [{ id: 7 }];
let configResponse = { localToken: "test-token" }; let configFails = false;
const fetch = async (url, opts = {}) => {
  if (url.endsWith("/ping") && pingExt) return { ok: true, status: 200, json: async () => ({ ext: pingExt }) };
  // the token bootstrap: the app hands the add-in its per-install secret here
  if (url.endsWith("/config")) {
    if (configFails) throw new TypeError("Failed to fetch");
    return { ok: true, status: 200, json: async () => configResponse };
  }
  if (url.endsWith("/check")) {
    checkCalls.push(JSON.parse(opts.body)); checkHeaders.push(opts.headers || {}); checkMeta.push({ method: opts.method || "POST", body: opts.body || "" });
    if (checkFails) throw new TypeError("Failed to fetch");
    return { ok: true, status: 200, json: async () => ({ ok: checkOk, kind: "file", fileName: "f.bin", size: 1234 }) };
  }
  if (url.endsWith("/add")) { if (appDown) throw new TypeError("Failed to fetch"); sent.push(JSON.parse(opts.body)); sentHeaders.push(opts.headers || {}); sentMeta.push({ method: opts.method || "POST", body: opts.body || "" }); }
  if (url.endsWith("/resolve") && JSON.parse(opts.body).url.includes("unsupported.example")) {
    return { ok: true, status: 200, json: async () => ({ ok: false, error: "Unsupported URL: https://unsupported.example/watch" }) };
  }
  if (url.endsWith("/resolve")) {
    ytCalls.push(JSON.parse(opts.body));
    await new Promise((r) => setTimeout(r, 30)); // yt-dlp takes a moment
    return { ok: true, status: 200, json: async () => ({ ok: true, title: "Real: Title", options: [
      { key: "1080p|MP4", label: "1080p · MP4", sub: "94 MB", quality: "1080p", videoUrl: "https://gv/137", audioUrl: "https://gv/140", ext: "mp4", size: 94e6, headers: [["User-Agent", "YtDlpUA"]] },
      { key: "audio|M4A", label: "Audio only · M4A 130k", sub: "4 MB", quality: "audio", videoUrl: "https://gv/140", audioUrl: null, ext: "m4a", size: 4e6, headers: [] } ] }) };
  }
  if (url.includes("/pl/master")) {
    return { ok: true, status: 200, text: async () => "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360\n360/index.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=5000000,RESOLUTION=1920x1080\n1080/index.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=2500000,RESOLUTION=1280x720\nhttps://cdn2.example/720/index.m3u8\n" };
  }
  if (url.includes("/pl/") && url.endsWith("index.m3u8")) {
    return { ok: true, status: 200, text: async () => "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXTINF:6,\na.ts\n" };
  }
  return { ok: true, status: 200 };
};
const ctx = { chrome, fetch, crypto: require("crypto").webcrypto, TextEncoder: require("util").TextEncoder, AbortController, setTimeout, clearTimeout, navigator: { userAgent: "TestUA" }, console, URL,
  importScripts: (...f) => f.forEach((x) => vm.runInContext(fs.readFileSync(path.join(dir, x), "utf8"), ctx)) };
vm.createContext(ctx);
// a clock the test can move: the 60 s config cache is driven by it
vm.runInContext("var __now = Date.now(); Date.now = () => __now; globalThis.__advanceNow = (ms) => { __now += ms; }; globalThis.__nowMs = () => __now;", ctx);
const advance = (ms) => ctx.__advanceNow(ms);
vm.runInContext(fs.readFileSync(path.join(dir, "background.js"), "utf8"), ctx);

const gv = (itag, mime, clen, dur, r) => `https://rr1---sn-x.googlevideo.com/videoplayback?expire=9&itag=${itag}&mime=${encodeURIComponent(mime)}&clen=${clen}&dur=${dur}&pot=T&range=${r}&rn=1`;
const tick = () => new Promise((r) => setTimeout(r, 20));

// Q53: a POST's HMAC signature must recompute from its own method,
// path, time, nonce and body under the per-install secret — and the
// secret itself must never travel in a header.
const { createHmac } = require("crypto");
function verifySig(i) {
  const h = sentHeaders[i], m = sentMeta[i];
  assert.strictEqual(typeof h["X-Ezoo-Sig"], "string", "every POST carries a signature");
  assert.strictEqual(typeof h["X-Ezoo-Time"], "string", "every POST carries a time");
  assert.match(h["X-Ezoo-Nonce"] || "", /^[0-9a-f]{32}$/, "every POST carries a fresh nonce");
  const expect = createHmac("sha256", "test-token")
    .update(`${m.method}\n/add\n${h["X-Ezoo-Time"]}\n${h["X-Ezoo-Nonce"]}\n${m.body}`)
    .digest("hex");
  assert.strictEqual(h["X-Ezoo-Sig"], expect, "the signature matches the signed request");
  assert(!("X-SwiftGet-Token" in h), "the secret itself is never sent");
}

// Q53 again, for /check: every POST is signed, not just /add
function verifyCheckSig(i) {
  const h = checkHeaders[i], m = checkMeta[i];
  assert.strictEqual(typeof h["X-Ezoo-Sig"], "string", "every /check carries a signature");
  const expect = createHmac("sha256", "test-token")
    .update(`${m.method}\n/check\n${h["X-Ezoo-Time"]}\n${h["X-Ezoo-Nonce"]}\n${m.body}`)
    .digest("hex");
  assert.strictEqual(h["X-Ezoo-Sig"], expect, "the /check signature matches the signed request");
}

(async () => {
  // player loads an ad, then the real video in many pieces
  listeners.before({ tabId: 7, url: gv(136, "video/mp4", 5e6, "15.0", "0-1000") });
  await tick();
  for (const r of ["0-1000", "1001-2000", "2001-3000"]) {
    listeners.before({ tabId: 7, url: gv(137, "video/mp4", 150e6, "300.5", r) }); await tick();
    listeners.before({ tabId: 7, url: gv(140, "audio/mp4", 4e6, "300.5", r) }); await tick();
  }
  const last = tabMsgs[tabMsgs.length - 1];
  assert.strictEqual(last.type, "swiftget-media");
  assert.strictEqual(last.items[0].label, "1080p Full HD · MP4");
  assert.strictEqual(last.items[0].yt.tag, "Full HD");
  // Q26 plumbing: every offered quality carries its identity, so the app
  // can renew the expired link in the same quality later
  assert.strictEqual(last.items[0].yt.source, tabUrl, "the page goes with the quality");
  assert.strictEqual(last.items[0].yt.key, "1080p|MP4", "the chosen quality has a key");
  const audioOpt = last.items.find((i) => i.yt && i.yt.quality === "audio");
  assert(audioOpt && audioOpt.yt.key === "audio|M4A", "audio-only has a key too");
  assert(last.items.every((i) => !/^https?:\/\//.test(i.sub || "")), "no row shows a URL");
  console.log("button shows:", last.items.map((i) => i.label).join(" | "));

  // user clicks 1080p on the page
  const res = await new Promise((resolve) =>
    listeners.msg({ type: "swiftget-download", item: last.items[0], title: "(2) Cool video - YouTube" }, { tab: { id: 7, url: tabUrl } }, resolve));
  assert.strictEqual(res.ok, true);
  const req = sent[0];
  verifySig(0);
  assert(req.url.includes("itag=137") && !req.url.includes("range="));
  assert(req.audioUrl.includes("itag=140"));
  assert.strictEqual(req.fileName, "Cool video [1080p].mp4");
  assert.strictEqual(req.sourcePage, tabUrl, "the page is sent, so the app can renew the link");
  assert.strictEqual(req.sourceKey, "1080p|MP4", "the chosen quality is sent with the download");
  assert.strictEqual(req.userAgent, "TestUA");
  console.log("sent to app:", req.fileName, "| audio:", !!req.audioUrl);

  // navigating to another video clears the list
  tabUrl = "https://www.youtube.com/watch?v=BBBBBBBBBBB";
  await listeners.upd(7, { url: tabUrl }, { id: 7, url: tabUrl }); await tick();
  const afterNav = tabMsgs[tabMsgs.length - 1].items;
  assert(!afterNav.some((i) => i.label === "1080p · MP4" && i.yt && i.yt.videoUrl.includes("itag=137")), "old video's items gone");

  // ---- SABR video: no usable stream URL, the app (yt-dlp) gives the qualities
  listeners.before({ tabId: 7, url: "https://rr1---sn-x.googlevideo.com/videoplayback?expire=9&sabr=1&ump=1" });
  await listeners.upd(7, { url: tabUrl, status: "complete" }, { id: 7, url: tabUrl });
  await new Promise((r) => setTimeout(r, 150));
  assert.strictEqual(ytCalls.length, 1, "asked the app once");
  assert.strictEqual(ytCalls[0].url, "https://www.youtube.com/watch?v=BBBBBBBBBBB");
  assert(Array.isArray(ytCalls[0].cookieList), "sends the site's cookies as a list");
  const labels = tabMsgs.map((m) => (m.items[0] || {}).label);
  assert(labels.includes("Getting video qualities…"), "shows progress while resolving");
  const final = tabMsgs[tabMsgs.length - 1].items;
  assert.strictEqual(final[0].label, "1080p · MP4");
  const res2 = await new Promise((resolve) =>
    listeners.msg({ type: "swiftget-download", item: final[0], title: "ignored - YouTube" }, { tab: { id: 7, url: tabUrl } }, resolve));
  assert.strictEqual(res2.ok, true);
  const req2 = sent[sent.length - 1];
  assert.strictEqual(req2.fileName, "Real_ Title [1080p].mp4");
  assert.strictEqual(req2.sourcePage, "https://www.youtube.com/watch?v=BBBBBBBBBBB");
  assert.strictEqual(req2.sourceKey, "1080p|MP4");
  assert.strictEqual(req2.audioUrl, "https://gv/140");
  assert.strictEqual(req2.headers[0][1], "YtDlpUA");
  console.log("SABR video via app:", req2.fileName, "| key:", req2.sourceKey);

  // ---- any other site: a reel in an Instagram feed
  tabUrl = "https://www.instagram.com/";
  await listeners.upd(8, { url: tabUrl, status: "loading" }, { id: 8, url: tabUrl });
  tabMsgs.length = 0;
  listeners.msg({ type: "swiftget-video-seen", page: "https://www.instagram.com/reel/C1abcDEF/" }, { tab: { id: 8, url: tabUrl } }, () => {});
  await new Promise((r) => setTimeout(r, 150));
  const igCall = ytCalls[ytCalls.length - 1];
  assert.strictEqual(igCall.url, "https://www.instagram.com/reel/C1abcDEF/");
  const igItems = tabMsgs[tabMsgs.length - 1].items;
  assert.strictEqual(igItems[0].label, "1080p · MP4");
  const res3 = await new Promise((resolve) =>
    listeners.msg({ type: "swiftget-download", item: igItems[0], title: "x" }, { tab: { id: 8, url: tabUrl } }, resolve));
  assert.strictEqual(res3.ok, true);
  const req3 = sent[sent.length - 1];
  assert.strictEqual(req3.sourcePage, "https://www.instagram.com/reel/C1abcDEF/");
  assert.strictEqual(req3.referrer, "https://www.instagram.com/reel/C1abcDEF/");
  console.log("Instagram reel via app:", req3.fileName);
  // (Q53) the last POSTs are signed too, not just the first one
  verifySig(sentHeaders.length - 1);
  // browser downloads: only new ones, each once, no loop when the app fails
  const now = new Date().toISOString();
  // Chrome style: decided before "Save as" (the browser waits for suggest)
  const dl = async (o) => {
    // a fresh start each time, on the mocked clock the add-in itself
    // reads: the clock moves in the panel tests below, and a download
    // older than 30 s is a restored one, left alone
    const item = { id: Math.random(), state: "in_progress", startTime: new Date(ctx.__nowMs()).toISOString(), ...o };
    await listeners.dl(item); // nothing happens here any more
    const waits = listeners.det(item, () => suggested.push(item.id));
    assert.strictEqual(waits, true, "the browser waits for our answer");
    for (let i = 0; i < 20; i++) await tick();
    const kept = suggested.includes(item.id), taken = cancelled.includes(item.id);
    assert(kept !== taken, "answered exactly once: left to the browser or taken by the app");
  };
  let before = sent.length;
  await dl({ url: "https://example.com/old.pdf", startTime: new Date(Date.now() - 3600e3).toISOString() });
  await dl({ url: "https://example.com/done.pdf", state: "complete" });
  await dl({ url: "https://example.com/ext.pdf", byExtensionId: "other" });
  assert.strictEqual(sent.length, before, "restored / finished / extension downloads are left alone");
  await dl({ url: "https://example.com/new.pdf" });
  await dl({ url: "https://example.com/new.pdf" });
  assert.strictEqual(sent.length, before + 1, "a new download goes to the app once");
  appDown = true;
  await dl({ url: "https://example.com/fail.pdf" });
  assert.deepStrictEqual(browserDl, ["https://example.com/fail.pdf"], "given back to the browser once");
  await dl({ url: "https://example.com/fail.pdf", byExtensionId: "self" });
  await dl({ url: "https://example.com/fail.pdf" }); // same address right after: the browser keeps it
  assert.strictEqual(browserDl.length, 1, "no loop");
  appDown = false;
  console.log("browser downloads: restored ones ignored, no repeats, no loop");
  // add-in reloads itself once when the program brings a newer one
  pingExt = "1.0.5"; delete session.versionCheckAt;
  await ctx.followAppVersion(); await tick();
  assert.strictEqual(reloads, 1, "reloads for the new version");
  delete session.versionCheckAt;
  await ctx.followAppVersion(); await tick();
  assert.strictEqual(reloads, 1, "only once per version");
  pingExt = "1.0.4"; delete session.versionCheckAt;
  await ctx.followAppVersion();
  assert.strictEqual(reloads, 1, "same version: nothing");
  pingExt = null;
  console.log("self update: reloads once when the program has a newer add-in");
  // a player inside a frame (Dailymotion's "geo.dailymotion.com/player/…"):
  // the page around it is looked up, its qualities are offered
  {
    const page = "https://www.dailymotion.com/video/xac78kk";
    const n = ytCalls.length;
    const res = await new Promise((resolve) =>
      listeners.msg({ type: "swiftget-resolve-video", page: "", src: "", frame: true }, { tab: { id: 21, url: page }, frameId: 4 }, resolve));
    assert(res.items && res.items.length >= 2, "qualities of the page: " + JSON.stringify(res));
    assert.strictEqual(ytCalls[n].url, page, "the page, not the player frame");
    assert(!ytCalls.slice(n).some((c) => /geo\.dailymotion\.com/.test(c.url)));
    console.log("player in a frame: uses the page's qualities");
  }
  // the on / off switch of the add-in's window
  {
    localStore.enabled = false;
    listeners.store({ enabled: { newValue: false } }, "local");
    const before = sent.length;
    await dl({ url: "https://example.com/while-off.zip" });
    assert.strictEqual(sent.length, before, "off: the browser keeps its downloads");
    const cfg = await new Promise((r) => listeners.msg({ type: "swiftget-config" }, { tab: { id: 7, url: tabUrl } }, r));
    assert.strictEqual(cfg.enabled, false, "off: no buttons on videos");
    const n = tabMsgs.length;
    await ctx.pushToTab(7);
    assert.strictEqual(tabMsgs.slice(n).pop().items.length, 0, "off: nothing offered on the page");
    localStore.enabled = true;
    await dl({ url: "https://example.com/while-on.zip" });
    assert.strictEqual(sent.length, before + 1, "on again: back to the app");
    console.log("on / off switch: off leaves everything to the browser");
  }
  // a DASH stream (.mpd): offered, its qualities come from the app
  {
    listeners.headers({ tabId: 31, url: "https://cdn.example/v/manifest.mpd", statusCode: 200, responseHeaders: [{ name: "Content-Type", value: "application/dash+xml" }] });
    await tick(); await tick();
    const items = await new Promise((r) => listeners.msg({ type: "swiftget-get-media", tabId: 31 }, {}, r));
    const dash = items.find((i) => i.resolve);
    assert(dash && dash.resolve.endsWith(".mpd"), "DASH stream offered: " + JSON.stringify(items));
    const n = ytCalls.length;
    const res = await new Promise((r) => listeners.msg({ type: "swiftget-resolve-video", resolve: dash.resolve }, { tab: { id: 31, url: "https://site.example/watch/1" } }, r));
    assert(res.items && res.items.length, "its qualities");
    assert.strictEqual(ytCalls[n].url, dash.resolve);
    assert.strictEqual(ytCalls[n].referer, "https://site.example/watch/1", "the page goes along as referrer");
    console.log("DASH stream: offered, qualities from the app with the page as referrer");
  }
  // an HLS stream with several qualities: each one by its name (from the
  // playlist), the lists the player opened itself are not repeated
  {
    const hdr = [{ name: "Content-Type", value: "application/vnd.apple.mpegurl" }];
    listeners.headers({ tabId: 41, url: "https://cdn.example/pl/master", statusCode: 200, responseHeaders: hdr });
    await tick(); await tick(); await tick();
    listeners.headers({ tabId: 41, url: "https://cdn.example/pl/1080/index.m3u8", statusCode: 200, responseHeaders: hdr });
    for (let i = 0; i < 6; i++) await tick();
    const items = await new Promise((r) => listeners.msg({ type: "swiftget-get-media", tabId: 41 }, {}, r));
    const labels = items.map((i) => i.label);
    assert.strictEqual(JSON.stringify(labels), JSON.stringify(["1080p · MP4 · stream", "720p · MP4 · stream", "360p · MP4 · stream"]));
    assert.strictEqual(items[0].url, "https://cdn.example/pl/1080/index.m3u8");
    assert.strictEqual(items[1].url, "https://cdn2.example/720/index.m3u8");
    // Q26 plumbing: each HLS quality carries its identity, and no row shows a URL
    assert.strictEqual(items[0].qualityKey, "1080p");
    assert.strictEqual(items[1].qualityKey, "720p");
    assert(items.every((i) => !/^https?:\/\//.test(i.sub || "")), "no HLS row shows a URL");
    // the button of that player: the page is nothing the app knows -> its streams
    const res = await new Promise((r) =>
      listeners.msg({ type: "swiftget-resolve-video", page: "", src: "", frame: false }, { tab: { id: 41, url: "https://unsupported.example/watch" } }, r));
    assert(res.items && res.items.length === 3, "the streams instead of an error: " + JSON.stringify(res));
    console.log("HLS qualities:", labels.join(" | "));
    // downloading an HLS quality sends its identity with it (Q26 plumbing)
    const hlsBefore = sent.length;
    const hlsRes = await new Promise((r) =>
      listeners.msg({ type: "swiftget-download", item: items[0], title: "x" }, { tab: { id: 41, url: "https://site.example/watch" } }, r));
    assert.strictEqual(hlsRes.ok, true);
    assert.strictEqual(sent[hlsBefore].qualityKey, "1080p");
    assert.strictEqual(sent[hlsBefore].url, "https://cdn.example/pl/1080/index.m3u8");
  }
  // a plain file found on a page: name + size, no URL anywhere in the row
  {
    listeners.headers({ tabId: 51, url: "https://cdn.example/clip.mp4", statusCode: 200,
      responseHeaders: [{ name: "Content-Type", value: "video/mp4" }, { name: "Content-Length", value: "150000000" }] });
    for (let i = 0; i < 6; i++) await tick();
    const items = await new Promise((r) => listeners.msg({ type: "swiftget-get-media", tabId: 51 }, {}, r));
    assert.strictEqual(items.length, 1);
    assert.strictEqual(items[0].label, "MP4");
    assert.strictEqual(items[0].size, 150000000);
    assert.strictEqual(items[0].sub, "");
    console.log("file row: name + size, no URL");
  }
  // a file link clicked in the page: straight to the app, with the token
  {
    const before = sent.length;
    const res = await new Promise((r) =>
      listeners.msg({ type: "swiftget-link", url: "https://example.com/file.pdf" }, { tab: { id: 7, url: "https://example.com/page" } }, r));
    assert.strictEqual(res.ok, true, "click taken: " + JSON.stringify(res));
    assert.strictEqual(sent.length, before + 1, "the clicked link goes to the app once");
    assert.strictEqual(sent[sent.length - 1].url, "https://example.com/file.pdf");
    assert.strictEqual(sent[sent.length - 1].referrer, "https://example.com/page");
    assert.strictEqual(sent[sent.length - 1].clip, null, "a plain download carries no clip");
    verifySig(sentHeaders.length - 1);
    // the same link again right away: sent once, the browser's copy is dropped
    const res2 = await new Promise((r) =>
      listeners.msg({ type: "swiftget-link", url: "https://example.com/file.pdf" }, { tab: { id: 7, url: "https://example.com/page" } }, r));
    assert.strictEqual(res2.ok, true);
    assert.strictEqual(sent.length, before + 1, "a second click on the same link is not sent again");
    console.log("file-link click: to the app once, with the token");
  }
  // P4: a part cut in the page's panel travels with the download
  {
    const before = sent.length;
    const res = await new Promise((r) =>
      listeners.msg({ type: "swiftget-download", item: { url: "https://cdn.example/movie.mp4" }, title: "x", clip: { start: 80, end: 205, exact: true } }, { tab: { id: 7, url: "https://example.com/page" } }, r));
    assert.strictEqual(res.ok, true, "clip download taken: " + JSON.stringify(res));
    assert.strictEqual(sent.length, before + 1);
    assert.deepStrictEqual(sent[sent.length - 1].clip, { start: 80, end: 205, exact: true });
    assert.strictEqual(sent[sent.length - 1].url, "https://cdn.example/movie.mp4");
    console.log("clip download: part travels with the /add body");
  }
  // P4-bis: a part of a YouTube video cannot be cut yet — refuse
  // it plainly instead of silently sending the whole video
  {
    const before = sent.length;
    const res = await new Promise((r) =>
      listeners.msg({ type: "swiftget-download", item: { yt: { quality: "1080p", videoUrl: "https://gv/137", audioUrl: "https://gv/140", ext: "mp4", key: "1080p|MP4" } }, title: "x", clip: { start: 80, end: 205, exact: true } }, { tab: { id: 7, url: "https://example.com/page" } }, r));
    assert.strictEqual(res.ok, false, "a YouTube clip is refused: " + JSON.stringify(res));
    assert.match(res.error || "", /not supported/, "the refusal says why");
    assert.strictEqual(sent.length, before, "nothing is sent for the refused clip");
    console.log("YouTube clip: refused plainly, no silent whole download");
  }
  // ---- H5: the video panel honours its setting, even when the app is gone
  // the download tests above already fetched /config once, so the cache
  // holds { localToken } with no panel; the clock moves it past 60 s
  const panelCfg = async () => new Promise((r) => listeners.msg({ type: "swiftget-config" }, { tab: { id: 7, url: tabUrl } }, r));
  {
    // app unreachable + a stored off-setting: the stored setting wins
    localStore.panel = { enabled: false, skipSites: [] };
    configFails = true;
    advance(61000);
    const cfg = await panelCfg();
    assert.strictEqual(cfg.enabled, false, "app down, last-known off: no panel");
    // the app answers with the panel off: that is what the page is told,
    // and it is remembered for when the app is closed again
    configFails = false;
    configResponse = { localToken: "test-token", panel: { enabled: false, skipSites: ["example.com"] } };
    advance(61000);
    const cfg2 = await panelCfg();
    assert.strictEqual(cfg2.enabled, false, "live setting off: no panel");
    assert.deepStrictEqual(cfg2.skipSites, ["example.com"]);
    assert.deepStrictEqual(localStore.panel, { enabled: false, skipSites: ["example.com"] }, "the fetched setting is remembered");
    // the app answers with the panel on: remembered too
    configResponse = { localToken: "test-token", panel: { enabled: true, skipSites: [] } };
    advance(61000);
    const cfg3 = await panelCfg();
    assert.strictEqual(cfg3.enabled, true, "live setting on: panel");
    assert.strictEqual(localStore.panel.enabled, true, "the on-setting is remembered");
    // an old app that sends no panel at all: the last-known setting wins
    localStore.panel = { enabled: false, skipSites: [] };
    configResponse = { localToken: "test-token" };
    advance(61000);
    const cfg4 = await panelCfg();
    assert.strictEqual(cfg4.enabled, false, "no panel in the config: last-known off wins");
    // nothing ever stored and no panel anywhere: the panel defaults to on
    delete localStore.panel;
    configFails = true;
    advance(61000);
    const cfg5 = await panelCfg();
    assert.strictEqual(cfg5.enabled, true, "never configured: the panel defaults to on");
    configFails = false;
    console.log("panel setting: live config wins, last-known wins when the app is gone");
  }
  // ---- P3: /check — the app is asked before a download is taken
  {
    const n = checkCalls.length;
    const before = sent.length;
    await dl({ url: "https://example.com/check-ok.zip" });
    assert.strictEqual(sent.length, before + 1, "the app can fetch it: taken");
    assert.strictEqual(checkCalls.length, n + 1, "asked /check once");
    assert.strictEqual(checkCalls[n].url, "https://example.com/check-ok.zip");
    verifyCheckSig(n);
    // the app cannot fetch it (a sign-in page): the browser keeps it
    checkOk = false;
    const before2 = sent.length;
    await dl({ url: "https://example.com/check-no.zip" });
    assert.strictEqual(sent.length, before2, "the app cannot fetch it: left to the browser");
    assert.strictEqual(checkCalls[checkCalls.length - 1].url, "https://example.com/check-no.zip");
    checkOk = true;
    // /check itself fails: fail open, the download is taken
    checkFails = true;
    const before3 = sent.length;
    await dl({ url: "https://example.com/check-down.zip" });
    assert.strictEqual(sent.length, before3 + 1, "no answer from /check: taken anyway");
    checkFails = false;
    console.log("/check: asked once per decision, no means browser, failure means take");
  }
  // ---- P3: the browser's own request goes with the download
  {
    listeners.sendHeaders({
      url: "https://example.com/sent.zip",
      requestHeaders: [
        { name: "Cookie", value: "sess=1" },
        { name: "Referer", value: "https://example.com/page" },
        { name: "User-Agent", value: "SentUA" },
        { name: "Sec-Fetch-Site", value: "same-origin" },
        { name: "Authorization", value: "Bearer x" },
      ],
    });
    const before = sent.length;
    await dl({ url: "https://example.com/sent.zip" });
    const req = sent[sent.length - 1];
    assert.strictEqual(req.cookies, "sess=1", "the cookie the site really got");
    assert.strictEqual(req.referrer, "https://example.com/page");
    assert.strictEqual(req.userAgent, "SentUA");
    assert.deepStrictEqual(req.headers, [["Sec-Fetch-Site", "same-origin"]], "only the pass-on headers, never Authorization");
    verifySig(sentHeaders.length - 1);
    console.log("browser request: cookie + referrer + user agent + pass-on headers");
  }
  // ---- P3: a PDF open in the browser's viewer stays there
  {
    queryTabs = [{ id: 7, url: "https://example.com/view.pdf" }];
    const before = sent.length;
    await dl({ url: "https://example.com/view.pdf", mime: "application/pdf" });
    assert.strictEqual(sent.length, before, "the viewer keeps its PDF");
    queryTabs = [{ id: 7 }];
    console.log("PDF in the viewer: left to the browser");
  }
  // ---- P3: the app needs a newer add-in (minExt) — last: the config
  // keeps the newer version after this
  {
    configResponse = { localToken: "test-token", minExt: "9.9.9" };
    advance(61000);
    const r0 = reloads;
    await ctx.getConfig();
    assert.strictEqual(reloads, r0 + 1, "reloads once for the newer add-in");
    advance(61000);
    await ctx.getConfig();
    assert.strictEqual(reloads, r0 + 1, "only once per version");
    console.log("minExt handshake: reloads once for a newer add-in");
  }
  console.log("background simulation: all checks passed");
})().catch((e) => { console.error(e); process.exit(1); });
