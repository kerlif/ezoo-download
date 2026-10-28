//! New versions of Ezoo Download from GitHub, checked through the fork's
//! releases API (see `swiftget_engine::selfupdate`): right after the
//! program opens it looks for a newer release, and then every 24 hours
//! while it stays open. A failed check (offline, rate-limited, malformed
//! answers) fails silently and is retried later with backoff — no error
//! dialogs, no nagging. The check can be turned off in Options > General;
//! Help > Check for updates still works then.
//!
//! When a newer release is found, its artifact is downloaded in the
//! background into a temporary directory and verified (SHA-256 against
//! both the signed manifest and the release checksum file). Only then is
//! the user asked whether to install it, and installation waits until no
//! download is running — unless the user drives the install themselves
//! from Help > Check for updates, which installs at once.
//!
//! Linux: an AppImage is replaced the same way (ask, then replace the
//! file; the new version starts on the next launch). A .deb/.rpm install
//! or a Flatpak is notify-only, with a link to the release page; a
//! Flatpak offer has a button that opens the page. An install whose kind
//! cannot be told is notify-only too, which is always safe.

use crate::i18n::t;
use crate::i18n::tf;
use crate::message;
#[cfg(windows)]
use crate::open_window;
use crate::{Ctx, Ui};
#[cfg(windows)]
use crate::{Theme, UpdateDialog};
#[cfg(windows)]
use slint::ComponentHandle;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use swiftget_engine::http::{self, ProxyConfig};
use swiftget_engine::selfupdate;
use swiftget_engine::{Manager, Status};

/// One check / download at a time (automatic or from the menu).
static BUSY: AtomicBool = AtomicBool::new(false);

/// Versions the user already declined (automatic checks) or was already
/// told about (notify-only), this run: asking again every 24 hours about
/// the same release would be nagging. A newer version asks again.
static DECLINED: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
static NOTIFIED: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

fn update_dir() -> PathBuf {
    swiftget_engine::default_data_dir().join("update")
}

fn cache_path() -> PathBuf {
    update_dir().join("checker-cache.json")
}

fn downloading(mgr: &Manager) -> bool {
    mgr.snapshot().iter().any(|v| {
        matches!(
            v.status,
            Status::Connecting | Status::Downloading | Status::Assembling
        )
    })
}

/// The checker client for the current proxy settings, falling back to
/// the system proxy when the settings do not build a client.
fn checker_client_for(mgr: &Manager) -> Option<reqwest::Client> {
    let proxy = ProxyConfig::from_settings(&mgr.settings());
    http::checker_client(&proxy)
        .or_else(|_| {
            http::checker_client(&ProxyConfig {
                mode: 1,
                ..Default::default()
            })
        })
        .ok()
}

#[cfg(windows)]
fn setup_filename(version: &str) -> String {
    format!(
        "EzooDownload-Setup-{}.exe",
        version.trim_start_matches(['v', 'V'])
    )
}

/// Ask a yes/no question on the UI thread, from an async task.
async fn ask_user(text: String) -> bool {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let posted = slint::invoke_from_event_loop(move || {
        let yes = crate::ask(&text);
        let _ = tx.send(yes);
    });
    if posted.is_err() {
        return false;
    }
    rx.await.unwrap_or(false)
}

/// Show a message on the UI thread, from an async task.
fn tell_user(text: String) {
    let _ = slint::invoke_from_event_loop(move || message(&text));
}

fn already_seen(slot: &std::sync::Mutex<Option<String>>, version: &str) -> bool {
    slot.lock()
        .map(|g| g.as_deref() == Some(version))
        .unwrap_or(false)
}

fn mark_seen(slot: &std::sync::Mutex<Option<String>>, version: &str) {
    if let Ok(mut g) = slot.lock() {
        *g = Some(version.to_string());
    }
}

/// Start the installer and close the program (downloads resume next time).
#[cfg(windows)]
fn apply(setup: PathBuf, sha256: String) {
    apply_as(setup, sha256, false)
}

#[cfg(windows)]
fn apply_as(setup: PathBuf, sha256: String, visible: bool) {
    match selfupdate::run_installer(&setup, &sha256, visible) {
        Ok(()) => {
            let _ = slint::quit_event_loop();
        }
        Err(e) => message(&tf(
            "Could not start the update:\n\n{}",
            &[&format!("{e:#}")],
        )),
    }
}

/// Outcome of one automatic round, deciding how long until the next try.
enum RoundOutcome {
    /// Checked (or offered and handled): wait the regular 24 hours.
    Done,
    /// A verified release turned out bad, or the user declined: also
    /// wait 24 hours — retrying eagerly would nag or hammer.
    Settled,
    /// The network or the service failed: retry with backoff.
    Retryable { retry_after_secs: Option<u64> },
}

pub fn spawn(ctx: &Ctx) {
    // leftovers of an update that is installed now, or a partial download
    if let Ok(rd) = std::fs::read_dir(update_dir()) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with(".download")
                || (cfg!(windows) && (name.contains(&format!("-{}.", selfupdate::current()))))
            {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    #[cfg(windows)]
    {
        if !selfupdate::installed() {
            return; // a portable copy: there is no installation to update
        }
    }
    let mgr = ctx.mgr.clone();
    ctx.rt.spawn(async move {
        // first check as soon as the program is open
        tokio::time::sleep(Duration::from_secs(3)).await;
        let mut failures: u32 = 0;
        loop {
            if !mgr.settings().update_check {
                // off in Options: look again in a while, in case it is
                // turned back on; Help > Check for updates still works
                tokio::time::sleep(Duration::from_secs(300)).await;
                continue;
            }
            if BUSY.swap(true, Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_secs(60)).await;
                continue;
            }
            let outcome = round(&mgr).await;
            BUSY.store(false, Ordering::SeqCst);
            match outcome {
                RoundOutcome::Done | RoundOutcome::Settled => {
                    failures = 0;
                    tokio::time::sleep(selfupdate::CHECK_INTERVAL).await;
                }
                RoundOutcome::Retryable { retry_after_secs } => {
                    failures = failures.saturating_add(1);
                    let wait = selfupdate::next_retry_delay(failures, retry_after_secs);
                    tokio::time::sleep(wait).await;
                }
            }
        }
    });
}

/// One automatic check. Silent throughout: nothing here shows a dialog
/// except the ask / notify flows of a verified update (see below).
async fn round(mgr: &Arc<Manager>) -> RoundOutcome {
    let Some(client) = checker_client_for(mgr) else {
        return RoundOutcome::Retryable {
            retry_after_secs: None,
        };
    };
    match selfupdate::check_latest(
        &client,
        &selfupdate::api_latest_url(),
        &cache_path(),
        selfupdate::current(),
    )
    .await
    {
        selfupdate::CheckOutcome::UpToDate => RoundOutcome::Done,
        selfupdate::CheckOutcome::SilentFail {
            retry_after_secs, ..
        } => RoundOutcome::Retryable { retry_after_secs },
        selfupdate::CheckOutcome::Available(rel) => {
            handle_available(mgr, &client, &rel, false).await
        }
    }
}

/// A verified newer release: download, verify, then ask / notify, per
/// platform. `manual` is Help > Check for updates (Linux): it bypasses
/// the declined/notified memory and reports failures to the user, who
/// asked. Returns how the automatic loop should schedule the next check.
async fn handle_available(
    mgr: &Arc<Manager>,
    client: &reqwest::Client,
    rel: &selfupdate::CheckedRelease,
    manual: bool,
) -> RoundOutcome {
    #[cfg(windows)]
    {
        return windows_available(mgr, client, rel, manual).await;
    }
    #[cfg(not(windows))]
    {
        return linux_available(mgr, client, rel, manual).await;
    }
}

/// The Windows half of `handle_available`.
#[cfg(windows)]
async fn windows_available(
    mgr: &Arc<Manager>,
    client: &reqwest::Client,
    rel: &selfupdate::CheckedRelease,
    manual: bool,
) -> RoundOutcome {
    {
        if !selfupdate::installed() {
            return RoundOutcome::Done;
        }
        if !manual && already_seen(&DECLINED, &rel.version) {
            return RoundOutcome::Done;
        }
        let url = rel.setup_asset_url();
        let filename = setup_filename(&rel.version);
        let setup = match selfupdate::download_artifact(
            client,
            &url,
            &filename,
            &rel.manifest.sha256,
            &rel.sums_text,
            &update_dir(),
            &|_, _| {},
        )
        .await
        {
            Ok(p) => p,
            Err(e) => {
                if manual {
                    tell_user(tf("Could not get the update:\n\n{}", &[&format!("{e:#}")]));
                }
                return if e.is_verification() {
                    RoundOutcome::Settled
                } else {
                    RoundOutcome::Retryable {
                        retry_after_secs: None,
                    }
                };
            }
        };
        // the user is asked, and the install waits until no download
        // is running (the manual window is the immediate path instead)
        let question = tf(
            "Version {} was downloaded and checked. Install it now? Ezoo Download closes and opens again when it is installed.",
            &[&rel.version],
        );
        if !ask_user(question).await {
            mark_seen(&DECLINED, &rel.version);
            return RoundOutcome::Done;
        }
        while downloading(mgr) {
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
        let sha = rel.manifest.sha256.clone();
        let _ = slint::invoke_from_event_loop(move || apply(setup, sha));
        RoundOutcome::Done
    }
}

/// The Linux half of `handle_available` (AppImage replace / notify-only).
#[cfg(not(windows))]
async fn linux_available(
    mgr: &Arc<Manager>,
    client: &reqwest::Client,
    rel: &selfupdate::CheckedRelease,
    manual: bool,
) -> RoundOutcome {
    match selfupdate::detect_package_current() {
        selfupdate::PackageKind::AppImage => {
            let Some(current) = selfupdate::current_appimage_path() else {
                return notify_only(rel, manual).await;
            };
            if !manual && already_seen(&DECLINED, &rel.version) {
                return RoundOutcome::Done;
            }
            let Some((filename, url)) = rel.appimage_asset() else {
                return notify_only(rel, manual).await;
            };
            let file = match selfupdate::download_artifact(
                client,
                &url,
                &filename,
                // the AppImage hash is cross-checked against the
                // release's checksum file (see docs/UPDATE-CHECKER.md);
                // the signed manifest gates the offer itself
                &appimage_sha(rel, &filename),
                &rel.sums_text,
                &update_dir(),
                &|_, _| {},
            )
            .await
            {
                Ok(p) => p,
                Err(e) => {
                    if manual {
                        tell_user(tf("Could not get the update:\n\n{}", &[&format!("{e:#}")]));
                    }
                    return if e.is_verification() {
                        RoundOutcome::Settled
                    } else {
                        RoundOutcome::Retryable {
                            retry_after_secs: None,
                        }
                    };
                }
            };
            let question = tf(
                "Version {} was downloaded and checked. Replace the AppImage file now? The new version starts the next time you open Ezoo Download.",
                &[&rel.version],
            );
            if !ask_user(question).await {
                mark_seen(&DECLINED, &rel.version);
                return RoundOutcome::Done;
            }
            while downloading(mgr) {
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
            let sha = appimage_sha(rel, &filename);
            match selfupdate::install_appimage(&file, &current, &sha) {
                Ok(()) => {
                    tell_user(tf(
                        "The AppImage file was replaced. Open Ezoo Download again to use version {}.",
                        &[&rel.version],
                    ));
                }
                Err(e) => {
                    tell_user(tf(
                        "Could not start the update:\n\n{}",
                        &[&format!("{e:#}")],
                    ));
                }
            }
            RoundOutcome::Done
        }
        selfupdate::PackageKind::Flatpak => {
            if !manual && already_seen(&NOTIFIED, &rel.version) {
                return RoundOutcome::Done;
            }
            mark_seen(&NOTIFIED, &rel.version);
            let text = format!(
                "{}\n\n{}\n{}",
                tf("Version {} is available", &[&rel.version]),
                t("Open the release page in the browser?"),
                rel.release_url
            );
            if ask_user(text).await {
                let _ = open::that(&rel.release_url);
            }
            RoundOutcome::Done
        }
        selfupdate::PackageKind::Managed | selfupdate::PackageKind::Unknown => {
            notify_only(rel, manual).await
        }
    }
}

/// The checksum the AppImage is verified against: the release checksum
/// file's entry for it. The signed manifest's hash covers the Windows
/// installer only, so an AppImage release without a checksum entry is
/// never downloaded (the sums check in `download_artifact` refuses it).
#[cfg(not(windows))]
fn appimage_sha(rel: &selfupdate::CheckedRelease, filename: &str) -> String {
    selfupdate::sums_hash(&rel.sums_text, filename).unwrap_or_default()
}

/// Notify-only: a message with the release-page link, once per version
/// per run for automatic checks, always for a manual check.
#[cfg(not(windows))]
async fn notify_only(rel: &selfupdate::CheckedRelease, manual: bool) -> RoundOutcome {
    if !manual && already_seen(&NOTIFIED, &rel.version) {
        return RoundOutcome::Done;
    }
    mark_seen(&NOTIFIED, &rel.version);
    tell_user(format!(
        "{}\n\n{}",
        tf("Version {} is available", &[&rel.version]),
        tf(
            "New versions for Linux are published on the GitHub Releases page of the project:\n{}\n\nThe .deb updates with the package manager, the AppImage by downloading the new file.",
            &[&rel.release_url],
        ),
    ));
    RoundOutcome::Done
}

/// What the update window shows: written by the check / download (async
/// side), copied to the window by a timer on the UI thread.
#[cfg(windows)]
#[derive(Clone, Default)]
struct Shown {
    state: i32,
    heading: String,
    detail: String,
    progress: f32,
    /// downloaded and checked: run this setup (path, sha256)
    install: Option<(PathBuf, String)>,
    done: bool,
}

thread_local! {
    static WINDOW_TIMER: slint::Timer = slint::Timer::default();
}

#[cfg(windows)]
fn size_text(n: u64) -> String {
    let mb = n as f64 / (1024.0 * 1024.0);
    format!("{mb:.1} MB")
}

/// Help > Check for updates: look now instead of waiting for the next check.
/// On Windows in a window of its own; on Linux with the real check and a
/// message (or the AppImage ask) when it answers.
pub fn check_now(ui: &Ui) {
    #[cfg(not(windows))]
    {
        check_now_linux(ui);
    }
    #[cfg(windows)]
    {
        check_now_windows(ui);
    }
}

/// Help > Check for updates on Linux: the check itself, then a message
/// for the outcome. Failures are shown here (the user asked), unlike the
/// silent automatic checks.
#[cfg(not(windows))]
fn check_now_linux(ui: &Ui) {
    if BUSY.swap(true, Ordering::SeqCst) {
        message(&t(
            "A new version is being prepared. It is installed by itself when it is ready.",
        ));
        return;
    }
    let mgr = ui.ctx.mgr.clone();
    ui.ctx.rt.spawn(async move {
        let finish = |text: String| {
            BUSY.store(false, Ordering::SeqCst);
            tell_user(text);
        };
        let Some(client) = checker_client_for(&mgr) else {
            return finish(format!(
                "{}\n\n{}",
                t("Could not check for updates"),
                selfupdate::releases_url()
            ));
        };
        match selfupdate::check_latest(
            &client,
            &selfupdate::api_latest_url(),
            &cache_path(),
            selfupdate::current(),
        )
        .await
        {
            selfupdate::CheckOutcome::UpToDate => finish(tf(
                "Ezoo Download is up to date (version {}).",
                &[&selfupdate::current()],
            )),
            selfupdate::CheckOutcome::SilentFail { .. } => finish(format!(
                "{}\n\n{}",
                t("Could not check for updates"),
                selfupdate::releases_url()
            )),
            selfupdate::CheckOutcome::Available(rel) => {
                let outcome = handle_available(&mgr, &client, &rel, true).await;
                BUSY.store(false, Ordering::SeqCst);
                let _ = outcome;
            }
        }
    });
}

/// The update dialog, created (and wired exactly once) on first use.
/// On Linux it is never created: `check_now` answers with a message.
#[cfg(windows)]
pub(crate) fn ensure_updater(ui: &Ui) -> Option<UpdateDialog> {
    if let Some(d) = ui.updater.upgrade() {
        return Some(d);
    }
    let Ok(d) = UpdateDialog::new() else {
        return None;
    };
    crate::set_theme_of(
        d.global::<Theme>(),
        &crate::Look::of(&ui.ctx.mgr.settings()),
    );
    {
        let w = d.as_weak();
        d.on_close_window(move || {
            if let Some(d) = w.upgrade() {
                let _ = d.hide();
            }
        });
    }
    ui.updater.store(d);
    ui.updater.upgrade()
}

#[cfg(windows)]
fn check_now_windows(ui: &Ui) {
    let Some(d) = ensure_updater(ui) else {
        return;
    };
    open_window!(d);
    let set = |state: i32, heading: String, detail: String| {
        d.set_state(state);
        d.set_heading(heading.into());
        d.set_detail(detail.into());
        d.set_progress(-1.0);
    };
    if BUSY.swap(true, Ordering::SeqCst) {
        set(
            0,
            t("A new version is being prepared. It is installed by itself when it is ready."),
            String::new(),
        );
        return;
    }
    set(
        0,
        t("Checking for updates…"),
        tf("This is version {}.", &[&selfupdate::current()]),
    );
    let shared = Arc::new(std::sync::Mutex::new(Shown {
        state: 0,
        progress: -1.0,
        ..Default::default()
    }));
    {
        // UI thread: show what the async side wrote
        let (shared, weak) = (shared.clone(), d.as_weak());
        WINDOW_TIMER.with(|timer| {
            timer.start(
                slint::TimerMode::Repeated,
                Duration::from_millis(100),
                move || {
                    let now = shared.lock().unwrap().clone();
                    if let Some(d) = weak.upgrade() {
                        if now.state != 0 || !now.heading.is_empty() {
                            d.set_state(now.state);
                            d.set_heading(now.heading.clone().into());
                            d.set_detail(now.detail.clone().into());
                            d.set_progress(now.progress);
                        }
                    }
                    if let Some((setup, sha)) = shared.lock().unwrap().install.take() {
                        apply_as(setup, sha, true);
                    }
                    if now.done {
                        WINDOW_TIMER.with(|t| t.stop());
                    }
                },
            );
        });
    }
    let tell = move |shared: &std::sync::Mutex<Shown>,
                     state: i32,
                     heading: String,
                     detail: String,
                     progress: f32| {
        let mut s = shared.lock().unwrap();
        s.state = state;
        s.heading = heading;
        s.detail = detail;
        s.progress = progress;
    };
    let mgr = ui.ctx.mgr.clone();
    ui.ctx.rt.spawn(async move {
        let finish = |shared: &std::sync::Mutex<Shown>| {
            BUSY.store(false, Ordering::SeqCst);
            shared.lock().unwrap().done = true;
        };
        let Some(client) = checker_client_for(&mgr) else {
            tell(
                &shared,
                4,
                t("Could not check for updates"),
                tf("This is version {}.", &[&selfupdate::current()]),
                -1.0,
            );
            return finish(&shared);
        };
        let rel = match selfupdate::check_latest(
            &client,
            &selfupdate::api_latest_url(),
            &cache_path(),
            selfupdate::current(),
        )
        .await
        {
            selfupdate::CheckOutcome::SilentFail { .. } => {
                tell(
                    &shared,
                    4,
                    t("Could not check for updates"),
                    tf("This is version {}.", &[&selfupdate::current()]),
                    -1.0,
                );
                return finish(&shared);
            }
            selfupdate::CheckOutcome::UpToDate => {
                tell(
                    &shared,
                    1,
                    t("Ezoo Download is up to date"),
                    tf(
                        "Version {} is the newest version.",
                        &[&selfupdate::current()],
                    ),
                    -1.0,
                );
                return finish(&shared);
            }
            selfupdate::CheckOutcome::Available(rel) => rel,
        };
        if !selfupdate::installed() {
            tell(
                &shared,
                4,
                tf("Version {} is available", &[&rel.version]),
                t("This copy was not installed with the setup, so it cannot update itself."),
                -1.0,
            );
            return finish(&shared);
        }
        let heading = tf("Downloading version {}…", &[&rel.version]);
        tell(&shared, 2, heading.clone(), String::new(), -1.0);
        let (s2, h2) = (shared.clone(), heading.clone());
        let progress = move |done: u64, total: Option<u64>| {
            let (detail, p) = match total {
                Some(t) if t > 0 => (
                    format!("{} / {}", size_text(done), size_text(t)),
                    done as f32 / t as f32,
                ),
                _ => (size_text(done), -1.0),
            };
            tell(&s2, 2, h2.clone(), detail, p);
        };
        let url = rel.setup_asset_url();
        let filename = setup_filename(&rel.version);
        match selfupdate::download_artifact(
            &client,
            &url,
            &filename,
            &rel.manifest.sha256,
            &rel.sums_text,
            &update_dir(),
            &progress,
        )
        .await
        {
            Ok(setup) => {
                tell(
                    &shared,
                    3,
                    tf("Installing version {}…", &[&rel.version]),
                    t("Ezoo Download closes now and opens again when it is installed."),
                    -1.0,
                );
                // a moment to read it, then the setup runs in front of the user
                tokio::time::sleep(Duration::from_millis(1200)).await;
                shared.lock().unwrap().install = Some((setup, rel.manifest.sha256.clone()));
                finish(&shared);
            }
            Err(e) => {
                tell(
                    &shared,
                    4,
                    t("Could not get the update"),
                    format!("{e:#}"),
                    -1.0,
                );
                finish(&shared);
            }
        }
    });
}
