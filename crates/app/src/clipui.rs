//! "Cut a part of a video": the window where a file (or a web
//! address) and the from / to times are chosen, with a look at the
//! part before it is cut. A file is cut on the spot; an address
//! becomes a download that fetches only the part (see
//! `Manager::run_clip`).

use crate::i18n::{t, tf};
use crate::open_window;
use crate::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use swiftget_engine::clip::{self, CutInput};
use swiftget_engine::{Clip, RequestHeaders};
use tokio_util::sync::CancellationToken;

/// What a cut in flight shares with the dialog: its cancel token
/// and its polling timer (the progress itself lives in the timer
/// closure's own arcs).
struct Running {
    cancel: CancellationToken,
    timer: slint::Timer,
}

#[derive(Default)]
struct State {
    running: Option<Running>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn is_web(u: &str) -> bool {
    let u = u.trim();
    u.starts_with("https://") || u.starts_with("http://")
}

/// The dialog, created on first use and wired exactly once.
pub(crate) fn ensure_clip(ui: &Ui) -> Option<ClipDialog> {
    if let Some(d) = ui.clip_dlg.upgrade() {
        return Some(d);
    }
    let Ok(d) = ClipDialog::new() else {
        return None;
    };
    crate::set_theme_of(
        d.global::<Theme>(),
        &crate::Look::of(&ui.ctx.mgr.settings()),
    );
    wire_clip(ui, &d);
    ui.clip_dlg.store(d);
    ui.clip_dlg.upgrade()
}

/// Open the window, prefilled with a file path or an address.
pub(crate) fn open(ui: &Ui, source: Option<String>) {
    let Some(d) = ensure_clip(ui) else { return };
    if let Some(s) = source {
        d.set_source(s.into());
        if d.get_to_text().is_empty() {
            d.invoke_fetch();
        }
    } else if d.get_source().is_empty() {
        // a video address in the clipboard is the likely one
        let clip = arboard::Clipboard::new()
            .ok()
            .and_then(|mut c| c.get_text().ok())
            .unwrap_or_default();
        if is_web(&clip) && !clip.contains(char::is_whitespace) {
            d.set_source(clip.trim().into());
        }
    }
    open_window!(d);
}

/// Open it for a finished download: cut a part out of that file.
pub(crate) fn open_for_download(ui: &Ui, id: u64) {
    let Some(v) = ui.ctx.mgr.get(id) else { return };
    if v.status != Status::Completed {
        return;
    }
    open(ui, Some(v.save_path.to_string_lossy().into_owned()));
}

/// From / to as they are now, or what is wrong with them.
fn chosen(d: &ClipDialog) -> Result<Clip, String> {
    let from = clip::parse_time(&d.get_from_text())
        .ok_or_else(|| t("The start time does not parse: write seconds or mm:ss."))?;
    let to = clip::parse_time(&d.get_to_text())
        .ok_or_else(|| t("The end time does not parse: write seconds or mm:ss."))?;
    let dur = d.get_duration() as f64;
    if dur > 0.0 && from >= dur {
        return Err(t("The start is after the end of the video."));
    }
    if dur > 0.0 && to > dur {
        return Err(t("The end is after the end of the video."));
    }
    Clip::new(from, to, d.get_exact()).map_err(|e| t(&e))
}

/// What will be cut, from the probe: "720p · 1280×720 · h264", or
/// "Sound only" when there is no picture track to cut.
fn quality_text(pic: Option<clip::Picture>) -> String {
    match pic {
        Some(p) if p.height > 0 => {
            let tag = if p.height >= 4320 {
                "8K".to_string()
            } else if p.height >= 2160 {
                "4K".to_string()
            } else if p.height >= 1440 {
                "2K".to_string()
            } else {
                format!("{}p", p.height)
            };
            tf(
                "Quality: {}",
                &[&format!("{tag} · {}×{} · {}", p.width, p.height, p.codec)],
            )
        }
        Some(p) if !p.codec.is_empty() => tf("Quality: {}", &[&p.codec]),
        _ => t("Sound only (no video track)"),
    }
}

/// A free name next to `source` for the clip.
fn output_path(source: &Path, clip: &Clip) -> PathBuf {
    let name = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "video.mp4".into());
    let dir = source.parent().unwrap_or_else(|| std::path::Path::new("."));
    let wanted = swiftget_engine::filename::sanitize(&swiftget_engine::clip_name(&name, clip));
    let mut out = dir.join(&wanted);
    let mut n = 2;
    while out.exists() {
        let (stem, ext) = swiftget_engine::filename::split_ext(&wanted);
        let next = if ext.is_empty() {
            format!("{stem} ({n})")
        } else {
            format!("{stem} ({n}).{ext}")
        };
        out = dir.join(next);
        n += 1;
    }
    out
}

fn set_status(d: &ClipDialog, text: &str) {
    d.set_status(text.into());
    d.set_progress(-1.0);
    d.set_busy(false);
}

fn wire_clip(ui: &Ui, d: &ClipDialog) {
    {
        let w = d.as_weak();
        d.on_browse(move || {
            let Some(d) = w.upgrade() else { return };
            let mut dlg = crate::winfx::file_box();
            dlg = dlg.add_filter(
                "Videos",
                &["mp4", "mkv", "webm", "mov", "avi", "mp3", "m4a"],
            );
            if let Some(p) = dlg.pick_file() {
                d.set_source(p.to_string_lossy().into_owned().into());
                d.invoke_fetch();
            }
        });
    }
    {
        // "Read length": how long the video plays, so the end can
        // default to it and the times can be checked
        let ui = ui.clone();
        let w = d.as_weak();
        d.on_fetch(move || {
            let Some(d) = w.upgrade() else { return };
            let source = d.get_source().trim().to_string();
            if source.is_empty() {
                return;
            }
            d.set_busy(true);
            d.set_status(t("Reading the video…").into());
            d.set_quality_text("".into());
            let mgr = ui.ctx.mgr.clone();
            let rt = ui.ctx.rt.clone();
            let ua = mgr.settings().user_agent.clone();
            let w2 = w.clone();
            rt.spawn(async move {
                let looked = if is_web(&source) {
                    mgr.clip_look(&source, RequestHeaders::default(), ua).await
                } else {
                    match swiftget_engine::mux::find_ffmpeg() {
                        Some(ffmpeg) => {
                            clip::look(&ffmpeg, &CutInput::File(PathBuf::from(&source))).await
                        }
                        None => Err(anyhow::anyhow!("ffmpeg is missing")),
                    }
                };
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(d) = w2.upgrade() else { return };
                    match looked {
                        Ok((dur, pic)) => {
                            d.set_quality_text(quality_text(pic).into());
                            if let Some(dur) = dur {
                                d.set_duration(dur as f32);
                                d.set_duration_text(
                                    tf("Length: {}", &[&clip::format_time(dur)]).into(),
                                );
                                if d.get_to_text().trim().is_empty() {
                                    d.set_to_text(clip::format_time(dur).into());
                                }
                                d.set_status("".into());
                            } else {
                                d.set_status(
                                    t("The length could not be read, but the cut can still be tried.").into(),
                                );
                            }
                        }
                        Err(e) => {
                            d.set_status(tf("Could not read the video: {}", &[&format!("{e:#}")]).into());
                        }
                    }
                    d.set_busy(false);
                });
            });
        });
    }
    {
        // "Preview": a quick cut of at most the first minute, opened
        // in the system player
        let ui = ui.clone();
        let w = d.as_weak();
        d.on_preview(move || {
            let Some(d) = w.upgrade() else { return };
            let c = match chosen(&d) {
                Ok(c) => c,
                Err(e) => {
                    set_status(&d, &e);
                    return;
                }
            };
            let source = d.get_source().trim().to_string();
            if source.is_empty() {
                return;
            }
            d.set_busy(true);
            d.set_status(t("Cutting the preview…").into());
            let mgr = ui.ctx.mgr.clone();
            let rt = ui.ctx.rt.clone();
            let ua = mgr.settings().user_agent.clone();
            let cancel = CancellationToken::new();
            STATE.with(|s| {
                s.borrow_mut().running = Some(Running {
                    cancel: cancel.clone(),
                    timer: slint::Timer::default(),
                });
            });
            let w2 = w.clone();
            rt.spawn(async move {
                let r = if is_web(&source) {
                    mgr.preview_url(&source, RequestHeaders::default(), ua, &c, &cancel)
                        .await
                } else {
                    mgr.preview_clip(CutInput::File(PathBuf::from(&source)), &c, &cancel)
                        .await
                };
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(d) = w2.upgrade() else { return };
                    STATE.with(|s| s.borrow_mut().running = None);
                    match r {
                        Ok(path) => {
                            d.set_busy(false);
                            d.set_status(t("Preview opened in your player.").into());
                            let _ = open::that_detached(path);
                        }
                        Err(e) => {
                            set_status(
                                &d,
                                &tf("Could not cut the preview: {}", &[&format!("{e:#}")]),
                            );
                        }
                    }
                });
            });
        });
    }
    {
        // "Cut part": a file is cut on the spot (with a progress bar
        // in this window); an address becomes a download that fetches
        // only the part
        let ui = ui.clone();
        let w = d.as_weak();
        d.on_cut(move || {
            let Some(d) = w.upgrade() else { return };
            let c = match chosen(&d) {
                Ok(c) => c,
                Err(e) => {
                    set_status(&d, &e);
                    return;
                }
            };
            let source = d.get_source().trim().to_string();
            if source.is_empty() {
                return;
            }
            if is_web(&source) {
                let name = swiftget_engine::filename::from_url(&source)
                    .unwrap_or_else(|| "video.mp4".into());
                let id = ui.ctx.mgr.add(NewDownload {
                    url: source,
                    file_name: Some(name),
                    headers: RequestHeaders::default(),
                    clip: Some(c),
                    start: true,
                    ..Default::default()
                });
                let _ = d.hide();
                crate::show_progress_for_start(&ui, id);
                crate::refresh(&ui);
                return;
            }
            let src_path = PathBuf::from(&source);
            if !src_path.is_file() {
                set_status(&d, &t("That file does not exist."));
                return;
            }
            let out = output_path(&src_path, &c);
            let out_name = out
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            d.set_busy(true);
            d.set_progress(0.0);
            d.set_status(tf("Cutting {}…", &[&out_name]).into());
            let mgr = ui.ctx.mgr.clone();
            let rt = ui.ctx.rt.clone();
            let progress = Arc::new(AtomicU32::new(0));
            let result: Arc<Mutex<Option<Result<clip::Done, String>>>> =
                Arc::new(Mutex::new(None));
            let cancel = CancellationToken::new();
            let p2 = progress.clone();
            let r2 = result.clone();
            let c2 = cancel.clone();
            rt.spawn(async move {
                let r = mgr
                    .cut_local_file(
                        &src_path,
                        &c,
                        &out,
                        &move |f| {
                            p2.store((f.clamp(0.0, 1.0) * 1000.0) as u32, Ordering::Relaxed);
                        },
                        Some(&c2),
                    )
                    .await;
                *r2.lock().unwrap() = Some(r.map_err(|e| format!("{e:#}")));
            });
            let timer = slint::Timer::default();
            let w2 = w.clone();
            let p3 = progress.clone();
            let r3 = result.clone();
            let out_name2 = out_name.clone();
            timer.start(
                slint::TimerMode::Repeated,
                Duration::from_millis(200),
                move || {
                    let Some(d) = w2.upgrade() else { return };
                    d.set_progress(p3.load(Ordering::Relaxed) as f32 / 1000.0);
                    if let Some(r) = r3.lock().unwrap().take() {
                        STATE.with(|s| {
                            if let Some(run) = s.borrow_mut().running.take() {
                                run.timer.stop();
                            }
                        });
                        d.set_busy(false);
                        d.set_progress(-1.0);
                        match r {
                            Ok(clip::Done::AsAsked) => {
                                d.set_status(
                                    tf("Cut saved as {}.", &[&out_name2]).into(),
                                );
                            }
                            Ok(clip::Done::QuickInstead) => {
                                d.set_status(
                                    tf(
                                        "Cut saved as {} — the exact cut was not possible for this video, so it was cut the quick way.",
                                        &[&out_name2],
                                    )
                                    .into(),
                                );
                            }
                            Err(e) => {
                                d.set_status(tf("Could not cut the part: {}", &[&e]).into());
                            }
                        }
                    }
                },
            );
            STATE.with(|s| {
                s.borrow_mut().running = Some(Running {
                    cancel,
                    timer,
                });
            });
        });
    }
    {
        let w = d.as_weak();
        d.on_cancel(move || {
            STATE.with(|s| {
                if let Some(run) = &s.borrow().running {
                    run.cancel.cancel();
                }
            });
            if let Some(d) = w.upgrade() {
                d.set_status(t("Cancelling…").into());
            }
        });
    }
    {
        let w = d.as_weak();
        d.on_close_window(move || {
            STATE.with(|s| {
                if let Some(run) = &s.borrow().running {
                    run.cancel.cancel();
                }
                s.borrow_mut().running = None;
            });
            if let Some(d) = w.upgrade() {
                let _ = d.hide();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_text_names_the_picture() {
        assert_eq!(
            quality_text(Some(clip::Picture {
                codec: "h264".into(),
                width: 1280,
                height: 720
            })),
            "Quality: 720p · 1280×720 · h264"
        );
        assert_eq!(
            quality_text(Some(clip::Picture {
                codec: "hevc".into(),
                width: 3840,
                height: 2160
            })),
            "Quality: 4K · 3840×2160 · hevc"
        );
        assert_eq!(quality_text(None), "Sound only (no video track)");
    }
}
