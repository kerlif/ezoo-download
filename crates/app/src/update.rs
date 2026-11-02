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
//! downloads the matching package into ~/Downloads, hash-verified, and
//! shows where the file is; a Flatpak is notify-only, with a link to the
//! release page; an install whose kind cannot be told is notify-only too,
//! which is always safe.

use crate::i18n::{is_arabic, t, tf};
use crate::message;
use crate::open_window;
#[cfg(windows)]
use crate::UpdateDialog;
use crate::{Ctx, Look, Theme, Ui, UpdateNoticeDialog};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use swiftget_engine::config::Settings;
use swiftget_engine::http::{self, ProxyConfig};
use swiftget_engine::selfupdate;
use swiftget_engine::{Manager, Status};

/// One check / download at a time (automatic or from the menu).
static BUSY: AtomicBool = AtomicBool::new(false);

/// Versions the user already declined (automatic checks) or was already
/// told about (notify-only), this run: asking again every 24 hours about
/// the same release would be nagging. A newer version asks again.
static DECLINED: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
#[cfg(not(windows))]
static NOTIFIED: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Versions the user put off with «Later» in the update notice, this
/// run: the notice is shown at most once per run for a given version
/// (hotfix §6, Q48 — including when a mid-run 24h check finds the
/// update). No durable state: the next launch re-offers if the update
/// is still pending, and a newer version re-offers at once.
static LATER: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// The notice is shown unless this run already saw this version declined
/// or put off with Later. A manual check always shows it, as it bypasses
/// DECLINED today.
fn notice_suppressed(version: &str, manual: bool) -> bool {
    !manual && (already_seen(&DECLINED, version) || already_seen(&LATER, version))
}

/// The running copy, named for a sentence: every update string uses this
/// one helper, so no message can offer the wrong package again (hotfix
/// §3.1b). The pure core is `package_label_for`, so tests can cover
/// every kind without faking the process environment.
pub(crate) fn package_label() -> &'static str {
    #[cfg(windows)]
    {
        if selfupdate::installed() {
            "the Windows setup"
        } else {
            "this copy"
        }
    }
    #[cfg(not(windows))]
    {
        package_label_for(selfupdate::detect_package_current())
    }
}

/// Unix-only helpers below are compiled on Windows too, but only for
/// tests: they are dead in the Windows production build (every call
/// site is `#[cfg(not(windows))]`), yet the unit tests that cover them
/// run on Windows as well. `cfg(any(not(windows), test))` keeps them
/// for both; the two items with no test callers (`read_os_release`,
/// `downloads_folder`) are `#[cfg(not(windows))]` only.
#[cfg(any(not(windows), test))]
fn package_label_for(kind: selfupdate::PackageKind) -> &'static str {
    match kind {
        selfupdate::PackageKind::AppImage => "AppImage",
        selfupdate::PackageKind::Flatpak => "the Flatpak",
        selfupdate::PackageKind::Managed => "the .deb/.rpm package",
        selfupdate::PackageKind::Unknown => "this copy",
    }
}

/// The running copy, named for the About dialog's first line (hotfix
/// §3.1a, Q45): "Version 1.1.1 (AppImage)" / "(.deb)" / "(.rpm)" /
/// "(Flatpak)" / "(Windows setup)" / "(portable copy)".
pub(crate) fn package_short_label() -> String {
    #[cfg(windows)]
    {
        if selfupdate::installed() {
            "Windows setup".to_string()
        } else {
            "portable copy".to_string()
        }
    }
    #[cfg(not(windows))]
    {
        short_label_for(
            selfupdate::detect_package_current(),
            managed_flavour(&read_os_release()),
        )
        .to_string()
    }
}

#[cfg(any(not(windows), test))]
fn short_label_for(kind: selfupdate::PackageKind, flavour: ManagedFlavour) -> &'static str {
    match kind {
        selfupdate::PackageKind::AppImage => "AppImage",
        selfupdate::PackageKind::Flatpak => "Flatpak",
        selfupdate::PackageKind::Managed => match flavour {
            ManagedFlavour::Deb => ".deb",
            ManagedFlavour::Rpm => ".rpm",
        },
        selfupdate::PackageKind::Unknown => "portable copy",
    }
}

/// Which system package a Managed install takes (hotfix §3.1c, Q40).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(any(not(windows), test))]
pub(crate) enum ManagedFlavour {
    Deb,
    Rpm,
}

/// The flavour from an `/etc/os-release` text: `.deb` on Debian/Ubuntu
/// likes, `.rpm` on RHEL/Fedora/SUSE likes, `.deb` when unreadable
/// (the fallback — .deb is the package this project ships first).
#[cfg(any(not(windows), test))]
fn managed_flavour(os_release: &str) -> ManagedFlavour {
    let mut tokens: Vec<String> = Vec::new();
    for line in os_release.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() == "ID" || key.trim() == "ID_LIKE" {
            tokens.extend(
                value
                    .trim()
                    .trim_matches('"')
                    .split_whitespace()
                    .map(|t| t.to_ascii_lowercase()),
            );
        }
    }
    if tokens
        .iter()
        .any(|t| t == "rhel" || t == "fedora" || t == "suse" || t == "opensuse")
    {
        ManagedFlavour::Rpm
    } else {
        ManagedFlavour::Deb
    }
}

/// `/etc/os-release` as text, empty when it cannot be read.
#[cfg(not(windows))]
fn read_os_release() -> String {
    std::fs::read_to_string("/etc/os-release").unwrap_or_default()
}

/// The release asset matching a Managed install's flavour, if the
/// release published one.
#[cfg(any(not(windows), test))]
fn managed_asset(
    rel: &selfupdate::CheckedRelease,
    flavour: ManagedFlavour,
) -> Option<(String, String)> {
    match flavour {
        ManagedFlavour::Deb => rel.deb_asset(),
        ManagedFlavour::Rpm => rel.rpm_asset(),
    }
}

/// The folder a Managed update is downloaded into: ~/Downloads, or the
/// download folder from Settings when HOME is not set.
#[cfg(not(windows))]
fn downloads_folder(mgr: &Manager) -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    downloads_folder_for(home.as_deref(), &mgr.settings().download_dir)
}

#[cfg(any(not(windows), test))]
fn downloads_folder_for(home: Option<&std::path::Path>, fallback: &std::path::Path) -> PathBuf {
    home.map(|h| h.join("Downloads"))
        .unwrap_or_else(|| fallback.to_path_buf())
}

/// Automatic rounds run only when the user left them on (upstream #49):
/// `Settings.update_check`, Options > General. Help > Check for updates
/// never consults this flag, so a manual check still works with the
/// setting off.
pub(crate) fn auto_checks_enabled(settings: &Settings) -> bool {
    settings.update_check
}

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
    if let Ok(t) = std::fs::read_to_string(update_dir().join("tries.txt")) {
        let v = t.split(' ').next().unwrap_or_default().to_string();
        if !selfupdate::is_newer(&v, selfupdate::current()) {
            // that version (or a newer one) is what runs now
            let _ = std::fs::remove_file(update_dir().join("tries.txt"));
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
            if !auto_checks_enabled(&mgr.settings()) {
                // off in Options (upstream #49): look again in a while,
                // in case it is turned back on; Help > Check for updates
                // still works
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

/// How many times the setup of `version` was started already (it closes
/// the program and should open the new version again; when the program
/// opens as the old version again, that setup did not get through).
/// Windows-only: the only caller is `windows_available`.
#[cfg(windows)]
fn tries_of(version: &str) -> u32 {
    std::fs::read_to_string(update_dir().join("tries.txt"))
        .ok()
        .and_then(|t| {
            let (v, n) = t.trim().split_once(' ')?;
            (v == version).then(|| n.parse().ok()).flatten()
        })
        .unwrap_or(0)
}

#[cfg(windows)]
fn count_try(version: &str) {
    let n = tries_of(version) + 1;
    let _ = std::fs::create_dir_all(update_dir());
    let _ = std::fs::write(update_dir().join("tries.txt"), format!("{version} {n}"));
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

/// A verified newer release: the notice first (highlights + «Update
/// now» / «Later»), then the platform arm — nothing is downloaded
/// before the user says Update now. `manual` is Help > Check for
/// updates (Linux): it bypasses the declined/notified memory and reports
/// failures to the user, who asked. Returns how the automatic loop
/// should schedule the next check.
async fn handle_available(
    mgr: &Arc<Manager>,
    client: &reqwest::Client,
    rel: &selfupdate::CheckedRelease,
    manual: bool,
) -> RoundOutcome {
    if notice_suppressed(&rel.version, manual) {
        return RoundOutcome::Done;
    }
    match ask_notice(rel).await {
        NoticeChoice::Later => {
            mark_seen(&LATER, &rel.version);
            return RoundOutcome::Done;
        }
        NoticeChoice::UpdateNow => {}
    }
    #[cfg(windows)]
    {
        return windows_available(mgr, client, rel, manual, true).await;
    }
    #[cfg(not(windows))]
    {
        return linux_available(mgr, client, rel, manual, true).await;
    }
}

/// What the user answered in the update notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NoticeChoice {
    UpdateNow,
    Later,
}

thread_local! {
    /// The notice window open now, kept alive while it is shown.
    static NOTICE_WIN: RefCell<Option<UpdateNoticeDialog>> = const { RefCell::new(None) };
}

/// Show the update notice and wait for the answer. «Update now» /
/// «Later» come back through a oneshot; closing the window, or a dialog
/// that cannot be shown at all, counts as Later (nothing is downloaded
/// without an explicit Update now).
async fn ask_notice(rel: &selfupdate::CheckedRelease) -> NoticeChoice {
    let highlights = selfupdate::release_highlights(&rel.notes, is_arabic());
    let heading = tf("Version {} is available", &[&rel.version]);
    let link = rel.release_url.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let shared = Arc::new(std::sync::Mutex::new(Some(tx)));
    let posted = slint::invoke_from_event_loop(move || {
        show_update_notice(heading, highlights, link, shared);
    });
    if posted.is_err() {
        return NoticeChoice::Later;
    }
    rx.await.unwrap_or(NoticeChoice::Later)
}

/// Build the notice window on the UI thread: heading, highlights (or a
/// generic "see the release page" line when the release body had none),
/// the full-notes link, and the two buttons. `tx` answers exactly once,
/// whatever closes the window.
fn show_update_notice(
    heading: String,
    highlights: Vec<String>,
    link: String,
    tx: Arc<std::sync::Mutex<Option<tokio::sync::oneshot::Sender<NoticeChoice>>>>,
) {
    let Ok(d) = UpdateNoticeDialog::new() else {
        answer_notice(&tx, NoticeChoice::Later);
        return;
    };
    // the saved theme/language, not the Slint defaults, from the start
    if let Some(ui) = crate::lazy_ui() {
        crate::set_theme_of(d.global::<Theme>(), &Look::of(&ui.ctx.mgr.settings()));
    }
    d.set_heading(heading.into());
    d.set_rtl(is_arabic());
    let items: Vec<SharedString> = if highlights.is_empty() {
        vec![
            t("See the release notes on the release page for what changed in this version.").into(),
        ]
    } else {
        highlights.into_iter().map(|l| l.into()).collect()
    };
    d.set_items(ModelRc::new(VecModel::from(items)));
    d.set_link(link.clone().into());
    let answer = move |choice: NoticeChoice| {
        answer_notice(&tx, choice);
    };
    {
        let (w, answer) = (d.as_weak(), answer.clone());
        d.on_update_now(move || {
            if let Some(d) = w.upgrade() {
                let _ = d.hide();
            }
            answer(NoticeChoice::UpdateNow);
        });
    }
    {
        let (w, answer) = (d.as_weak(), answer.clone());
        d.on_later(move || {
            if let Some(d) = w.upgrade() {
                let _ = d.hide();
            }
            answer(NoticeChoice::Later);
        });
    }
    {
        let link = link.clone();
        d.on_open_link(move || {
            let _ = open::that(&link);
        });
    }
    {
        let (w, answer) = (d.as_weak(), answer.clone());
        d.on_close_window(move || {
            if let Some(d) = w.upgrade() {
                let _ = d.hide();
            }
            answer(NoticeChoice::Later);
        });
    }
    {
        let w = d.as_weak();
        d.window().on_close_requested(move || {
            if let Some(d) = w.upgrade() {
                let _ = d.hide();
            }
            answer(NoticeChoice::Later);
            slint::CloseRequestResponse::HideWindow
        });
    }
    open_window!(d);
    NOTICE_WIN.with(|x| *x.borrow_mut() = Some(d));
}

/// Answer the waiting `ask_notice` exactly once, then let the window go.
fn answer_notice(
    tx: &Arc<std::sync::Mutex<Option<tokio::sync::oneshot::Sender<NoticeChoice>>>>,
    choice: NoticeChoice,
) {
    if let Ok(mut g) = tx.lock() {
        if let Some(tx) = g.take() {
            let _ = tx.send(choice);
        }
    }
    slint::Timer::single_shot(Duration::from_millis(1), || {
        NOTICE_WIN.with(|x| *x.borrow_mut() = None);
    });
}

/// The Windows half of `handle_available`. The notice already asked
/// («Update now»), so `confirmed` is always true here; the install
/// question after the download stays — it confirms the install itself,
/// not the update.
#[cfg(windows)]
async fn windows_available(
    mgr: &Arc<Manager>,
    client: &reqwest::Client,
    rel: &selfupdate::CheckedRelease,
    manual: bool,
    _confirmed: bool,
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
            "Version {} was downloaded and checked. Install {} now? Ezoo Download closes and opens again when it is installed.",
            &[&rel.version, &package_label()],
        );
        if !ask_user(question).await {
            mark_seen(&DECLINED, &rel.version);
            return RoundOutcome::Done;
        }
        while downloading(mgr) {
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
        let sha = rel.manifest.sha256.clone();
        // upstream 1.0.8: a setup started twice that still did not get
        // through is not started a third time by itself; Help > Check
        // for updates installs it in front of the user instead (its
        // own window, check_now_windows, does not go through here)
        let tries = tries_of(&rel.version);
        if tries >= 2 {
            crate::note(&format!(
                "update {} did not install after {tries} tries; waiting for the user",
                rel.version
            ));
            return RoundOutcome::Done;
        }
        count_try(&rel.version);
        // visible: the setup runs with its own progress window
        // (upstream 1.0.8: silent closing looked like a crash)
        let _ = slint::invoke_from_event_loop(move || apply_as(setup, sha, true));
        RoundOutcome::Done
    }
}

/// The Linux half of `handle_available` (AppImage replace / Managed
/// download / Flatpak notify-only). `confirmed` is the notice's
/// «Update now»: when true the second ask is dropped, but installing
/// still waits until no download is running.
#[cfg(not(windows))]
async fn linux_available(
    mgr: &Arc<Manager>,
    client: &reqwest::Client,
    rel: &selfupdate::CheckedRelease,
    manual: bool,
    confirmed: bool,
) -> RoundOutcome {
    match selfupdate::detect_package_current() {
        selfupdate::PackageKind::AppImage => {
            let Some(current) = selfupdate::current_appimage_path() else {
                return notify_only(mgr, client, rel, manual).await;
            };
            if !manual && already_seen(&DECLINED, &rel.version) {
                return RoundOutcome::Done;
            }
            let Some((filename, url)) = rel.appimage_asset() else {
                return notify_only(mgr, client, rel, manual).await;
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
                "Version {} was downloaded and checked. Replace {} now? The new version starts the next time you open Ezoo Download.",
                &[&rel.version, &package_label()],
            );
            // the notice already asked («Update now»): the second ask is
            // dropped, but the replace still waits until no download is
            // running (below)
            if !confirmed && !ask_user(question).await {
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
            // «Update now» in the notice is the confirmation: there is
            // nothing to download here, the release page is where the
            // update happens (the app never runs the package manager)
            if confirmed {
                let _ = open::that(&rel.release_url);
            } else {
                let text = format!(
                    "{}\n\n{}\n{}",
                    tf("Version {} is available", &[&rel.version]),
                    t("Open the release page in the browser?"),
                    rel.release_url
                );
                if ask_user(text).await {
                    let _ = open::that(&rel.release_url);
                }
            }
            RoundOutcome::Done
        }
        selfupdate::PackageKind::Managed | selfupdate::PackageKind::Unknown => {
            notify_only(mgr, client, rel, manual).await
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

/// Notify-only: for a Managed install the notice's «Update now»
/// downloads the matching package into ~/Downloads (see
/// `download_managed`); anything else keeps the releases-page message.
/// Once per version per run for automatic checks, always for a manual
/// check.
#[cfg(not(windows))]
async fn notify_only(
    mgr: &Arc<Manager>,
    client: &reqwest::Client,
    rel: &selfupdate::CheckedRelease,
    manual: bool,
) -> RoundOutcome {
    if !manual && already_seen(&NOTIFIED, &rel.version) {
        return RoundOutcome::Done;
    }
    mark_seen(&NOTIFIED, &rel.version);
    match selfupdate::detect_package_current() {
        selfupdate::PackageKind::Managed | selfupdate::PackageKind::Unknown => {
            download_managed(mgr, client, rel, manual).await
        }
        _ => {
            tell_user(format!(
                "{}\n\n{}",
                tf("Version {} is available", &[&rel.version]),
                tf(
                    "No matching package file was published for this release ({}). See the release page:\n{}",
                    &[&package_label(), &rel.release_url],
                ),
            ));
            RoundOutcome::Done
        }
    }
}

/// The Managed arm of the update (hotfix §3.1c, Q40): download the
/// matching asset — `.deb` on Debian/Ubuntu-likes, `.rpm` elsewhere —
/// hash-verified against the release sums (the same verification the
/// AppImage arm uses), into ~/Downloads, then open the folder so the
/// user finds the file. The app never runs the package manager, so the
/// user installs the file themselves. `download_artifact` wipes every
/// other file in its target dir, so the asset is staged in the update
/// dir and only then placed in Downloads.
#[cfg(not(windows))]
async fn download_managed(
    mgr: &Arc<Manager>,
    client: &reqwest::Client,
    rel: &selfupdate::CheckedRelease,
    manual: bool,
) -> RoundOutcome {
    let flavour = managed_flavour(&read_os_release());
    let Some((filename, url)) = managed_asset(rel, flavour) else {
        tell_user(format!(
            "{}\n\n{}",
            tf("Version {} is available", &[&rel.version]),
            tf(
                "No matching package file was published for this release ({}). See the release page:\n{}",
                &[&package_label(), &rel.release_url],
            ),
        ));
        return RoundOutcome::Done;
    };
    let staged = match selfupdate::download_artifact(
        client,
        &url,
        &filename,
        &selfupdate::sums_hash(&rel.sums_text, &filename).unwrap_or_default(),
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
    let folder = downloads_folder(mgr);
    let dest = folder.join(&filename);
    match std::fs::create_dir_all(&folder).and_then(|()| std::fs::copy(&staged, &dest)) {
        Ok(_) => {
            let _ = std::fs::remove_file(&staged);
            // show the user where the file is
            let _ = open::that_detached(&folder);
            tell_user(tf(
                "Version {} was downloaded to:\n{}",
                &[&rel.version, &dest.display().to_string()],
            ));
        }
        Err(e) => {
            tell_user(tf(
                "The update was downloaded, but it could not be placed in {}:\n{}",
                &[&folder.display().to_string(), &format!("{e:#}")],
            ));
        }
    }
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
                "Ezoo Download ({}) is up to date (version {}).",
                &[&package_label(), &selfupdate::current()],
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
        tf(
            "This is version {} ({}).",
            &[&selfupdate::current(), &package_label()],
        ),
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
                tf(
                    "This is version {} ({}).",
                    &[&selfupdate::current(), &package_label()],
                ),
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
                    tf(
                        "This is version {} ({}).",
                        &[&selfupdate::current(), &package_label()],
                    ),
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
                        "Version {} ({}) is the newest version.",
                        &[&selfupdate::current(), &package_label()],
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AboutDialog;
    use slint::Model;
    use swiftget_engine::selfupdate::{CheckedRelease, Release};

    /// A release with a .deb, an .rpm and an AppImage asset, for the
    /// Managed-asset and label tests.
    fn fake_release(version: &str) -> CheckedRelease {
        CheckedRelease {
            version: version.to_string(),
            tag: format!("v{version}"),
            release_url: "https://github.com/kerlif/ezoo-download/releases/tag/v9.9.9".to_string(),
            manifest: Release {
                version: version.to_string(),
                url: String::new(),
                sha256: String::new(),
                sig: String::new(),
            },
            sums_text: String::new(),
            sums_signature_verified: false,
            assets: vec![
                (
                    "EzooDownload-9.9.9-amd64.deb".into(),
                    "https://example.invalid/d.deb".into(),
                ),
                (
                    "EzooDownload-9.9.9.x86_64.rpm".into(),
                    "https://example.invalid/r.rpm".into(),
                ),
                (
                    "EzooDownload-9.9.9-x86_64.AppImage".into(),
                    "https://example.invalid/a.AppImage".into(),
                ),
            ],
            notes: String::new(),
        }
    }

    #[test]
    fn package_labels_name_the_running_copy() {
        use swiftget_engine::selfupdate::PackageKind as K;
        assert_eq!(package_label_for(K::AppImage), "AppImage");
        assert_eq!(package_label_for(K::Flatpak), "the Flatpak");
        assert_eq!(package_label_for(K::Managed), "the .deb/.rpm package");
        assert_eq!(package_label_for(K::Unknown), "this copy");
        // the live label is always one of the four
        assert!(matches!(
            package_label(),
            "AppImage"
                | "the Flatpak"
                | "the .deb/.rpm package"
                | "this copy"
                | "the Windows setup"
        ));
    }

    #[test]
    fn short_labels_for_about() {
        use swiftget_engine::selfupdate::PackageKind as K;
        assert_eq!(
            short_label_for(K::AppImage, ManagedFlavour::Deb),
            "AppImage"
        );
        assert_eq!(short_label_for(K::Flatpak, ManagedFlavour::Deb), "Flatpak");
        assert_eq!(short_label_for(K::Managed, ManagedFlavour::Deb), ".deb");
        assert_eq!(short_label_for(K::Managed, ManagedFlavour::Rpm), ".rpm");
        assert_eq!(
            short_label_for(K::Unknown, ManagedFlavour::Deb),
            "portable copy"
        );
        assert!(!package_short_label().is_empty());
    }

    #[test]
    fn managed_flavour_reads_os_release() {
        let ubuntu = "NAME=\"Ubuntu\"\nID=ubuntu\nID_LIKE=debian\n";
        assert_eq!(managed_flavour(ubuntu), ManagedFlavour::Deb);
        let debian = "ID=debian\n";
        assert_eq!(managed_flavour(debian), ManagedFlavour::Deb);
        let fedora = "ID=fedora\nID_LIKE=fedora\n";
        assert_eq!(managed_flavour(fedora), ManagedFlavour::Rpm);
        let rhel = "ID=\"rhel\"\nID_LIKE=\"fedora\"\n";
        assert_eq!(managed_flavour(rhel), ManagedFlavour::Rpm);
        let suse = "ID=\"opensuse-leap\"\nID_LIKE=\"suse opensuse\"\n";
        assert_eq!(managed_flavour(suse), ManagedFlavour::Rpm);
        // unreadable or unrecognised: .deb is the fallback
        assert_eq!(managed_flavour(""), ManagedFlavour::Deb);
        assert_eq!(managed_flavour("ID=arch\n"), ManagedFlavour::Deb);
        assert_eq!(
            managed_flavour("garbage without equals\n"),
            ManagedFlavour::Deb
        );
    }

    #[test]
    fn managed_asset_picks_the_matching_package() {
        let rel = fake_release("9.9.9");
        assert_eq!(
            managed_asset(&rel, ManagedFlavour::Deb),
            Some((
                "EzooDownload-9.9.9-amd64.deb".into(),
                "https://example.invalid/d.deb".into()
            ))
        );
        assert_eq!(
            managed_asset(&rel, ManagedFlavour::Rpm),
            Some((
                "EzooDownload-9.9.9.x86_64.rpm".into(),
                "https://example.invalid/r.rpm".into()
            ))
        );
        // no matching asset: None, never a wrong-kind asset
        let bare = CheckedRelease {
            assets: vec![("EzooDownload-Setup.exe".into(), "u".into())],
            ..rel
        };
        assert_eq!(managed_asset(&bare, ManagedFlavour::Deb), None);
        assert_eq!(managed_asset(&bare, ManagedFlavour::Rpm), None);
    }

    #[test]
    fn downloads_folder_is_home_downloads() {
        assert_eq!(
            downloads_folder_for(
                Some(std::path::Path::new("/home/tensi")),
                std::path::Path::new("/fallback")
            ),
            PathBuf::from("/home/tensi/Downloads")
        );
        assert_eq!(
            downloads_folder_for(None, std::path::Path::new("/fallback")),
            PathBuf::from("/fallback")
        );
    }

    #[test]
    fn later_suppresses_the_notice_until_a_newer_version() {
        // a unique version per test: the gate statics live for the whole
        // test process, so tests must not share version strings
        let v = "9.9.9-h2-later";
        assert!(!notice_suppressed(v, false));
        mark_seen(&LATER, v);
        assert!(notice_suppressed(v, false));
        // a newer version re-offers at once
        assert!(!notice_suppressed("9.9.10-h2-later", false));
        // a manual check bypasses the suppression, as it bypasses
        // DECLINED today
        assert!(!notice_suppressed(v, true));
    }

    #[test]
    fn declined_versions_suppress_the_notice_too() {
        let v = "9.9.9-h2-declined";
        assert!(!notice_suppressed(v, false));
        mark_seen(&DECLINED, v);
        assert!(notice_suppressed(v, false));
        assert!(!notice_suppressed(v, true));
    }

    #[test]
    fn auto_checks_follow_the_setting() {
        // upstream #49: the setting already exists; the spawn loop reads
        // exactly this predicate (see `spawn`)
        let mut s = Settings::default();
        assert!(s.update_check);
        assert!(auto_checks_enabled(&s));
        s.update_check = false;
        assert!(!auto_checks_enabled(&s));
    }

    /// A second `handle_available()` in the same run shows nothing once
    /// the user pressed Later: the gate returns Done before any dialog
    /// or network. (The fresh-launch half is by construction: LATER is
    /// in-memory only, so a new process re-offers.)
    #[tokio::test]
    async fn handle_available_is_done_when_the_notice_is_suppressed() {
        let dir = std::env::temp_dir().join(format!("ezoo-h2-test-{}-handle", std::process::id()));
        let mgr =
            Manager::new(tokio::runtime::Handle::current(), dir.clone()).expect("Manager::new");
        let client = reqwest::Client::new();
        let version = "9.9.9-h2-handle";
        mark_seen(&LATER, version);
        let rel = fake_release(version);
        let outcome = handle_available(&mgr, &client, &rel, false).await;
        assert!(matches!(outcome, RoundOutcome::Done));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A display is needed to open a window; headless CI skips these
    /// (they run under Xvfb in the smoke test instead).
    fn display_available() -> bool {
        std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some()
    }

    /// Both dialogs are opened in this one test, on this one thread:
    /// the Slint platform is bound to the thread that first initialized
    /// it, so a second test on another thread cannot create a window
    /// afterwards ("The Slint platform was initialized in another
    /// thread"). Headless CI skips this (it runs under Xvfb in the
    /// smoke test instead).
    #[test]
    fn notice_and_about_dialogs_open_as_their_own_windows() {
        if !display_available() {
            eprintln!("SKIP: no display, the dialogs cannot open here");
            return;
        }
        let Ok(about) = AboutDialog::new() else {
            eprintln!("SKIP: no Slint backend");
            return;
        };
        about.set_version("1.1.1".into());
        about.set_package("AppImage".into());
        about.set_path("/tmp/EzooDownload".into());
        assert_eq!(about.get_version(), "1.1.1");
        assert_eq!(about.get_package(), "AppImage");
        assert_eq!(about.get_path(), "/tmp/EzooDownload");
        let _ = about.show();
        assert!(about.window().is_visible());
        // About is open, and the notice is a second, separate window,
        // with highlights and both buttons
        let notice = UpdateNoticeDialog::new().expect("the notice dialog opens on the same thread");
        notice.set_heading("Version 9.9.9 is available".into());
        notice.set_rtl(false);
        let items: Vec<SharedString> = vec!["First highlight".into(), "Second highlight".into()];
        notice.set_items(ModelRc::new(VecModel::from(items)));
        notice.set_link("https://example.invalid/releases".into());
        assert_eq!(notice.get_heading(), "Version 9.9.9 is available");
        assert_eq!(notice.get_items().iter().count(), 2);
        assert_eq!(notice.get_link(), "https://example.invalid/releases");
        let _ = notice.show();
        assert!(notice.window().is_visible());
        // both windows are up at once: two separate floating windows,
        // not one window re-used
        assert!(about.window().is_visible());
        let _ = notice.hide();
        let _ = about.hide();
    }
}
