//! "Add Ezoo Download Manager to my browser": opens the browser on the right page
//! (its store page once the extension is published there, otherwise its
//! extensions page with the folder path ready in the clipboard).

use crate::i18n::{t, tf};
use crate::open_window;
use crate::*;

/// Store pages of the published extension. Empty = not published yet:
/// then the extension is added from its folder (developer mode).
const STORE_CHROME: &str = "";
const STORE_EDGE: &str = "";
const STORE_FIREFOX: &str = "";
const STORE_OPERA: &str = "";
const STORE_BRAVE: &str = "";

/// The Firefox add-on signed by Mozilla (built by the release): installed
/// from this file it stays in Firefox for good.
const FIREFOX_XPI: &str = "ezoo-download-firefox.xpi";

#[derive(Clone)]
pub(crate) struct Browser {
    name: &'static str,
    exe: Option<PathBuf>,
    extensions_page: &'static str,
    store: &'static str,
    firefox: bool,
    /// The disc colour on the browser card (crates/app/ui/dialogs.slint
    /// `BrowserCard`): v1.1.2 EDM branding — the logo palette, one hue per
    /// browser, kept distinct: Chrome EDM Green fill #2e8b47, Edge EDM Blue
    /// dark #2f6699, Firefox amber-orange #c85a00 (amber family), Opera EDM
    /// Red #c2453f, Brave orange-red #d63f14. The white globe reads >= 3:1
    /// on every one of them; the old browser-brand tints are gone.
    tint: slint::Color,
}

#[cfg(windows)]
fn env_path(var: &str, rest: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(|v| PathBuf::from(v).join(rest))
}

/// A program on PATH (Linux: chrome, firefox... are plain names).
#[cfg(not(windows))]
fn on_path(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

fn first_existing(list: Vec<Option<PathBuf>>) -> Option<PathBuf> {
    list.into_iter().flatten().find(|p| p.exists())
}

#[cfg(windows)]
fn detect() -> Vec<Browser> {
    let rgb = slint::Color::from_rgb_u8;
    vec![
        Browser {
            name: "Chrome",
            exe: first_existing(vec![
                env_path("ProgramFiles", r"Google\Chrome\Application\chrome.exe"),
                env_path("ProgramFiles(x86)", r"Google\Chrome\Application\chrome.exe"),
                env_path("LOCALAPPDATA", r"Google\Chrome\Application\chrome.exe"),
            ]),
            extensions_page: "chrome://extensions/",
            store: STORE_CHROME,
            firefox: false,
            tint: rgb(0x2e, 0x8b, 0x47),
        },
        Browser {
            name: "Edge",
            exe: first_existing(vec![
                env_path(
                    "ProgramFiles(x86)",
                    r"Microsoft\Edge\Application\msedge.exe",
                ),
                env_path("ProgramFiles", r"Microsoft\Edge\Application\msedge.exe"),
            ]),
            extensions_page: "edge://extensions/",
            store: STORE_EDGE,
            firefox: false,
            tint: rgb(0x2f, 0x66, 0x99),
        },
        Browser {
            name: "Firefox",
            exe: first_existing(vec![
                env_path("ProgramFiles", r"Mozilla Firefox\firefox.exe"),
                env_path("ProgramFiles(x86)", r"Mozilla Firefox\firefox.exe"),
                env_path("LOCALAPPDATA", r"Mozilla Firefox\firefox.exe"),
            ]),
            extensions_page: "about:debugging#/runtime/this-firefox",
            store: STORE_FIREFOX,
            firefox: true,
            tint: rgb(0xc8, 0x5a, 0x00),
        },
        Browser {
            name: "Opera",
            exe: first_existing(vec![
                env_path("LOCALAPPDATA", r"Programs\Opera\opera.exe"),
                env_path("LOCALAPPDATA", r"Programs\Opera GX\opera.exe"),
                env_path("ProgramFiles", r"Opera\opera.exe"),
            ]),
            extensions_page: "opera://extensions",
            store: STORE_OPERA,
            firefox: false,
            tint: rgb(0xc2, 0x45, 0x3f),
        },
        Browser {
            name: "Brave",
            exe: first_existing(vec![
                env_path(
                    "ProgramFiles",
                    r"BraveSoftware\Brave-Browser\Application\brave.exe",
                ),
                env_path(
                    "ProgramFiles(x86)",
                    r"BraveSoftware\Brave-Browser\Application\brave.exe",
                ),
                env_path(
                    "LOCALAPPDATA",
                    r"BraveSoftware\Brave-Browser\Application\brave.exe",
                ),
            ]),
            extensions_page: "brave://extensions/",
            store: STORE_BRAVE,
            firefox: false,
            tint: rgb(0xd6, 0x3f, 0x14),
        },
    ]
}

#[cfg(not(windows))]
fn detect() -> Vec<Browser> {
    let rgb = slint::Color::from_rgb_u8;
    vec![
        Browser {
            name: "Chrome",
            exe: first_existing(vec![
                on_path("google-chrome"),
                on_path("google-chrome-stable"),
                on_path("chromium"),
                on_path("chromium-browser"),
            ]),
            extensions_page: "chrome://extensions/",
            store: STORE_CHROME,
            firefox: false,
            tint: rgb(0x2e, 0x8b, 0x47),
        },
        Browser {
            name: "Edge",
            exe: first_existing(vec![
                on_path("microsoft-edge"),
                on_path("microsoft-edge-stable"),
            ]),
            extensions_page: "edge://extensions/",
            store: STORE_EDGE,
            firefox: false,
            tint: rgb(0x2f, 0x66, 0x99),
        },
        Browser {
            name: "Firefox",
            exe: first_existing(vec![
                on_path("firefox"),
                on_path("firefox-esr"),
                Some(PathBuf::from("/usr/lib/firefox/firefox")),
            ]),
            extensions_page: "about:debugging#/runtime/this-firefox",
            store: STORE_FIREFOX,
            firefox: true,
            tint: rgb(0xc8, 0x5a, 0x00),
        },
        Browser {
            name: "Opera",
            exe: first_existing(vec![
                on_path("opera"),
                Some(PathBuf::from("/usr/bin/opera")),
            ]),
            extensions_page: "opera://extensions",
            store: STORE_OPERA,
            firefox: false,
            tint: rgb(0xc2, 0x45, 0x3f),
        },
        Browser {
            name: "Brave",
            exe: first_existing(vec![
                on_path("brave-browser"),
                on_path("brave"),
                Some(PathBuf::from("/usr/bin/brave-browser")),
            ]),
            extensions_page: "brave://extensions/",
            store: STORE_BRAVE,
            firefox: false,
            tint: rgb(0xd6, 0x3f, 0x14),
        },
    ]
}

/// The extension as it came with this program: when a file of its folder is
/// missing or was changed (an antivirus, a half-finished copy...), the
/// browser says "files missing or unreadable" and won't load it, so the
/// folder is put back in order from this copy first.
const EXTENSION_FILES: &[(&str, &[u8])] = &[
    (
        "background.js",
        include_bytes!("../../../extension/background.js"),
    ),
    (
        "content.js",
        include_bytes!("../../../extension/content.js"),
    ),
    (
        "yt-core.js",
        include_bytes!("../../../extension/yt-core.js"),
    ),
    (
        "popup.html",
        include_bytes!("../../../extension/popup.html"),
    ),
    ("popup.js", include_bytes!("../../../extension/popup.js")),
    (
        "options.html",
        include_bytes!("../../../extension/options.html"),
    ),
    (
        "options.js",
        include_bytes!("../../../extension/options.js"),
    ),
    (
        "icons/icon16.png",
        include_bytes!("../../../extension/icons/icon16.png"),
    ),
    (
        "icons/icon32.png",
        include_bytes!("../../../extension/icons/icon32.png"),
    ),
    (
        "icons/icon48.png",
        include_bytes!("../../../extension/icons/icon48.png"),
    ),
    (
        "icons/icon128.png",
        include_bytes!("../../../extension/icons/icon128.png"),
    ),
];
const MANIFEST: &[u8] = include_bytes!("../../../extension/manifest.json");
const MANIFEST_FIREFOX: &[u8] = include_bytes!("../../../extension/manifest.firefox.json");

/// Every file of the extension in `dir`, as it came with the program (only
/// what differs is written). false = it could not be written there.
pub(crate) fn put_extension(dir: &Path, firefox: bool) -> bool {
    let manifest = if firefox { MANIFEST_FIREFOX } else { MANIFEST };
    let all = EXTENSION_FILES
        .iter()
        .copied()
        .chain(std::iter::once(("manifest.json", manifest)));
    let mut ok = true;
    for (name, data) in all {
        let path = dir.join(name);
        if std::fs::read(&path)
            .map(|have| have == data)
            .unwrap_or(false)
        {
            continue;
        }
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // through a new file, so the browser never reads a half one
        let tmp = path.with_extension("ezoo-new");
        if std::fs::write(&tmp, data)
            .and_then(|_| std::fs::rename(&tmp, &path))
            .is_err()
        {
            let _ = std::fs::remove_file(&tmp);
            ok = false;
        }
    }
    ok
}

/// At start: the extension folders next to the program are complete
/// (an extension loaded from them before an update gets the new files
/// at once). Where the program's folder cannot be written (a .deb under
/// /usr/share, a Flatpak), the copy in the program's data folder is
/// completed instead — never the system copy, and nothing is ever
/// deleted: unknown files in the folder are left alone.
pub(crate) fn prepare_extension_folders() {
    std::thread::spawn(|| {
        for firefox in [false, true] {
            let _ = extension_dir(firefox);
        }
    });
}

/// Repair the extension folders now (the browser dialog's "Repair"
/// button): rewrite every bundled file in the data-folder copy (and in
/// the copy next to the program when it is writable). Unknown files are
/// left alone — a repair never deletes anything.
pub(crate) fn repair_extension() -> bool {
    let mut ok = true;
    for firefox in [false, true] {
        let mut done = extension_dir(firefox)
            .map(|d| put_extension(&d, firefox))
            .unwrap_or(false);
        if !done {
            // the loaded copy could not be written (a read-only system
            // copy): complete the data-folder copy instead
            let own = swiftget_engine::default_data_dir().join(if firefox {
                "extension-firefox"
            } else {
                "extension"
            });
            done = put_extension(&own, firefox);
        }
        ok &= done;
    }
    ok
}
fn extension_dir(firefox: bool) -> Option<PathBuf> {
    let name = if firefox {
        "extension-firefox"
    } else {
        "extension"
    };
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let next_to = dir.join(name);
    if put_extension(&next_to, firefox) {
        return Some(next_to);
    }
    // Linux: the .deb also puts a copy under /usr/share/ezoo-download
    // (read-only there, so it is used as-is, never written)
    for p in [
        dir.join("..").join(name),
        dir.join("..")
            .join("share")
            .join("ezoo-download")
            .join(name),
        PathBuf::from("/usr/share/ezoo-download").join(name),
    ] {
        if p.exists() {
            return Some(p);
        }
    }
    let own = swiftget_engine::default_data_dir().join(name);
    put_extension(&own, firefox).then_some(own)
}

/// The signed Firefox add-on next to the program, when the release has one.
fn firefox_xpi() -> Option<PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    [
        dir.join(FIREFOX_XPI),
        dir.join("..").join(FIREFOX_XPI),
        dir.join("..")
            .join("share")
            .join("ezoo-download")
            .join(FIREFOX_XPI),
        PathBuf::from("/usr/share/ezoo-download").join(FIREFOX_XPI),
    ]
    .into_iter()
    .find(|p| p.exists())
}

pub(crate) fn open(ui: &Ui) {
    let Some(d) = ensure_browsers(ui) else {
        return;
    };
    let list = detect();
    let rows: Vec<BrowserRow> = list
        .iter()
        .map(|b| BrowserRow {
            name: b.name.into(),
            tint: b.tint,
            installed: b.exe.is_some(),
            action: t(if b.store.is_empty() {
                "Click to add"
            } else {
                "Click to add from the store"
            })
            .into(),
        })
        .collect();
    *ui.browser_list.borrow_mut() = list;
    d.set_browsers(ModelRc::new(VecModel::from(rows)));
    d.set_steps(SharedString::new());
    d.set_folder(
        extension_dir(false)
            .map(|p| p.display().to_string())
            .unwrap_or_default()
            .into(),
    );
    open_window!(d);
    d.window().set_minimized(false);
}

fn launch(exe: &Path, url: &str) {
    let _ = std::process::Command::new(exe).arg(url).spawn();
}

fn add(ui: &Ui, i: usize) {
    let Some(d) = ui.browsers.upgrade() else {
        return;
    };
    let Some(b) = ui.browser_list.borrow().get(i).cloned() else {
        return;
    };
    let Some(exe) = b.exe.clone() else {
        d.set_steps(tf("{} is not installed on this computer.", &[&b.name]).into());
        return;
    };
    if !b.store.is_empty() {
        launch(&exe, b.store);
        d.set_steps(
            tf(
                "{} opened the store page: click \"Add\" / \"Get\" there.",
                &[&b.name],
            )
            .into(),
        );
        return;
    }
    if b.firefox {
        if let Some(xpi) = firefox_xpi() {
            // the signed add-on: Firefox asks "Add Ezoo Download Manager?" and keeps it
            let _ = std::process::Command::new(&exe).arg(&xpi).spawn();
            d.set_folder(xpi.display().to_string().into());
            d.set_steps(t("Firefox opened the Ezoo Download Manager add-on.\n1. Click \"Add\" in the message Firefox shows.\n2. Then click \"OK\".\n\nThe add-on is signed by Mozilla, so it stays in Firefox after you close it and updates by itself.").into());
            return;
        }
    }
    let Some(dir) = extension_dir(b.firefox) else {
        d.set_steps(t("The extension folder was not found next to EzooDownload.exe. Install Ezoo Download Manager again.").into());
        return;
    };
    let path = dir.display().to_string();
    if let Ok(mut c) = arboard::Clipboard::new() {
        let _ = c.set_text(path.clone());
    }
    launch(&exe, b.extensions_page);
    d.set_folder(path.clone().into());
    let steps = if b.firefox {
        t("Firefox opened its add-ons debugging page.\n1. Click \"Load Temporary Add-on…\".\n2. Paste the folder path (Ctrl+V, it is already copied) and open the file \"manifest.json\" in it.\n\nFirefox removes add-ons loaded this way when it closes. To keep it for good the extension must be signed by Mozilla (free) – ask for the signed version.")
    } else {
        tf("{} opened its extensions page.\n1. Turn on \"Developer mode\" (a switch on that page).\n2. Click \"Load unpacked\".\n3. Paste the folder path (Ctrl+V – it is already copied) and click \"Select folder\".\n\nThat's all: the Ezoo Download Manager icon appears next to the address bar. You do this only once.", &[&b.name])
    };
    d.set_steps(steps.into());
}

/// The browser-extension dialog, created (and wired once) on first use.
pub(crate) fn ensure_browsers(ui: &Ui) -> Option<BrowserDialog> {
    if let Some(d) = ui.browsers.upgrade() {
        return Some(d);
    }
    let Ok(d) = BrowserDialog::new() else {
        return None;
    };
    crate::set_theme_of(
        d.global::<Theme>(),
        &crate::Look::of(&ui.ctx.mgr.settings()),
    );
    wire(ui, &d);
    ui.browsers.store(d);
    ui.browsers.upgrade()
}

pub(crate) fn wire(ui: &Ui, d: &BrowserDialog) {
    {
        let ui = ui.clone();
        d.on_add(move |i| add(&ui, i.max(0) as usize));
    }
    {
        let w = d.as_weak();
        d.on_copy_folder(move || {
            if let Some(d) = w.upgrade() {
                if let Ok(mut c) = arboard::Clipboard::new() {
                    let _ = c.set_text(d.get_folder().to_string());
                }
            }
        });
    }
    {
        let w = d.as_weak();
        d.on_open_folder(move || {
            if let Some(d) = w.upgrade() {
                let f = d.get_folder().to_string();
                if !f.is_empty() {
                    let _ = open::that_detached(f);
                }
            }
        });
    }
    {
        let w = d.as_weak();
        d.on_repair(move || {
            let ok = repair_extension();
            if let Some(d) = w.upgrade() {
                d.set_steps(
                    t(if ok {
                        "The extension files were repaired. If the browser still says files are missing, remove the extension and add it again from this folder."
                    } else {
                        "The extension files could not be repaired: the folder could not be written."
                    })
                    .into(),
                );
                // the folder shown may have moved to the data-folder copy
                d.set_folder(
                    extension_dir(false)
                        .map(|p| p.display().to_string())
                        .unwrap_or_default()
                        .into(),
                );
            }
        });
    }
    {
        let w = d.as_weak();
        d.on_close_window(move || {
            if let Some(d) = w.upgrade() {
                let _ = d.hide();
            }
        });
    }
}
