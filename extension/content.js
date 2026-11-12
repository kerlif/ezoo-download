// Floating "Download video" button, shown when the background worker
// detects a video/audio file or HLS stream on the page. Runs in every frame:
// videos inside frames (embedded players) and inside shadow DOM get their
// own button too.
(() => {
  if (window.__ezooDownload) return;
  window.__ezooDownload = true;
  const TOP = window.top === window;

  let items = [];
  // Options > File Types > "Download panel for web players". The last
  // setting the app sent us, kept in storage so the panel stays off when
  // the app is closed; the first /config fetch (or refresh) overwrites it.
  let panel = { enabled: true, skipSites: [] };
  try {
    chrome.storage.local.get({ panel: null }, (o) => {
      if (o && o.panel && typeof o.panel.enabled === "boolean") panel = o.panel;
    });
  } catch (_) {
    /* storage not reachable: the first /config fetch decides */
  }
  const hostMatches = (host, p) => {
    p = String(p).replace(/^https?:\/\//i, "").split("/")[0].toLowerCase();
    if (!p) return false;
    if (p.includes("*")) return new RegExp("^" + p.replace(/[.]/g, "\\.").replace(/\*/g, ".*") + "$").test(host);
    return host === p || host.endsWith("." + p);
  };
  // the on / off switch of the add-in's window
  let switchedOn = true;
  // the video button in full screen too (the add-in's window); off: hidden there
  let showInFullscreen = false;
  const panelAllowed = () =>
    switchedOn && panel.enabled !== false && !(panel.skipSites || []).some((s) => hostMatches(location.hostname.toLowerCase(), s));

  const fmtSize = (n) => {
    if (!n) return "";
    const u = ["B", "KB", "MB", "GB"];
    let i = 0;
    while (n >= 1024 && i < u.length - 1) {
      n /= 1024;
      i++;
    }
    return `${n.toFixed(i ? 1 : 0)} ${u[i]}`;
  };
  const fmtLen = (s) => {
    s = Math.round(Number(s) || 0);
    if (!s) return "";
    const two = (n) => String(n).padStart(2, "0");
    return s >= 3600 ? `${Math.floor(s / 3600)}:${two(Math.floor(s / 60) % 60)}:${two(s % 60)}` : `${Math.floor(s / 60)}:${two(s % 60)}`;
  };
  // the resolution in plain words (the app sends it; streams seen in the
  // player only have the height)
  const tagOf = (h) => (!h ? "" : h >= 4320 ? "8K" : h >= 2160 ? "4K" : h >= 1440 ? "2K" : h >= 1080 ? "Full HD" : h >= 720 ? "HD" : "SD");

  const cleanTitle = () =>
    (document.title || "video").replace(/[<>:"/\\|?*\u0000-\u001f]/g, "_").trim().slice(0, 120) || "video";

  // --------------------------------------------------------------- videos
  // Videos of the page, also those inside shadow DOM (players built as web
  // components). The deep search is slower, so it runs every few rounds.
  let shadowVideos = [];
  let deepRound = 0;
  function collectShadow(root, out, depth) {
    if (depth > 6) return;
    for (const el of root.querySelectorAll("*")) {
      const sr = el.shadowRoot;
      if (!sr) continue;
      out.push(...sr.querySelectorAll("video"));
      collectShadow(sr, out, depth + 1);
    }
  }
  function allVideos() {
    const list = [...document.querySelectorAll("video")];
    if (deepRound++ % 4 === 0) {
      const found = [];
      try {
        collectShadow(document, found, 0);
      } catch (_) {
        /* a strange page: keep what we have */
      }
      shadowVideos = found;
    }
    for (const v of shadowVideos) if (v.isConnected && !list.includes(v)) list.push(v);
    return list;
  }

  // The video the user is watching: a playing one first, else the biggest
  // one on screen.
  function biggestVideo() {
    let best = null;
    let score = 0;
    for (const v of allVideos()) {
      const r = v.getBoundingClientRect();
      if (r.width < 200 || r.height < 100) continue;
      const visible = Math.max(0, Math.min(r.bottom, innerHeight) - Math.max(r.top, 0));
      if (visible < 60) continue;
      const s = r.width * visible * (!v.paused && !v.ended ? 4 : 1);
      if (s > score) {
        best = v;
        score = s;
      }
    }
    return best;
  }
  const playing = (v) => !v.paused && !v.ended && v.duration > 3;

  // Pages that are one video: their own address is the best link for it.
  const VIDEO_PAGE = [
    /youtube\.com\/(watch|shorts\/|live\/|embed\/)/i,
    /youtube-nocookie\.com\/embed\//i,
    /tiktok\.com\/@[^/]+\/(video|photo)\//i,
    /instagram\.com\/(reels?|p|tv)\//i,
    /facebook\.com\/(.+\/videos\/|watch|reel\/|share\/[vr]\/|plugins\/video)/i,
    /fb\.watch\//i,
    /(twitter|x)\.com\/[^/]+\/status\//i,
    /vimeo\.com\/\d+/i,
    /player\.vimeo\.com\/video\//i,
    // (not the "geo.dailymotion.com/player/…" frame: that is only the player)
    /dailymotion\.com\/(video\/|embed\/video\/)/i,
    /dai\.ly\//i,
    /reddit\.com\/r\/[^/]+\/comments\//i,
    /twitch\.tv\/(videos\/|[^/]+\/clip\/)/i,
    /clips\.twitch\.tv\//i,
    /pinterest\.[a-z.]+\/pin\//i,
    /snapchat\.com\/(spotlight|add)\//i,
    /linkedin\.com\/(posts|feed\/update)\//i,
  ];
  const isVideoPage = (u) => VIDEO_PAGE.some((r) => r.test(u));
  // Facebook feeds: the post a video is in is also a link the app can read
  // (the feed's own address - facebook.com/ - is not)
  const FB_POST = [
    /facebook\.com\/[^/?#]+\/posts\//i,
    /facebook\.com\/permalink\.php\?[^#]*story_fbid=/i,
    /facebook\.com\/story\.php\?[^#]*story_fbid=/i,
    /facebook\.com\/groups\/[^/]+\/(permalink|posts)\//i,
    /facebook\.com\/photo(\.php)?\/?\?[^#]*fbid=/i,
  ];
  const onFacebook = /(^|\.)facebook\.com$/i.test(location.hostname);
  const isPostLink = (u) => onFacebook && FB_POST.some((r) => r.test(u));
  // a Facebook feed (home, a page's wall, groups...): many videos, and its
  // own address is none of them
  const inFeed = () => onFacebook && !isVideoPage(location.href) && !isPostLink(location.href);
  // what is left of a Facebook link once its tracking part is cut off
  function cleanLink(u) {
    try {
      const x = new URL(u);
      for (const k of [...x.searchParams.keys()]) if (/^(__cft__|__tn__|_fb_noscript|ref|rdid|mibextid)/i.test(k)) x.searchParams.delete(k);
      x.hash = "";
      return x.toString();
    } catch {
      return u.split("#")[0];
    }
  }

  // In feeds (Facebook, Instagram, X...) the page shows many videos: look for
  // the post / reel link around the video being watched.
  function linkFor(video) {
    if (isVideoPage(location.href) || isPostLink(location.href)) return cleanLink(location.href);
    // a Facebook feed / home page is no video: nothing rather than the feed
    return ownLink(video) || (onFacebook ? "" : location.href);
  }

  // the post / video link next to this very video ("" = none found)
  function ownLink(video) {
    let el = video;
    let post = "";
    for (let depth = 0; el && depth < 25; depth++) {
      const links = el.querySelectorAll ? el.querySelectorAll("a[href]") : [];
      for (const a of links) {
        if (isVideoPage(a.href)) return cleanLink(a.href);
        if (!post && isPostLink(a.href)) post = cleanLink(a.href);
      }
      // the end of this post (Facebook marks its posts, it has no <article>)
      if (el.tagName === "ARTICLE" || (el.getAttribute && el.getAttribute("role") === "article")) break;
      // out of a shadow root: on to the element that holds it
      el = el.parentElement || (el.parentNode && el.parentNode.host) || null;
    }
    if (post) return post;
    // an embedded player: the frame's own address is the video
    if (!TOP && isVideoPage(location.href)) return location.href;
    return "";
  }

  let lastPage = "";
  function watchVideos() {
    const v = biggestVideo();
    if (!v) return;
    // ignore tiny muted background loops that never play for long
    if (v.paused && !(v.duration > 3) && !v.currentSrc) return;
    // in a frame (an embedded player): its own address when it is a video
    // page (YouTube / Vimeo embeds), otherwise the page around it (the
    // add-in knows that one)
    const page = TOP ? linkFor(v) : isVideoPage(location.href) ? location.href : "";
    if (page !== lastPage || (!TOP && !lastPage)) {
      if (!TOP && lastPage === "frame:" + page) return;
      lastPage = TOP ? page : "frame:" + page;
      chrome.runtime.sendMessage({ type: "swiftget-video-seen", page, frame: !TOP, feed: TOP && !page && inFeed() }).catch?.(() => {});
    }
  }

  // ---------------------------------------------------------------- bubbles
  // One "Download" bubble on every video of the page. The main video (the
  // one playing / the biggest) uses what the add-in found for the page; any
  // other video is looked up when its bubble is clicked. A bubble can be
  // dragged anywhere on its video and stays there.
  const bubbles = new Map(); // video -> bubble
  function removeAll() {
    for (const b of bubbles.values()) b.host.remove();
    bubbles.clear();
  }
  // A fresh panel setting from the app: remember it for when the app is
  // closed, and hide the bubbles when the panel is off.
  function applyPanelConfig(res) {
    if (!res || typeof res.enabled !== "boolean") return;
    panel = res;
    try {
      chrome.storage.local.set({ panel: res });
    } catch (_) {
      /* storage not reachable: the setting still applies to this page */
    }
    if (!panelAllowed()) removeAll();
  }
  // size of the button: the slider in the add-in's menu (50% - 200%)
  const OLD_SIZES = { small: 0.85, normal: 1, large: 1.25 };
  // its look: the add-in's menu > "Button style"
  const STYLES = ["compact", "blue", "dark", "light", "green", "icon"];
  let look = "compact";
  const applyLook = () => {
    for (const b of bubbles.values()) {
      const w = b.root.querySelector(".wrap");
      if (w) w.className = "wrap " + look;
      placeOne(b);
    }
  };
  const clampZoom = (z) => Math.min(2, Math.max(0.5, Number(z) || 1));
  let zoom = 1;
  // how solid the button is while the mouse is not on it (the menu: 20% - 100%)
  const clampSolid = (o) => Math.min(1, Math.max(0.2, Number(o) || 0.9));
  let solid = 0.9;
  const applyZoom = () => {
    for (const b of bubbles.values()) {
      b.host.style.setProperty("--z", String(zoom));
      b.host.style.setProperty("--o", String(solid));
      placeOne(b);
    }
  };
  try {
    chrome.storage.local.get({ enabled: true }, (o) => {
      switchedOn = !o || o.enabled !== false;
      if (!switchedOn) removeAll();
    });
    chrome.storage.onChanged.addListener((ch) => {
      if (!ch.enabled) return;
      switchedOn = ch.enabled.newValue !== false;
      if (!switchedOn) removeAll();
      else chrome.runtime.sendMessage({ type: "swiftget-media-request" }, (res) => !chrome.runtime.lastError && update(res));
    });
  } catch (_) {
    // storage not reachable (add-in reloading): stays on
  }
  try {
    chrome.storage.local.get({ bubbleFullscreen: false }, (o) => (showInFullscreen = !!(o && o.bubbleFullscreen)));
    chrome.storage.onChanged.addListener((ch) => {
      if (ch.bubbleFullscreen) {
        showInFullscreen = !!ch.bubbleFullscreen.newValue;
        bubbles.forEach((b) => placeOne(b));
      }
    });
    chrome.storage.local.get({ bubbleScale: null, bubbleSize: "normal", bubbleStyle: "compact", bubbleOpacity: 0.9 }, (o) => {
      zoom = clampZoom(o && o.bubbleScale != null ? o.bubbleScale : OLD_SIZES[o && o.bubbleSize]);
      solid = clampSolid(o && o.bubbleOpacity);
      look = STYLES.includes(o && o.bubbleStyle) ? o.bubbleStyle : "compact";
      applyZoom();
      applyLook();
    });
    chrome.storage.onChanged.addListener((ch) => {
      if (ch.bubbleStyle) {
        look = STYLES.includes(ch.bubbleStyle.newValue) ? ch.bubbleStyle.newValue : "compact";
        applyLook();
        return;
      }
      if (ch.bubbleOpacity) solid = clampSolid(ch.bubbleOpacity.newValue);
      else if (ch.bubbleScale) zoom = clampZoom(ch.bubbleScale.newValue);
      else if (ch.bubbleSize) zoom = clampZoom(OLD_SIZES[ch.bubbleSize.newValue]);
      else return;
      applyZoom();
    });
  } catch (_) {
    // storage not reachable (add-in reloading): default size
  }
  // EDM palette (from the supplied logo, 2026-11-05): navy #253149 / #1D283A,
  // blue #4687C7, green #54AB64, red #C2453F, grey #B0B3B8. White text on
  // raw #4687C7 is only 3.78:1, so every fill carrying white text uses the
  // darker AA-safe variants: blue #3570A6 (5.22:1), blue hover #2F6395
  // (6.29:1), green #2E7D3F (5.10:1). Hover/selected tints are the 12%
  // blue tint #E9F1F8. All fills are flat — no gradient anywhere.
  const STYLE = `
    :host { all: initial; }
    .wrap { font: 11px/1.25 "Segoe UI", system-ui, sans-serif; color: #1f2733; display: inline-block; zoom: var(--z, 1); }
    .btn { display: inline-flex; align-items: stretch; color: #fff; border: 1px solid #24567F;
           background: #3570A6; border-radius: 4px;
           box-shadow: 0 1px 6px rgba(0,0,0,.35); font: inherit; font-weight: 600;
           opacity: var(--o, .9); transition: opacity .15s; user-select: none; white-space: nowrap; }
    .btn:hover, .btn.open { opacity: 1; background: #2F6395; }
    .btn.drag { opacity: 1; }
    .part { display: inline-flex; align-items: center; gap: 5px; padding: 3px 7px; cursor: grab; }
    .part:hover { background: rgba(255,255,255,.12); }
    .btn.drag .part { cursor: grabbing; }
    .side { padding: 3px 6px; cursor: pointer; border-left: 1px solid rgba(255,255,255,.28); display: inline-flex; align-items: center; }
    .side:hover { background: rgba(255,255,255,.18); }
    .ico { width: 13px; height: 13px; flex: none; }
    .caret { font-size: 9px; opacity: .9; }
    .spin { width: 9px; height: 9px; border: 2px solid rgba(255,255,255,.4); border-top-color: #fff; border-radius: 50%;
            animation: r .8s linear infinite; flex: none; }
    @keyframes r { to { transform: rotate(360deg); } }
    .list { width: 330px; margin-top: 4px; background: #fff; border-radius: 6px; box-shadow: 0 6px 22px rgba(0,0,0,.28);
            overflow: hidden; max-height: 360px; overflow-y: auto; border: 1px solid #d5dbe4; }
    .head { padding: 7px 10px; background: #f4f6fa; border-bottom: 1px solid #e3e8ef; font-weight: 600; color: #253149;
            overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .item { padding: 6px 10px; cursor: pointer; border-bottom: 1px solid #eef1f5; display: flex; gap: 6px; }
    .item:last-child { border-bottom: 0; }
    .item:hover { background: #E9F1F8; }
    .num { color: #6b7686; min-width: 16px; text-align: right; }
    .body { flex: 1; min-width: 0; }
    .t { font-weight: 600; display: flex; align-items: center; gap: 5px; flex-wrap: wrap; }
    .s { color: #6b7686; font-size: 10.5px; overflow-wrap: anywhere; }
    .tag { font-size: 9.5px; font-weight: 700; padding: 0 4px; border-radius: 3px; color: #fff; background: #5f6b7a; line-height: 14px; }
    .tag.HD { background: #2E7D3F; } .tag.FullHD { background: #3570A6; } .tag.K2 { background: #7b3fd6; }
    .tag.K4 { background: #b34d12; } .tag.K8 { background: #c0262d; }
    .codec { font-size: 9.5px; color: #4a5566; border: 1px solid #c9d1dc; border-radius: 3px; padding: 0 3px; line-height: 13px; }
    .all { font-weight: 700; color: #3570A6; }
    .msg { padding: 8px 10px; color: #2E7D3F; }
    .help { padding: 8px 10px; color: #334; background: #fffbe6; border-bottom: 1px solid #f0e2a8; }
    .act { margin-top: 6px; display: inline-block; background: #3570A6; color: #fff; border-radius: 4px; padding: 3px 9px; cursor: pointer; }
    .act:hover { background: #2F6395; }
    .off { cursor: default; opacity: .7; }
    .off:hover { background: #fff; }
    .err { color: #C2453F; }
    /* the count badge, the row size, and the per-row buttons */
    .count { background: #F5A524; color: #1f2733; font-weight: 700; font-size: 10px; line-height: 14px;
             padding: 0 5px; border-radius: 7px; margin-left: 2px; flex: none; }
    .sz { font-weight: 700; margin-left: auto; white-space: nowrap; padding-left: 8px; }
    .est { font-size: 9.5px; font-weight: 700; color: #57606e; background: #eef1f5; border-radius: 3px; padding: 0 4px; line-height: 14px; }
    .acts { display: flex; gap: 4px; align-items: center; flex: none; }
    .mini { font: inherit; font-size: 10.5px; font-weight: 600; color: #3570A6; background: #eef1f5;
            border: 1px solid #d5dbe4; border-radius: 4px; padding: 2px 7px; cursor: pointer; white-space: nowrap; }
    .mini:hover { background: #E9F1F8; }
    .mini:focus-visible { outline: 2px solid #3570A6; outline-offset: 1px; }
    /* the two windows: "All qualities" and "Download part" — one
       choice at a time, never a mixed panel. The selected tab is
       marked by text colour AND an underline, never colour alone. */
    .tabs { display: flex; border-bottom: 1px solid #e3e8ef; background: #f4f6fa; }
    .tab { flex: 1; padding: 7px 6px; text-align: center; font: inherit; font-weight: 600; color: #334; background: none;
           border: 0; border-bottom: 2px solid transparent; cursor: pointer; }
    .tab:hover { background: #E9F1F8; }
    .tab.on { color: #3570A6; border-bottom-color: #3570A6; background: #fff; }
    .tab:focus-visible { outline: 2px solid #3570A6; outline-offset: -2px; }
    /* "Download part": pick a quality, then the start and end */
    .clipwin { padding: 10px; }
    .chead { font-weight: 700; margin-bottom: 4px; color: #253149; }
    .cnote { color: #4a5566; font-size: 11px; margin: 4px 0 8px; }
    .qrow { display: flex; gap: 6px; padding: 6px 8px; cursor: pointer; border: 1px solid #e3e8ef; border-radius: 6px; margin-bottom: 6px; align-items: center; }
    .qrow:hover { background: #E9F1F8; }
    .qrow.sel { border-color: #3570A6; background: #E9F1F8; }
    .selmark { color: #3570A6; font-weight: 700; flex: none; }
    .clipform { display: flex; gap: 6px; align-items: center; padding: 6px 0 2px; flex-wrap: wrap; }
    .clipform input[type=text] { width: 62px; font: inherit; font-size: 11px; padding: 2px 5px; border: 1px solid #d5dbe4; border-radius: 4px; }
    .clipform input[type=text]:focus-visible { outline: 2px solid #3570A6; outline-offset: 1px; }
    .clipform input[type=checkbox]:focus-visible { outline: 2px solid #3570A6; outline-offset: 1px; }
    .clipform label { font-size: 11px; color: #334; display: inline-flex; gap: 3px; align-items: center; cursor: pointer; }
    .back { margin-top: 8px; }
    @media (prefers-reduced-motion: reduce) { .spin { animation: none; } }
    /* looks — flat fills in the EDM palette, one per look */
    .dark .btn { background: #253149; border-color: #1D283A; }
    .dark .btn:hover, .dark .btn.open { background: #1D283A; }
    .light .btn { background: #FFFFFF; border-color: #B0B3B8; color: #253149; }
    .light .btn:hover, .light .btn.open { background: #E9F1F8; }
    .light .side { border-left-color: #c9d3df; }
    .light .spin { border-color: rgba(53,112,166,.3); border-top-color: #3570A6; }
    .green .btn { background: #2E7D3F; border-color: #256B34; }
    .green .btn:hover, .green .btn.open { background: #256B34; }
    .compact .btn { border-radius: 6px; background: #3570A6; border-color: #24567F; }
    .compact .btn:hover, .compact .btn.open { background: #2F6395; }
    .compact #label, .compact .caret, .compact #help { display: none; }
    .compact .part { padding: 4px 7px; }
    .compact .ico { width: 18px; height: 18px; }
    .compact #close { padding: 4px 8px; font-size: 13px; border-left-color: rgba(255,255,255,.3); }
    .icon .btn { border-radius: 50%; }
    .icon .part { padding: 5px; }
    .icon #label, .icon .caret, .icon .side, .icon .count { display: none; }
    .icon .ico { width: 16px; height: 16px; }`;

  function isMain(video) {
    return TOP && video === biggestVideo();
  }

  function directSrc(video) {
    const src = video.currentSrc || video.src || "";
    return /^https?:/i.test(src) && !/\.m3u8|\.mpd/i.test(src) ? src : "";
  }

  // where the bubbles live: in full screen the player's own element (the
  // only thing the browser shows then)
  function layer() {
    const f = document.fullscreenElement || document.webkitFullscreenElement;
    return f && f.tagName !== "VIDEO" ? f : document.documentElement;
  }

  const el = (tag, props = {}, ...kids) => {
    const e = document.createElement(tag);
    for (const [k, v] of Object.entries(props)) {
      if (k === "text") e.textContent = v;
      else e.setAttribute(k, v);
    }
    e.append(...kids);
    return e;
  };

  function makeBubble(video) {
    const host = document.createElement("div");
    host.style.all = "initial";
    host.style.position = "fixed";
    host.style.zIndex = "2147483647";
    host.style.setProperty("--z", String(zoom));
    host.style.setProperty("--o", String(solid));
    const root = host.attachShadow({ mode: "closed" });
    const svgNs = "http://www.w3.org/2000/svg";
    // the simplified EDM mark (decided 2026-11-05): a flat navy tile with
    // the green D-arrow only — the D letterform whose stem runs down into
    // a download arrowhead. No gradient, no E/M, no badge dot.
    const svg = document.createElementNS(svgNs, "svg");
    svg.setAttribute("class", "ico");
    svg.setAttribute("viewBox", "0 0 16 16");
    const tile = document.createElementNS(svgNs, "rect");
    for (const [k, v] of [["x", "0.5"], ["y", "0.5"], ["width", "15"], ["height", "15"], ["rx", "2.5"], ["fill", "#253149"]]) tile.setAttribute(k, v);
    svg.appendChild(tile);
    const arrow = document.createElementNS(svgNs, "path");
    arrow.setAttribute(
      "d",
      "M4.5 3 H8.6 C11.2 3 12.8 4.7 12.8 6.75 C12.8 8.8 11.2 10.5 8.6 10.5 H6.9 V11.6 H8 L5.7 14.6 L3.4 11.6 H4.5 Z " +
        "M6.9 5.2 H8.4 C9.8 5.2 10.6 5.9 10.6 6.75 C10.6 7.6 9.8 8.3 8.4 8.3 H6.9 Z"
    );
    arrow.setAttribute("fill-rule", "evenodd");
    arrow.setAttribute("fill", "#54AB64");
    svg.appendChild(arrow);
    const list = el("div", { class: "list", id: "list" });
    list.hidden = true;
    const spin = el("span", { class: "spin", id: "spin" });
    spin.hidden = true;
    const count = el("span", { class: "count", id: "count" });
    count.hidden = true;
    root.append(
      el("style", { text: STYLE }),
      el(
        "div",
        { class: "wrap " + look },
        el(
          "div",
          { class: "btn", id: "main" },
          el("span", { class: "part", id: "grab" }, svg, spin, count, el("span", { id: "label", text: "Download video" }), el("span", { class: "caret", text: "▼" })),
          el("span", { class: "side", id: "help", title: "Help", text: "?" }),
          el("span", { class: "side", id: "close", title: "Hide", text: "✕" })
        ),
        list
      )
    );
    const b = { host, root, video, open: false, own: null, loading: false, dx: null, dy: null, help: false,
      // the two windows: "all" (every quality) or "clip" (one part);
      // the clip window keeps what the user typed when it re-renders
      view: "all", clipItem: null, clipFrom: "", clipTo: "", clipExact: false };
    const grab = root.getElementById("grab");
    // drag: move it anywhere on the video; a short press is a click
    let down = null;
    grab.addEventListener("mousedown", (e) => {
      if (e.button !== 0) return;
      const hr = host.getBoundingClientRect();
      down = { x: e.clientX, y: e.clientY, ox: e.clientX - hr.left, oy: e.clientY - hr.top, moved: false };
      e.preventDefault();
    });
    addEventListener("mousemove", (e) => {
      if (!down) return;
      if (!down.moved && Math.abs(e.clientX - down.x) + Math.abs(e.clientY - down.y) < 5) return;
      down.moved = true;
      root.getElementById("main").classList.add("drag");
      const r = video.getBoundingClientRect();
      b.dx = e.clientX - down.ox - r.left;
      b.dy = e.clientY - down.oy - r.top;
      placeOne(b);
    });
    addEventListener("mouseup", () => {
      if (!down) return;
      const wasDrag = down.moved;
      down = null;
      root.getElementById("main").classList.remove("drag");
      if (!wasDrag) clicked(b);
    });
    root.getElementById("help").addEventListener("click", () => {
      b.help = !b.help;
      b.open = true;
      renderOne(b);
    });
    root.getElementById("close").addEventListener("click", () => {
      b.dismissed = true;
      host.remove();
    });
    layer().appendChild(host);
    bubbles.set(video, b);
    renderOne(b);
    placeOne(b);
    return b;
  }

  function itemsOf(b) {
    if (b.own) return b.own;
    return isMain(b.video) ? items : [];
  }

  function lookUp(b, stream) {
    // another video on the page (or nothing found for it yet): look it up now
    b.loading = true;
    b.open = true;
    renderOne(b, "Looking for the video qualities…");
    chrome.runtime.sendMessage(
      // its own post / video link, or its own file; the page only when
      // neither is there
      {
        type: "swiftget-resolve-video",
        resolve: stream || "",
        // in a frame that is only a player: the add-in uses the page around it
        page: ownLink(b.video) || (directSrc(b.video) || !TOP || inFeed() ? "" : location.href),
        feed: TOP && inFeed(),
        src: directSrc(b.video),
        frame: !TOP,
      },
      (res) => {
        b.loading = false;
        if (chrome.runtime.lastError || !res) return notRunning(b, () => lookUp(b, stream));
        if (res.error) {
          if (/not running/i.test(res.error)) return notRunning(b, () => lookUp(b, stream));
          return renderOne(b, res.error, true);
        }
        b.own = res.items || [];
        renderOne(b);
      }
    );
  }

  function clicked(b) {
    const list = itemsOf(b);
    if (!list.length && !b.loading) return lookUp(b);
    b.help = false;
    b.open = !b.open;
    renderOne(b);
  }

  // The app is closed: offer to open it, then do what the user wanted.
  function notRunning(b, retry) {
    b.open = true;
    renderOne(b, "Ezoo Download Manager is not running.", true, {
      text: "Open Ezoo Download Manager",
      run: () => {
        // the program registers "ezoo-download:" when it is installed
        const a = document.createElement("a");
        a.href = "ezoo-download://open";
        a.style.display = "none";
        document.documentElement.appendChild(a);
        a.click();
        a.remove();
        renderOne(b, "Opening Ezoo Download Manager…");
        let tries = 0;
        const wait = setInterval(() => {
          tries++;
          chrome.runtime.sendMessage({ type: "swiftget-status" }, (res) => {
            if (chrome.runtime.lastError) return;
            if (res && res.ok) {
              clearInterval(wait);
              retry();
            } else if (tries >= 25) {
              clearInterval(wait);
              renderOne(b, "Ezoo Download Manager did not start. Open it from the Start menu and try again.", true);
            }
          });
        }, 1000);
      },
    });
  }

  function heightOf(q) {
    const m = /^(\d{3,4})p/.exec(q || "");
    return m ? Number(m[1]) : 0;
  }

  // One line of the menu, in plain words: the item's name ("MP4 file ·
  // 1080p [Full HD] AV1"), its size on the same line ("245 MB", with an
  // "estimated" mark when the number is only an estimate), and under it
  // "12:34 · 2,500 kbps". No address is ever shown in a row.
  function describe(m) {
    const y = m.yt;
    if (!y) {
      const sub = m.sub && !/^https?:\/\//i.test(m.sub) ? m.sub : "";
      return { main: m.label, tag: "", codec: "", sub, size: m.size ? fmtSize(m.size) : "", sizeEstimated: false };
    }
    const audio = y.quality === "audio" || y.quality === "mp3" || /^audio\|/.test(y.key || "");
    const ext = (y.ext || "mp4").toUpperCase();
    const q = audio ? "" : y.quality || "";
    const tag = audio ? "" : y.tag || tagOf(heightOf(q));
    const codec = audio ? "" : y.codec || (/\bAV1\b/.test(m.label || "") ? "AV1" : "");
    const kbps = y.kbps || (y.size && y.duration ? Math.round((y.size * 8) / y.duration / 1000) : 0);
    const sub = [fmtLen(y.duration), kbps ? `${kbps.toLocaleString("en")} kbps` : "", y.quality === "mp3" ? "made into MP3 after the download" : ""]
      .filter(Boolean)
      .join(" · ");
    const kind = audio ? `${ext} audio` : `${ext} file`;
    const sizeEstimated = y.quality === "mp3" || y.sizeSource === "filesize_approx" || y.sizeSource === "estimated";
    return { main: q ? `${kind} · ${q}` : kind, tag, codec, sub, size: y.size ? fmtSize(y.size) : "", sizeEstimated };
  }

  // where Preview opens / what Copy copies: the direct stream, else the
  // file's own address; a "choose a quality" row has nothing to preview
  const previewUrl = (m) => (m.yt && m.yt.videoUrl) || m.url || "";
  const copyUrl = (m) => (m.yt && m.yt.videoUrl) || m.url || m.resolve || "";

  // copy a link to the clipboard, with the old textarea trick as a fallback
  function copyLink(url, btn) {
    const done = () => {
      const old = btn.textContent;
      btn.textContent = "Copied ✔";
      setTimeout(() => {
        btn.textContent = old;
      }, 1500);
    };
    const fallback = () => {
      try {
        const ta = document.createElement("textarea");
        ta.value = url;
        ta.style.position = "fixed";
        ta.style.opacity = "0";
        document.documentElement.appendChild(ta);
        ta.select();
        document.execCommand("copy");
        ta.remove();
        done();
      } catch (_) {
        /* clipboard unavailable: the button just stays */
      }
    };
    try {
      const p = navigator.clipboard && navigator.clipboard.writeText(url);
      if (p && p.then) p.then(done, fallback);
      else done();
    } catch (_) {
      fallback();
    }
  }

  // "Download all": the whole playlist (YouTube list), or every file found
  function downloadAllOf(b, usable) {
    if (TOP && /youtube\.com/i.test(location.hostname) && /[?&]list=/.test(location.search)) {
      return { text: "Download all videos of this playlist", run: () => sendPlaylist(b) };
    }
    const files = usable.filter((m) => !m.yt && m.url);
    if (files.length >= 2) {
      return {
        text: `Download all (${files.length} files)`,
        run: () => files.forEach((m, i) => setTimeout(() => download(b, m), i * 300)),
      };
    }
    return null;
  }

  function sendPlaylist(b) {
    chrome.runtime.sendMessage({ type: "swiftget-playlist", url: location.href }, (res) => {
      if (chrome.runtime.lastError || !res || !res.ok) return notRunning(b, () => sendPlaylist(b));
      flashOne(b, "The playlist is open in Ezoo Download Manager ✔", false);
    });
  }

  function renderOne(b, note, isError, action) {
    const list = itemsOf(b);
    const usable = list.filter((i) => !i.disabled);
    const busy = b.loading || (!usable.length && list.some((i) => i.disabled && /Getting/.test(i.label)));
    b.root.getElementById("label").textContent = usable.length > 1 ? `Download video (${usable.length})` : "Download video";
    const count = b.root.getElementById("count");
    count.hidden = usable.length <= 1;
    count.textContent = usable.length > 1 ? String(usable.length) : "";
    b.root.getElementById("spin").hidden = !busy;
    b.root.querySelector("svg").style.display = busy ? "none" : "";
    b.root.getElementById("main").classList.toggle("open", b.open);
    const box = b.root.getElementById("list");
    box.hidden = !b.open;
    box.replaceChildren();
    if (b.help) {
      box.appendChild(
        el("div", {
          class: "help",
          text: "Click the button to choose a quality. Drag it to move it on the video. ✕ hides it for this video. Its size can be changed in the Ezoo Download Manager add-in menu.",
        })
      );
    }
    if (note) {
      // dir "auto": a note in Arabic reads right to left (its full stop at the end)
      const d = el("div", { class: "msg" + (isError ? " err" : ""), dir: "auto", text: note });
      if (action) {
        const a = el("div", { class: "act", text: action.text });
        a.addEventListener("click", action.run);
        d.append(el("br"), a);
      }
      box.appendChild(d);
      return;
    }
    if (!list.length) {
      box.appendChild(el("div", { class: "msg", text: "Looking for the video qualities…" }));
      return;
    }
    const title = (list.find((m) => m.yt && m.yt.title) || {}).yt;
    box.appendChild(el("div", { class: "head", dir: "auto", title: (title && title.title) || cleanTitle(), text: (title && title.title) || cleanTitle() }));
    // the two windows, chosen up front: every quality, or one part
    const tabs = el("div", { class: "tabs" });
    for (const [view, label] of [["all", "All qualities"], ["clip", "Download part"]]) {
      const tab = el("button", { type: "button", class: "tab" + (b.view === view ? " on" : ""), text: label });
      tab.addEventListener("click", (e) => {
        e.stopPropagation();
        stashClipInputs(b);
        b.view = view;
        renderOne(b);
      });
      tabs.appendChild(tab);
    }
    box.appendChild(tabs);
    if (b.view === "clip") {
      renderClip(b, box);
      return;
    }
    const all = downloadAllOf(b, usable);
    if (all) {
      const row = el("div", { class: "item" }, el("span", { class: "num", text: "⇊" }), el("div", { class: "body" }, el("div", { class: "t all", text: all.text })));
      row.addEventListener("click", all.run);
      box.appendChild(row);
    }
    let n = 0;
    list.forEach((m) => {
      const d = describe(m);
      const t = el("div", { class: "t" }, el("span", { text: d.main }));
      if (d.tag) t.append(el("span", { class: "tag " + d.tag.replace(/\s/g, "").replace(/^(\d)K$/, "K$1"), text: d.tag }));
      if (d.codec) t.append(el("span", { class: "codec", text: d.codec }));
      if (d.size) {
        t.append(el("span", { class: "sz", text: d.size }));
        if (d.sizeEstimated) t.append(el("span", { class: "est", text: "estimated" }));
      }
      const sub = d.sub.length > 110 ? d.sub.slice(0, 110) + "…" : d.sub;
      const row = el(
        "div",
        { class: "item" + (m.disabled ? " off" : "") },
        el("span", { class: "num", text: m.disabled ? "" : `${++n}.` }),
        el("div", { class: "body" }, t, el("div", { class: "s", text: sub }))
      );
      if (!m.disabled) {
        // Preview and Copy link: small buttons at the end of the row; the
        // row itself still downloads on click, so the buttons stop the click
        const acts = el("div", { class: "acts" });
        const pv = previewUrl(m);
        if (pv) {
          const b1 = el("button", { type: "button", class: "mini", title: "Preview in a new tab", text: "Preview" });
          b1.addEventListener("click", (e) => {
            e.stopPropagation();
            window.open(pv, "_blank", "noopener");
          });
          acts.append(b1);
        }
        const cp = copyUrl(m);
        if (cp) {
          const b2 = el("button", { type: "button", class: "mini", title: "Copy download link", text: "Copy link" });
          b2.addEventListener("click", (e) => {
            e.stopPropagation();
            copyLink(cp, b2);
          });
          acts.append(b2);
        }
        // "Cut part…": only a direct video file — a stream playlist
        // (.m3u8) is cut part-by-part, not through this panel yet.
        // It opens the "Download part" window with this quality
        // already chosen, instead of a cramped form under the row.
        if (m.url && !/\.m3u8($|\?)/i.test(m.url)) {
          const b3 = el("button", { type: "button", class: "mini", title: "Download only a part of this video", text: "Cut part…" });
          b3.addEventListener("click", (e) => {
            e.stopPropagation();
            showClip(b, m);
          });
          acts.append(b3);
        }
        row.append(acts);
        row.addEventListener("click", () => download(b, m));
      }
      box.appendChild(row);
    });
  }

  function placeOne(b) {
    const fs = document.fullscreenElement || document.webkitFullscreenElement;
    const r = b.video.getBoundingClientRect();
    // closed with its X: it stays closed (also after full screen, a new size…)
    // until the page shows another video; in full screen only when the user
    // asked for it (the add-in's window)
    const onScreen =
      !b.dismissed && (!fs || showInFullscreen) &&
      r.bottom > 0 && r.top < innerHeight && r.width >= 200 && r.height >= 100 && b.video.isConnected &&
      !(fs && fs.tagName === "VIDEO") && (!fs || fs.contains(b.video) || fs.shadowRoot);
    b.host.style.display = onScreen ? "block" : "none";
    if (!onScreen) return;
    if (b.host.parentNode !== layer()) layer().appendChild(b.host);
    // its real size (the user can make the button smaller or bigger)
    const w = b.host.getBoundingClientRect().width || 170;
    const h = b.root.getElementById("main").getBoundingClientRect().height || 22;
    let x = b.dx == null ? r.right - w - 8 : r.left + b.dx;
    let y = b.dy == null ? r.top + 8 : r.top + b.dy;
    x = Math.max(r.left, Math.min(x, r.right - 60));
    y = Math.max(r.top, Math.min(y, r.bottom - h));
    b.host.style.left = `${Math.max(4, Math.min(x, innerWidth - w))}px`;
    b.host.style.top = `${Math.max(4, Math.min(y, innerHeight - h - 4))}px`;
  }

  // How often the panel setting is re-read while the page stays open, so
  // toggling it in Options hides the panels without a reload (the app
  // itself is asked at most once a minute — see getConfig in background.js).
  const PANEL_REFRESH_MS = 30000;
  let lastPanelFetch = Date.now();
  function refreshPanelConfig() {
    lastPanelFetch = Date.now();
    chrome.runtime.sendMessage({ type: "swiftget-config" }, (res) => {
      if (!chrome.runtime.lastError && res) applyPanelConfig(res);
    });
  }

  function place() {
    if (Date.now() - lastPanelFetch >= PANEL_REFRESH_MS) refreshPanelConfig();
    if (!panelAllowed()) return;
    const vids = allVideos().filter((v) => {
      const r = v.getBoundingClientRect();
      return r.width >= 200 && r.height >= 100;
    });
    // bubbles of videos that went away
    for (const [v, b] of bubbles) {
      if (!v.isConnected) {
        b.host.remove();
        bubbles.delete(v);
      }
    }
    // the main video once the add-in found media or it plays (then it shows
    // "looking for qualities"), any other one as soon as it is a real player
    for (const v of vids.slice(0, 12)) {
      let b = bubbles.get(v);
      if (!b) {
        const show = isMain(v) ? items.length || playing(v) : !(v.paused && !(v.duration > 3) && !directSrc(v));
        if (!show) continue;
        b = makeBubble(v);
      }
      if (!b.dismissed) placeOne(b);
    }
  }

  function flashOne(b, text, isError) {
    b.open = true;
    renderOne(b, text, isError);
    setTimeout(() => {
      b.open = false;
      renderOne(b);
    }, 2500);
  }

  function download(b, m) {
    // a stream whose qualities are not known yet: ask the app for them
    if (m.resolve) return lookUp(b, m.resolve);
    const fileName = `${cleanTitle()}.${m.ext || "mp4"}`;
    chrome.runtime.sendMessage(
      { type: "swiftget-download", item: m, title: document.title, fileName, referrer: location.href },
      (res) => {
        if (chrome.runtime.lastError || !res || !res.ok) notRunning(b, () => download(b, m));
        else flashOne(b, "Sent to Ezoo Download Manager ✔", false);
      }
    );
  }

  // "mm:ss" or plain seconds -> seconds, or null when it does not
  // parse. Mirrors the engine's parse_time: at most three parts
  // ("1:2:3:4" is not a time), no negative part ("1:-30" is not
  // 30 seconds), and the result must be a finite number ("inf"
  // used to slip through as NaN and reach the app as null).
  function parseClipTime(text) {
    const t = String(text || "").trim().replace(",", ".");
    if (!t) return null;
    if (/^\d+(\.\d+)?$/.test(t)) {
      const n = Number(t);
      return Number.isFinite(n) && n >= 0 ? n : null;
    }
    const parts = t.split(":");
    if (parts.length < 2 || parts.length > 3) return null;
    let secs = 0;
    for (let i = 0; i < parts.length; i++) {
      if (!/^\d+(\.\d+)?$/.test(parts[i])) return null;
      const v = Number(parts[i]);
      if (i > 0 && v >= 60) return null;
      secs = secs * 60 + v;
    }
    return Number.isFinite(secs) ? secs : null;
  }

  // What the "Download part" window can cut: direct files only.
  // YouTube qualities have no direct file, .m3u8 is a stream
  // playlist, and a "choose a quality" row has nothing to cut yet.
  function clipItems(list) {
    return list.filter((m) => !m.disabled && m.url && !m.yt && !m.resolve && !/\.m3u8($|\?)/i.test(m.url));
  }

  // Keep what the user typed in the clip window before a re-render
  // (switching windows or picking another quality), so nothing
  // they typed is lost.
  function stashClipInputs(b) {
    const box = b.root.getElementById("list");
    if (!box) return;
    const from = box.querySelector(".tin-from");
    const to = box.querySelector(".tin-to");
    const exact = box.querySelector(".exact-cb");
    if (from) b.clipFrom = from.value;
    if (to) b.clipTo = to.value;
    if (exact) b.clipExact = !!exact.checked;
  }

  // Open the "Download part" window with this quality already
  // chosen to cut from.
  function showClip(b, m) {
    stashClipInputs(b);
    b.view = "clip";
    b.clipItem = m;
    b.open = true;
    renderOne(b);
  }

  // Window B, "Download part": the quality to cut from, then the
  // start and end of the part. Streams get a plain note that
  // cutting them is not supported, never a button that pretends
  // otherwise.
  function renderClip(b, box) {
    const list = itemsOf(b);
    const usable = list.filter((i) => !i.disabled);
    const picks = clipItems(list);
    if (!picks.includes(b.clipItem)) b.clipItem = picks[0] || null;
    const win = el("div", { class: "clipwin" });
    win.appendChild(el("div", { class: "chead", text: "Download part of a video" }));
    if (!picks.length) {
      win.appendChild(
        el("div", {
          class: "cnote",
          text: "Cutting a part is not supported for streams yet — YouTube videos and .m3u8 streams can only be downloaded whole in this version.",
        })
      );
      win.appendChild(backButton(b));
      box.appendChild(win);
      return;
    }
    win.appendChild(el("div", { class: "cnote", text: "Choose the quality to cut from, then the start and end of the part." }));
    for (const m of picks) {
      const d = describe(m);
      const row = el("div", { class: "qrow" + (m === b.clipItem ? " sel" : "") });
      if (m === b.clipItem) row.appendChild(el("span", { class: "selmark", text: "✓" }));
      const t = el("div", { class: "t" }, el("span", { text: d.main }));
      if (d.size) {
        t.appendChild(el("span", { class: "sz", text: d.size }));
        if (d.sizeEstimated) t.appendChild(el("span", { class: "est", text: "estimated" }));
      }
      row.appendChild(el("div", { class: "body" }, t, el("div", { class: "s", text: d.sub })));
      row.addEventListener("click", () => {
        stashClipInputs(b);
        b.clipItem = m;
        renderOne(b);
      });
      win.appendChild(row);
    }
    if (usable.some((m) => !picks.includes(m))) {
      win.appendChild(el("div", { class: "cnote", text: "YouTube videos and .m3u8 streams cannot be cut yet." }));
    }
    const from = el("input", { type: "text", class: "tin tin-from", placeholder: "from mm:ss" });
    from.value = b.clipFrom || "";
    const to = el("input", { type: "text", class: "tin tin-to", placeholder: "to mm:ss" });
    to.value = b.clipTo || "";
    const exact = el("input", { type: "checkbox", class: "exact-cb" });
    exact.checked = !!b.clipExact;
    const cut = el("button", { type: "button", class: "mini", text: "Cut part" });
    const err = el("span", { class: "err", text: "" });
    const form = el("div", { class: "clipform" }, from, to, el("label", {}, exact, el("span", { text: "Exact" })), cut, err);
    cut.addEventListener("click", (e) => {
      e.stopPropagation();
      const a = parseClipTime(from.value);
      const z = parseClipTime(to.value);
      if (a === null || z === null) {
        // inline, so the user keeps what they typed
        err.textContent = "Write the start and end as mm:ss or seconds.";
        return;
      }
      if (a < 0 || z - a < 0.5) {
        err.textContent = "End must be after start, at least half a second (mm:ss).";
        return;
      }
      if (z > 24 * 3600) {
        err.textContent = "The end cannot be more than 24 hours.";
        return;
      }
      stashClipInputs(b);
      downloadClip(b, b.clipItem, { start: a, end: z, exact: !!exact.checked });
    });
    win.appendChild(form);
    win.appendChild(
      el("div", {
        class: "cnote",
        text: "Quick cut starts at the nearest keyframe, so it may begin a second or two early. Exact cut re-encodes and starts on the very second.",
      })
    );
    win.appendChild(backButton(b));
    box.appendChild(win);
  }

  function backButton(b) {
    const back = el("button", { type: "button", class: "mini back", text: "← All qualities" });
    back.addEventListener("click", (e) => {
      e.stopPropagation();
      stashClipInputs(b);
      b.view = "all";
      renderOne(b);
    });
    return back;
  }

  function downloadClip(b, m, clip) {
    const fileName = `${cleanTitle()}.${m.ext || "mp4"}`;
    chrome.runtime.sendMessage(
      { type: "swiftget-download", item: m, title: document.title, fileName, referrer: location.href, clip },
      (res) => {
        if (chrome.runtime.lastError || !res) return notRunning(b, () => downloadClip(b, m, clip));
        if (!res.ok) {
          // the app refused the part (an invalid range, or a kind
          // it cannot cut): say so, instead of claiming the app is
          // not running. The window stays on "Download part" with
          // what the user typed.
          b.view = "clip";
          flashOne(b, res.error || "Ezoo Download Manager refused the part.", true);
          return;
        }
        b.view = "all";
        flashOne(b, "Sent to Ezoo Download Manager ✔", false);
      }
    );
  }

  function update(newItems) {
    if (!TOP) return; // frames look up their own videos
    items = newItems || [];
    if (!items.length) {
      // new page / new video: forget the bubbles and "hidden"
      for (const b of bubbles.values()) b.host.remove();
      bubbles.clear();
      return;
    }
    if (!panelAllowed()) return;
    place();
    for (const b of bubbles.values()) if (!b.own && !b.loading) renderOne(b);
  }

  // File links (.zip, .exe, .mp4…): a click goes straight to Ezoo Download Manager,
  // so the browser never starts its own download (and never asks where to
  // save it). The rules are the program's own (Options > File Types).
  const wildcard = (pattern, text) =>
    new RegExp(
      "^" + String(pattern).toLowerCase().replace(/[.+^${}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*").replace(/\?/g, ".") + "$"
    ).test(String(text).toLowerCase());
  let linkRules = null;
  const refreshLinkRules = () =>
    chrome.runtime.sendMessage({ type: "swiftget-link-rules" }, (res) => {
      linkRules = chrome.runtime.lastError ? null : res || null;
    });
  refreshLinkRules();
  setInterval(refreshLinkRules, 60000);
  // shown by the browser itself, not downloaded: never taken from a click.
  // Pages and pictures open in the browser; a PDF too, unless PDF is one of
  // the file types the app takes (then a click on its link goes to the app:
  // once it is open in the browser's PDF viewer, the viewer's own download
  // button always asks where to save, and that can't be stopped)
  const VIEWED = /^(html?|xhtml|php\d?|aspx?|jsp|cgi|pl|cfm|do|action|json|xml|txt|css|js|jpe?g|png|gif|webp|svg|bmp|ico|avif)$/;
  const keyHeld = (e, k) => (k === "alt" ? e.altKey : k === "ctrl" ? e.ctrlKey : k === "shift" ? e.shiftKey : false);
  function wantedLink(e, href) {
    const r = linkRules;
    if (!r || !switchedOn) return false;
    let u;
    try {
      u = new URL(href);
    } catch {
      return false;
    }
    if (!/^https?:$/.test(u.protocol)) return false;
    if (r.skipKey !== "none" && keyHeld(e, r.skipKey)) return false;
    const forced = r.forceKey !== "none" && keyHeld(e, r.forceKey);
    // Ctrl / Shift / Meta + click: a new tab or window, unless that is the "take it" key
    if (!forced && (e.ctrlKey || e.shiftKey || e.metaKey || e.altKey)) return false;
    const host = u.hostname.toLowerCase();
    if ((r.skipSites || []).some((s) => hostMatches(host, s))) return false;
    if ((r.skipAddresses || []).some((a) => wildcard(a, href))) return false;
    const last = decodeURIComponent(u.pathname.split("/").pop() || "");
    const dot = last.lastIndexOf(".");
    const ext = dot > 0 ? last.slice(dot + 1).toLowerCase() : "";
    if (!ext || VIEWED.test(ext)) return false;
    if (ext === "pdf" && !(r.exts || []).some((x) => wildcard(x, ext))) return false;
    // a .torrent file opens in the program's torrent window
    if (forced || ext === "torrent") return true;
    return (r.exts || []).some((x) => wildcard(x, ext));
  }
  addEventListener(
    "click",
    (e) => {
      if (e.button !== 0 || e.defaultPrevented) return;
      const a = e.composedPath ? e.composedPath().find((n) => n && n.tagName === "A" && n.href) : null;
      if (!a || a.dataset.ezooPass) return;
      const href = String(a.href);
      if (!wantedLink(e, href)) return;
      e.preventDefault();
      e.stopPropagation();
      chrome.runtime.sendMessage({ type: "swiftget-link", url: href }, (res) => {
        // not taken (the app is closed, the user chose the browser): the browser's way
        if (chrome.runtime.lastError || !res || !res.ok) {
          const tmp = document.createElement("a");
          tmp.href = href;
          if (a.hasAttribute("download")) tmp.setAttribute("download", a.getAttribute("download") || "");
          tmp.dataset.ezooPass = "1";
          document.documentElement.appendChild(tmp);
          tmp.click();
          tmp.remove();
        }
      });
    },
    true
  );

  // magnet links of torrents go to Ezoo Download Manager (Alt + click: to the browser)
  addEventListener(
    "click",
    (e) => {
      if (e.button !== 0 || e.altKey || e.defaultPrevented || !switchedOn) return;
      const a = e.composedPath ? e.composedPath().find((n) => n && n.tagName === "A" && n.href) : null;
      const href = a ? String(a.href) : "";
      if (!/^magnet:\?/i.test(href) || !/xt=urn:bt[im]h:/i.test(href)) return;
      e.preventDefault();
      e.stopPropagation();
      chrome.runtime.sendMessage({ type: "swiftget-magnet", url: href }, (res) => {
        // not taken (the app is closed, or sending is off): the browser's way
        if (chrome.runtime.lastError || !res || !res.ok) location.href = href;
      });
    },
    true
  );

  // a click anywhere else closes an open menu
  addEventListener(
    "mousedown",
    (e) => {
      const path = e.composedPath ? e.composedPath() : [];
      for (const b of bubbles.values()) {
        if (b.open && !path.includes(b.host)) {
          b.open = false;
          b.help = false;
          renderOne(b);
        }
      }
    },
    true
  );
  addEventListener("keydown", (e) => {
    if (e.key !== "Escape") return;
    for (const b of bubbles.values()) {
      if (b.open) {
        b.open = false;
        renderOne(b);
      }
    }
  });
  for (const ev of ["fullscreenchange", "webkitfullscreenchange"]) {
    document.addEventListener(ev, () => setTimeout(() => bubbles.forEach((b) => placeOne(b)), 50));
  }

  // every link of the page (or the selection) for "Download all links"
  function collectLinks(selectionOnly) {
    const sel = selectionOnly ? getSelection() : null;
    const inSel = (el) => !sel || (sel.rangeCount > 0 && sel.containsNode(el, true));
    const out = [];
    const seen = new Set();
    const add = (url, text) => {
      if (!url || !/^https?:/i.test(url)) return;
      url = url.split("#")[0];
      if (seen.has(url)) return;
      seen.add(url);
      out.push({ url, text: (text || "").replace(/\s+/g, " ").trim().slice(0, 200) });
    };
    // the page itself and the parts built as web components
    const roots = [document];
    try {
      for (const el of document.querySelectorAll("*")) if (el.shadowRoot) roots.push(el.shadowRoot);
    } catch (_) {
      /* a strange page */
    }
    for (const root of roots) {
      for (const a of root.querySelectorAll("a[href]")) if (inSel(a)) add(a.href, a.innerText || a.title);
      for (const el of root.querySelectorAll("img[src], video[src], audio[src], source[src], embed[src]")) {
        if (inSel(el)) add(el.src, el.alt || el.title || "");
      }
    }
    // addresses written as plain text (pages that show a list of links, a
    // note or a paste): what the user sees is what is collected
    const text = sel ? String(sel) : (document.body && document.body.innerText) || "";
    for (const m of text.slice(0, 2000000).matchAll(/\bhttps?:\/\/[^\s"'<>()\[\]{}]+/gi)) {
      add(m[0].replace(/[.,;:!?]+$/, ""), "");
      if (out.length > 5000) break;
    }
    return { page: location.href, links: out };
  }

  chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
    if (msg.type === "swiftget-media") update(msg.items);
    if (msg.type === "swiftget-collect-links") {
      if (!TOP) return false; // the page itself answers
      sendResponse(collectLinks(!!msg.selection));
      return true;
    }
    if (msg.type === "swiftget-open-panel") {
      const v = biggestVideo();
      if (!v) return false;
      // the panel's own setting decides, like every other show path: a
      // fresh fetch, so a setting changed since the page loaded applies
      chrome.runtime.sendMessage({ type: "swiftget-config" }, (res) => {
        if (!chrome.runtime.lastError && res) applyPanelConfig(res);
        if (!panelAllowed()) return;
        let b = bubbles.get(v);
        if (!b) b = makeBubble(v);
        b.dismissed = false;
        layer().appendChild(b.host);
        b.open = true;
        if (!itemsOf(b).length) lookUp(b);
        else renderOne(b);
        placeOne(b);
      });
    }
    return false;
  });
  chrome.runtime.sendMessage({ type: "swiftget-config" }, (res) => {
    if (!chrome.runtime.lastError && res) {
      lastPanelFetch = Date.now();
      applyPanelConfig(res);
    }
  });
  if (TOP) {
    // keys held while clicking a link: Alt = let the browser download it, etc.
    addEventListener(
      "mousedown",
      (e) => {
        chrome.runtime.sendMessage({ type: "swiftget-keys", alt: e.altKey, ctrl: e.ctrlKey, shift: e.shiftKey }).catch?.(() => {});
      },
      true
    );
    // media detected before this script loaded
    chrome.runtime.sendMessage({ type: "swiftget-media-request" }, (res) => {
      if (!chrome.runtime.lastError) update(res);
    });
  }

  addEventListener("scroll", place, { passive: true, capture: true });
  addEventListener("resize", place, { passive: true });
  setInterval(place, 700);
  setInterval(watchVideos, 1500);
  document.addEventListener("play", () => setTimeout(() => (watchVideos(), place()), 300), true);
  addEventListener("popstate", () => setTimeout(watchVideos, 500));
})();
