//! Site Grabber and "Download all links" (both use the same window).

use crate::i18n::{t, tf};
use crate::open_window;
use crate::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use swiftget_engine::fmt_size;
use swiftget_engine::grabber::{self, FoundFile, GrabOptions, GrabProgress};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct Item {
    checked: bool,
    url: String,
    name: String,
    ext: String,
    kind: &'static str,
    text: String,
    page: String,
    is_page: bool,
    /// file size, from a HEAD request after the crawl (None = unknown)
    size: Option<u64>,
}

#[derive(Default)]
pub(crate) struct GrabUi {
    all: Vec<Item>,
    /// rows on screen -> index in `all`
    shown: Vec<usize>,
    model: Option<Rc<VecModel<LinkRow>>>,
    incoming: Arc<Mutex<Vec<FoundFile>>>,
    progress: Option<Arc<GrabProgress>>,
    cancel: Option<CancellationToken>,
    finished: Arc<AtomicBool>,
    exploring: bool,
    /// links mode: the page and its cookies
    headers: RequestHeaders,
    /// sizes found by the probe task (index in `all`, bytes)
    sizes: Arc<Mutex<Vec<(usize, u64)>>>,
    /// a size probe is running (one per crawl / links batch)
    size_probing: bool,
    size_done: Arc<AtomicBool>,
    size_cancel: Option<CancellationToken>,
}

fn item_from(url: &str, text: &str, page: &str) -> Item {
    let name = filename::from_url(url).unwrap_or_else(|| "download".into());
    let ext = filename::extension(&name);
    let is_page = grabber::is_page_ext(&ext);
    Item {
        checked: !is_page,
        url: url.to_string(),
        kind: if is_page { "text" } else { kind_for(&name) },
        ext: ext.to_uppercase(),
        name,
        text: text.trim().chars().take(200).collect(),
        page: page.to_string(),
        is_page,
        size: None,
    }
}

fn type_ok(it: &Item, filter: i32) -> bool {
    match filter {
        1 => it.kind == "image",
        2 => it.kind == "video",
        3 => it.kind == "audio",
        4 => matches!(it.kind, "archive" | "exe" | "apk" | "disc"),
        5 => matches!(it.kind, "pdf" | "doc" | "sheet" | "slides" | "text") && !it.is_page,
        6 => it.is_page,
        _ => !it.is_page,
    }
}

fn row(it: &Item) -> LinkRow {
    LinkRow {
        checked: it.checked,
        name: it.name.clone().into(),
        kind: it.kind.into(),
        ext: it.ext.clone().into(),
        text: it.text.clone().into(),
        url: it.url.clone().into(),
        // exact size from the probe, "—" while unknown (never estimated)
        size: it.size.map(fmt_size).unwrap_or_else(|| "—".into()).into(),
    }
}

/// Rebuild the visible rows from the filters.
fn show(ui: &Ui) {
    let Some(d) = ui.grabber.upgrade() else {
        return;
    };
    let filter = d.get_filter().to_lowercase();
    let type_filter = d.get_type_filter();
    let mut g = ui.grab_state.borrow_mut();
    let shown: Vec<usize> = g
        .all
        .iter()
        .enumerate()
        .filter(|(_, it)| type_ok(it, type_filter))
        .filter(|(_, it)| {
            filter.is_empty()
                || it.name.to_lowercase().contains(&filter)
                || it.url.to_lowercase().contains(&filter)
        })
        .map(|(i, _)| i)
        .collect();
    let rows: Vec<LinkRow> = shown.iter().map(|i| row(&g.all[*i])).collect();
    g.shown = shown;
    let model = g
        .model
        .get_or_insert_with(|| Rc::new(VecModel::default()))
        .clone();
    let checked = g.all.iter().filter(|i| i.checked).count();
    drop(g);
    update_model(&model, rows);
    d.set_links(ModelRc::from(model));
    d.set_checked_text(tf("{} file(s) checked", &[&checked]).into());
}

fn reset(ui: &Ui, d: &GrabberDialog, links_mode: bool) {
    let mut g = ui.grab_state.borrow_mut();
    if let Some(c) = g.cancel.take() {
        c.cancel();
    }
    if let Some(c) = g.size_cancel.take() {
        c.cancel();
    }
    g.size_probing = false;
    g.size_done.store(false, Ordering::Relaxed);
    g.all.clear();
    g.shown.clear();
    g.exploring = false;
    g.progress = None;
    g.headers = RequestHeaders::default();
    g.incoming = Arc::new(Mutex::new(Vec::new()));
    drop(g);
    let s = ui.ctx.mgr.settings();
    d.set_links_mode(links_mode);
    d.set_exploring(false);
    d.set_message(SharedString::new());
    d.set_filter(SharedString::new());
    d.set_type_filter(0);
    d.set_queue_names(queue_names(&s));
    d.set_queue_index(0);
    d.set_folder(SharedString::new());
    d.set_progress_text(SharedString::new());
}

/// Tasks > Run site grabber.
pub(crate) fn open_grabber(ui: &Ui) {
    let Some(d) = ensure_grabber(ui) else {
        return;
    };
    if d.window().is_visible() && !d.get_links_mode() {
        open_window!(d);
        return;
    }
    reset(ui, &d, false);
    show(ui);
    open_window!(d);
}

/// "Download all links with Ezoo Download" from the browser.
pub(crate) fn open_links(ui: &Ui, l: api::LinksRequest) {
    let Some(d) = ensure_grabber(ui) else {
        return;
    };
    reset(ui, &d, true);
    {
        let mut g = ui.grab_state.borrow_mut();
        g.headers = RequestHeaders {
            referer: (!l.page.is_empty()).then(|| l.page.clone()),
            cookies: l.cookies.clone(),
            user_agent: l.user_agent.clone(),
            ..Default::default()
        };
        let mut seen = std::collections::HashSet::new();
        for link in &l.links {
            if seen.insert(link.url.clone()) {
                g.all.push(item_from(&link.url, &link.text, &l.page));
            }
        }
    }
    d.set_start_url(l.page.clone().into());
    show(ui);
    bring_to_front(&ui.ctx);
    open_window!(d);
    maybe_probe_sizes(ui);
}

/// One HEAD request for a file's size (Content-Length when the server
/// sends one). Best effort: any failure just leaves the size unknown.
/// No cookies are sent: a link that needs a session simply stays
/// unknown instead of probing with the browser's session.
async fn probe_size(
    client: &reqwest::Client,
    url: &str,
    referer: Option<&str>,
    user_agent: &str,
) -> Option<u64> {
    let mut rb = client.head(url);
    if let Some(r) = referer {
        rb = rb.header("referer", r);
    }
    if !user_agent.is_empty() {
        rb = rb.header("user-agent", user_agent);
    }
    let resp = tokio::time::timeout(Duration::from_secs(3), rb.send())
        .await
        .ok()?
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    // NOTE: `Response::content_length()` is the body's size hint, and a
    // HEAD response never has a body, so it always answers Some(0) here.
    // The size is the Content-Length header itself.
    resp.headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
}

/// Fill the shown rows' sizes with bounded HEAD requests, once per
/// crawl / links batch. The crawler never knows sizes, and a HEAD per
/// link during the crawl would hammer the site, so this runs after the
/// crawl: at most 100 rows, 6 at a time, 3 seconds each, and it stops
/// when the window closes or a new crawl starts.
fn maybe_probe_sizes(ui: &Ui) {
    let (targets, client, user_agent, sizes, done, cancel) = {
        let mut g = ui.grab_state.borrow_mut();
        if g.size_probing || g.all.is_empty() {
            return;
        }
        let targets: Vec<(usize, String, String)> = g
            .shown
            .iter()
            .take(100)
            .filter_map(|&i| {
                g.all
                    .get(i)
                    .filter(|it| it.size.is_none() && !it.is_page)
                    .map(|it| (i, it.url.clone(), it.page.clone()))
            })
            .collect();
        if targets.is_empty() {
            return;
        }
        let cancel = CancellationToken::new();
        g.size_cancel = Some(cancel.clone());
        g.size_probing = true;
        g.size_done.store(false, Ordering::Relaxed);
        (
            targets,
            ui.ctx.mgr.client(),
            ui.ctx.mgr.settings().user_agent,
            g.sizes.clone(),
            g.size_done.clone(),
            cancel,
        )
    };
    ui.ctx.rt.spawn(async move {
        let sem = Arc::new(tokio::sync::Semaphore::new(6));
        let mut set = tokio::task::JoinSet::new();
        for (idx, url, page) in targets {
            let client = client.clone();
            let sem = sem.clone();
            let cancel = cancel.clone();
            let sizes = sizes.clone();
            let ua = user_agent.clone();
            set.spawn(async move {
                let _permit = sem.acquire().await;
                if cancel.is_cancelled() {
                    return;
                }
                if let Some(n) = probe_size(
                    &client,
                    &url,
                    (!page.is_empty()).then_some(page.as_str()),
                    &ua,
                )
                .await
                {
                    sizes.lock().unwrap().push((idx, n));
                }
            });
        }
        while set.join_next().await.is_some() {}
        done.store(true, Ordering::Relaxed);
    });
}

/// New files from the explorer, and its counters (10 times a second).
pub(crate) fn poll(ui: &Ui) {
    // sizes found by the probe task, applied on the UI thread
    let mut drained = false;
    {
        let mut g = ui.grab_state.borrow_mut();
        if g.size_probing {
            let found: Vec<(usize, u64)> = std::mem::take(&mut *g.sizes.lock().unwrap());
            if !found.is_empty() {
                for (idx, n) in found {
                    if let Some(it) = g.all.get_mut(idx) {
                        it.size = Some(n);
                    }
                }
                drained = true;
            }
            if g.size_done.load(Ordering::Relaxed) {
                g.size_probing = false;
                g.size_done.store(false, Ordering::Relaxed);
                g.size_cancel = None;
            }
        }
    }
    if drained {
        show(ui);
    }
    let (new, progress, finished) = {
        let g = ui.grab_state.borrow();
        if !g.exploring {
            return;
        }
        let new: Vec<FoundFile> = std::mem::take(&mut *g.incoming.lock().unwrap());
        (new, g.progress.clone(), g.finished.load(Ordering::Relaxed))
    };
    let Some(d) = ui.grabber.upgrade() else {
        return;
    };
    if !new.is_empty() {
        let mut g = ui.grab_state.borrow_mut();
        for f in new {
            g.all.push(item_from(&f.url, &f.text, &f.page));
        }
        drop(g);
        show(ui);
    }
    if let Some(p) = progress {
        let current = p.current.lock().map(|c| c.clone()).unwrap_or_default();
        let text = tf(
            "{} of {} pages explored  ·  {} files found",
            &[
                &p.pages_done.load(Ordering::Relaxed),
                &p.pages_queued.load(Ordering::Relaxed),
                &p.files.load(Ordering::Relaxed),
            ],
        ) + &if finished {
            t("  ·  done")
        } else {
            format!("  ·  {current}")
        };
        d.set_progress_text(text.into());
    }
    if finished {
        ui.grab_state.borrow_mut().exploring = false;
        d.set_exploring(false);
        maybe_probe_sizes(ui);
    }
}

/// The Grabber dialog, created (and wired exactly once) on first use.
pub(crate) fn ensure_grabber(ui: &Ui) -> Option<GrabberDialog> {
    if let Some(d) = ui.grabber.upgrade() {
        return Some(d);
    }
    let Ok(d) = GrabberDialog::new() else {
        return None;
    };
    crate::set_theme_of(
        d.global::<Theme>(),
        &crate::Look::of(&ui.ctx.mgr.settings()),
    );
    wire(ui, &d);
    ui.grabber.store(d);
    ui.grabber.upgrade()
}

pub(crate) fn wire(ui: &Ui, d: &GrabberDialog) {
    {
        let w = d.as_weak();
        d.on_template_changed(move |t| {
            if let Some(d) = w.upgrade() {
                if t != 6 {
                    d.set_exts(grabber::template_exts(t).join(" ").into());
                }
            }
        });
    }
    {
        let ui = ui.clone();
        d.on_explore(move || {
            let Some(d) = ui.grabber.upgrade() else {
                return;
            };
            let start = d.get_start_url().trim().to_string();
            if !(start.starts_with("http://") || start.starts_with("https://"))
                || url::Url::parse(&start).is_err()
            {
                d.set_message(t("Type the address of the start page (http:// or https://)").into());
                return;
            }
            d.set_message(SharedString::new());
            let exts: Vec<String> = if d.get_template() == 6 {
                d.get_exts()
                    .split_whitespace()
                    .map(|e| e.trim_start_matches('.').to_lowercase())
                    .collect()
            } else {
                grabber::template_exts(d.get_template())
            };
            let s = ui.ctx.mgr.settings();
            let opts = GrabOptions {
                start_url: start.clone(),
                depth: d.get_depth().clamp(0, 10) as u32,
                other_sites: d.get_other_sites(),
                no_parent: d.get_no_parent(),
                exts,
                include: Vec::new(),
                exclude: Vec::new(),
                max_pages: 2000,
                parallel: d.get_parallel().clamp(1, 10) as usize,
                headers: RequestHeaders {
                    auth: s.login_for(&start),
                    ..Default::default()
                },
                user_agent: s.user_agent.clone(),
            };
            let progress = Arc::new(GrabProgress::default());
            let cancel = CancellationToken::new();
            let finished = Arc::new(AtomicBool::new(false));
            let incoming = Arc::new(Mutex::new(Vec::new()));
            {
                let mut g = ui.grab_state.borrow_mut();
                if let Some(c) = g.cancel.take() {
                    c.cancel();
                }
                if let Some(c) = g.size_cancel.take() {
                    c.cancel();
                }
                g.size_probing = false;
                g.size_done.store(false, Ordering::Relaxed);
                g.all.clear();
                g.progress = Some(progress.clone());
                g.cancel = Some(cancel.clone());
                g.finished = finished.clone();
                g.incoming = incoming.clone();
                g.exploring = true;
            }
            show(&ui);
            d.set_exploring(true);
            d.set_progress_text(t("Starting…").into());
            let client = ui.ctx.mgr.client();
            ui.ctx.rt.spawn(async move {
                let sink = incoming.clone();
                grabber::crawl(client, opts, progress, cancel, move |f| {
                    sink.lock().unwrap().push(f)
                })
                .await;
                finished.store(true, Ordering::Relaxed);
            });
        });
    }
    {
        let ui = ui.clone();
        d.on_stop_explore(move || {
            if let Some(c) = ui.grab_state.borrow().cancel.clone() {
                c.cancel();
            }
            ui.grab_state
                .borrow()
                .finished
                .store(true, Ordering::Relaxed);
        });
    }
    {
        let ui = ui.clone();
        d.on_toggle(move |i| {
            {
                let mut g = ui.grab_state.borrow_mut();
                if let Some(&idx) = g.shown.get(i.max(0) as usize) {
                    g.all[idx].checked = !g.all[idx].checked;
                }
            }
            show(&ui);
        });
    }
    {
        // one row's Preview button: open the link in the browser (a new
        // tab there, per Q20) — it never toggles the row's checkbox
        let ui = ui.clone();
        d.on_preview(move |i| {
            let g = ui.grab_state.borrow();
            if let Some(&idx) = g.shown.get(i.max(0) as usize) {
                if let Some(it) = g.all.get(idx) {
                    let _ = open::that_detached(&it.url);
                }
            }
        });
    }
    {
        // one row's Copy link button: the address to the clipboard
        let ui = ui.clone();
        d.on_copy_link(move |i| {
            let g = ui.grab_state.borrow();
            if let Some(&idx) = g.shown.get(i.max(0) as usize) {
                if let Some(it) = g.all.get(idx) {
                    if let Ok(mut c) = arboard::Clipboard::new() {
                        let _ = c.set_text(it.url.clone());
                    }
                }
            }
        });
    }
    {
        let ui = ui.clone();
        d.on_check_all(move |on| {
            {
                let mut g = ui.grab_state.borrow_mut();
                let shown = g.shown.clone();
                for idx in shown {
                    g.all[idx].checked = on;
                }
            }
            show(&ui);
        });
    }
    {
        let ui = ui.clone();
        d.on_filter_changed(move || show(&ui));
    }
    {
        let (w, mgr) = (d.as_weak(), ui.ctx.mgr.clone());
        d.on_browse(move || {
            let Some(d) = w.upgrade() else { return };
            if let Some(dir) = crate::winfx::file_box()
                .set_directory(mgr.settings().download_dir)
                .pick_folder()
            {
                d.set_folder(dir.display().to_string().into());
            }
        });
    }
    {
        let ui = ui.clone();
        d.on_download(move || {
            let Some(d) = ui.grabber.upgrade() else {
                return;
            };
            let (items, headers) = {
                let g = ui.grab_state.borrow();
                (
                    g.all
                        .iter()
                        .filter(|i| i.checked)
                        .cloned()
                        .collect::<Vec<_>>(),
                    g.headers.clone(),
                )
            };
            if items.is_empty() {
                d.set_message(t("Check the files you want first").into());
                return;
            }
            let s = ui.ctx.mgr.settings();
            let queue = s
                .queues
                .get(d.get_queue_index().max(0) as usize)
                .map(|q| q.id)
                .unwrap_or(MAIN_QUEUE);
            let folder = d.get_folder().trim().to_string();
            let use_text = d.get_use_text();
            let mut added = 0;
            for it in items {
                if ui.ctx.mgr.find_url(&it.url).is_some() {
                    continue;
                }
                let mut h = headers.clone();
                if h.referer.is_none() && !it.page.is_empty() {
                    h.referer = Some(it.page.clone());
                }
                ui.ctx.mgr.add(NewDownload {
                    url: it.url.clone(),
                    save_dir: (!folder.is_empty()).then(|| PathBuf::from(&folder)),
                    headers: h,
                    queue: Some(queue),
                    description: (use_text && !it.text.is_empty()).then(|| it.text.clone()),
                    start: false,
                    ..Default::default()
                });
                added += 1;
            }
            if d.get_start_now() {
                ui.ctx.mgr.start_queue(queue);
            }
            if let Some(m) = ui.ctx.main.upgrade() {
                m.set_view(format!("queue:{queue}").into());
            }
            d.set_message(SharedString::new());
            d.set_checked_text(tf("{} file(s) added to the queue", &[&added]).into());
            refresh(&ui);
        });
    }
    {
        let ui = ui.clone();
        d.on_close_window(move || {
            if let Some(c) = ui.grab_state.borrow_mut().cancel.take() {
                c.cancel();
            }
            if let Some(c) = ui.grab_state.borrow_mut().size_cancel.take() {
                c.cancel();
            }
            ui.grab_state.borrow_mut().size_probing = false;
            ui.grab_state.borrow_mut().exploring = false;
            if let Some(d) = ui.grabber.upgrade() {
                let _ = d.hide();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// one-shot server answering a HEAD with a fixed Content-Length
    async fn serve_head_once(response: &'static [u8]) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let _ = sock.read(&mut buf).await;
            let _ = sock.write_all(response).await;
        });
        addr
    }

    #[tokio::test]
    async fn probe_size_reads_content_length() {
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        // HTTP/1.1, like a normal server
        let addr = serve_head_once(
            b"HTTP/1.1 200 OK\r\nContent-Length: 12345\r\nContent-Type: application/octet-stream\r\n\r\n",
        )
        .await;
        assert_eq!(
            probe_size(&client, &format!("http://{addr}/file1.bin"), None, "test").await,
            Some(12345)
        );
        // HTTP/1.0, like the Phase 6 fixture server (BaseHTTPRequestHandler)
        let addr = serve_head_once(
            b"HTTP/1.0 200 OK\r\nServer: BaseHTTP/0.6 Python/3.12.3\r\nDate: Sun, 01 Nov 2026 11:00:00 GMT\r\nContent-Type: application/octet-stream\r\nContent-Length: 67890\r\nAccept-Ranges: bytes\r\n\r\n",
        )
        .await;
        assert_eq!(
            probe_size(&client, &format!("http://{addr}/file2.bin"), None, "test").await,
            Some(67890)
        );
    }

    #[tokio::test]
    async fn probe_size_unknown_without_content_length() {
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let addr =
            serve_head_once(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\n\r\n")
                .await;
        assert_eq!(
            probe_size(&client, &format!("http://{addr}/x.bin"), None, "test").await,
            None
        );
    }
}
