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

const isUrl = (s) => /^https?:\/\//i.test(s || "");
const sizeLike = (s) => /^\d+(\.\d+)?\s*(B|KB|MB|GB)$/i.test((s || "").trim());

// the row shows the item's name + size only — never its address. The size
// is the app's own number when it sent one ("94 MB"), else the byte count;
// an estimate is marked as such, never shown as exact.
const sizeOf = (m) => {
  if (m.sub && sizeLike(m.sub)) return { text: m.sub.trim(), estimated: false };
  const n = (m.yt && m.yt.size) || m.size || 0;
  if (!n) return { text: "", estimated: false };
  const est = !!(m.yt && (m.yt.quality === "mp3" || m.yt.sizeSource === "filesize_approx" || m.yt.sizeSource === "estimated"));
  return { text: fmtSize(n), estimated: est };
};
const subOf = (m) => {
  const s = m.sub || "";
  if (s && !isUrl(s) && !sizeLike(s)) return s;
  if (m.yt && m.yt.duration) return fmtLen(m.yt.duration);
  return "";
};
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
      document.body.appendChild(ta);
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

async function init() {
  document.getElementById("settings").addEventListener("click", () => {
    chrome.runtime.openOptionsPage();
    window.close();
  });

  // the on / off switch: off = the browser keeps its downloads, no buttons
  // on videos, nothing is looked up (the add-in stays installed)
  const on = document.getElementById("enabled");
  const showOn = (v) => {
    on.checked = v;
    document.body.classList.toggle("off", !v);
    document.getElementById("onText").textContent = v ? "Ezoo Download Manager is on" : "Ezoo Download Manager is paused";
    document.getElementById("onHint").textContent = v
      ? "Downloads and videos of this browser go to Ezoo Download Manager"
      : "The browser keeps its downloads, no buttons on videos";
  };
  chrome.storage.local.get({ enabled: true }, (o) => showOn(o.enabled !== false));
  on.addEventListener("change", () => {
    chrome.storage.local.set({ enabled: on.checked });
    showOn(on.checked);
  });

  const look = document.getElementById("look");
  chrome.storage.local.get({ bubbleStyle: "compact" }, (o) => (look.value = o.bubbleStyle || "compact"));
  look.addEventListener("change", () => chrome.storage.local.set({ bubbleStyle: look.value }));

  // how solid the button is on the video (the mouse on it: always solid)
  const solid = document.getElementById("solid");
  const solidText = document.getElementById("solidText");
  const showSolid = (v) => {
    solid.value = String(v);
    solidText.textContent = `${v}%`;
  };
  chrome.storage.local.get({ bubbleOpacity: 0.9 }, (o) => showSolid(Math.round((Number(o.bubbleOpacity) || 0.9) * 100)));
  solid.addEventListener("input", () => {
    showSolid(Number(solid.value));
    chrome.storage.local.set({ bubbleOpacity: Number(solid.value) / 100 });
  });

  // full screen: the button stays out of the way unless the user wants it
  const fullscreen = document.getElementById("fullscreen");
  chrome.storage.local.get({ bubbleFullscreen: false }, (o) => (fullscreen.checked = !!o.bubbleFullscreen));
  fullscreen.addEventListener("change", () => chrome.storage.local.set({ bubbleFullscreen: fullscreen.checked }));

  // size of the "Download this video" button on pages (also in the settings page)
  const scale = document.getElementById("scale");
  const scaleText = document.getElementById("scaleText");
  const OLD = { small: 85, normal: 100, large: 125 };
  const show = (v) => {
    scale.value = String(v);
    scaleText.textContent = `${v}%`;
  };
  chrome.storage.local.get({ bubbleScale: null, bubbleSize: "normal" }, (o) =>
    show(o.bubbleScale != null ? Math.round(o.bubbleScale * 100) : OLD[o.bubbleSize] || 100)
  );
  const save = (v) => {
    show(v);
    chrome.storage.local.set({ bubbleScale: v / 100 });
  };
  scale.addEventListener("input", () => save(Number(scale.value)));
  document.getElementById("scaleReset").addEventListener("click", () => save(100));

  chrome.runtime.sendMessage({ type: "swiftget-status" }, (res) => {
    const el = document.getElementById("status");
    const ok = res && res.ok;
    el.textContent = ok ? "connected" : "app not running";
    el.classList.toggle("on", !!ok);
    if (!ok) {
      // the program registers "ezoo-download:" when it is installed
      const open = document.createElement("a");
      open.href = "ezoo-download://open";
      open.className = "open";
      open.textContent = "Open Ezoo Download Manager";
      open.addEventListener("click", () => setTimeout(() => window.close(), 300));
      document.querySelector("section").appendChild(open);
    }
  });

  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab) return;
  chrome.runtime.sendMessage({ type: "swiftget-get-media", tabId: tab.id }, (items) => {
    if (!items || !items.length) return;
    const box = document.getElementById("media");
    box.replaceChildren();
    const title = (tab.title || "video").replace(/[<>:"/\\|?*\u0000-\u001f]/g, "_").slice(0, 120);
    // a YouTube playlist: all of its videos in the app's playlist window
    if (/youtube\.com/i.test(tab.url || "") && /[?&]list=/.test(tab.url || "")) {
      const row = document.createElement("div");
      row.className = "media";
      const info = document.createElement("div");
      info.textContent = "All videos of this playlist";
      const btn = document.createElement("button");
      btn.className = "all";
      btn.textContent = "Download all";
      btn.addEventListener("click", () =>
        chrome.runtime.sendMessage({ type: "swiftget-playlist", url: tab.url }, (res) => {
          btn.textContent = res && res.ok ? "Opened ✔" : "App not running";
        })
      );
      row.append(info, btn);
      box.appendChild(row);
    }
    const makeRow = (m) => {
      const row = document.createElement("div");
      row.className = "media";
      const info = document.createElement("div");
      const name = document.createElement("span");
      name.textContent = m.label;
      info.appendChild(name);
      const sz = sizeOf(m);
      if (sz.text) {
        const size = document.createElement("span");
        size.className = "sz";
        size.textContent = " · " + sz.text;
        info.appendChild(size);
        if (sz.estimated) {
          const est = document.createElement("span");
          est.className = "est";
          est.textContent = "estimated";
          info.appendChild(est);
        }
      }
      const small = document.createElement("small");
      small.textContent = subOf(m);
      info.appendChild(small);
      if (m.disabled) {
        // still looking / can't be downloaded: no buttons on this row
        row.append(info);
        return row;
      }
      const acts = document.createElement("div");
      acts.className = "acts";
      const pv = previewUrl(m);
      if (pv) {
        const b = document.createElement("button");
        b.className = "mini";
        b.textContent = "Preview";
        b.title = "Preview in a new tab";
        b.addEventListener("click", () => chrome.tabs.create({ url: pv }));
        acts.appendChild(b);
      }
      const cp = copyUrl(m);
      if (cp) {
        const b = document.createElement("button");
        b.className = "mini";
        b.textContent = "Copy link";
        b.title = "Copy download link";
        b.addEventListener("click", () => copyLink(cp, b));
        acts.appendChild(b);
      }
      const btn = document.createElement("button");
      btn.textContent = "Download";
      btn.addEventListener("click", () => {
        if (m.resolve) {
          // a stream: its qualities come from the app, shown in its place
          btn.textContent = "…";
          btn.disabled = true;
          chrome.runtime.sendMessage({ type: "swiftget-resolve-video", resolve: m.resolve }, (res) => {
            if (!res || res.error || !res.items) {
              btn.textContent = (res && res.error) || "App not running";
              return;
            }
            row.replaceWith(...res.items.map((it) => makeRow(it)));
          });
          return;
        }
        chrome.runtime.sendMessage(
          { type: "swiftget-download", item: m, title: tab.title, referrer: tab.url, fileName: `${title}.${m.ext || "mp4"}` },
          (res) => {
            btn.textContent = res && res.ok ? "Sent ✔" : "App not running";
          }
        );
      });
      acts.appendChild(btn);
      row.append(info, acts);
      return row;
    };
    items.forEach((m) => box.appendChild(makeRow(m)));
  });
}

init();
