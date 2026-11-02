//! "Download a playlist": every video of a playlist in one go, in
//! the quality you choose, into its own queue and folder.

use crate::open_window;
use crate::*;

use crate::i18n::{t, tf};

const QUALITIES: [&str; 11] = [
    "best", "2160", "1440", "1080", "720", "480", "360", "240", "144", "audio", "mp3",
];

pub(crate) fn open(ui: &Ui) {
    let Some(d) = ensure_playlist(ui) else {
        return;
    };
    if !d.window().is_visible() {
        // a playlist address in the clipboard is the likely one
        let clip = arboard::Clipboard::new()
            .ok()
            .and_then(|mut c| c.get_text().ok())
            .unwrap_or_default();
        let clip = clip.trim();
        if clip.contains("list=") || clip.contains("/playlist") || clip.contains("/@") {
            d.set_url(clip.into());
        }
        d.set_message(SharedString::new());
    }
    open_window!(d);
    d.window().set_minimized(false);
}

/// The browser's "Download all" on a playlist page: the window with that
/// list, already looking up its videos.
pub(crate) fn open_url(ui: &Ui, url: &str) {
    let Some(d) = ensure_playlist(ui) else {
        return;
    };
    d.set_url(url.into());
    d.set_message(SharedString::new());
    open_window!(d);
    d.window().set_minimized(false);
    if !d.get_busy() {
        d.invoke_fetch();
    }
}

fn fmt_len(secs: u64) -> String {
    if secs == 0 {
        String::new()
    } else if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
    } else {
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}

fn checked_text(rows: &VecModel<LinkRow>) -> String {
    let n = (0..rows.row_count())
        .filter_map(|i| rows.row_data(i))
        .filter(|r| r.checked)
        .count();
    tf("{} of {} videos checked", &[&n, &rows.row_count()])
}

/// The number shown for row `i` of `total` rows: `i + 1` normally,
/// `total - i` when numbering from the oldest video. The list order
/// never changes — newest stays on top, the bottom (oldest) row is #1.
fn row_number(i: usize, total: usize, reverse: bool) -> usize {
    if reverse {
        total - i
    } else {
        i + 1
    }
}

/// The exact file name the download loop will use for a row: the
/// sanitized title, an optional zero-padded number prefix, and the
/// extension. The skip check uses this same computation, so the two
/// can never disagree about the file name.
fn target_file_name(row: &LinkRow, numbered: bool, width: usize, ext: &str) -> String {
    let mut file = filename::sanitize(&row.name);
    if numbered {
        let n: usize = row.text.parse().unwrap_or(0);
        file = format!("{n:0width$} - {file}");
    }
    format!("{file}.{ext}")
}

/// The Playlist dialog, created (and wired exactly once) on first use.
pub(crate) fn ensure_playlist(ui: &Ui) -> Option<PlaylistDialog> {
    if let Some(d) = ui.playlist.upgrade() {
        return Some(d);
    }
    let Ok(d) = PlaylistDialog::new() else {
        return None;
    };
    crate::set_theme_of(
        d.global::<Theme>(),
        &crate::Look::of(&ui.ctx.mgr.settings()),
    );
    wire(ui, &d);
    ui.playlist.store(d);
    ui.playlist.upgrade()
}

pub(crate) fn wire(ui: &Ui, d: &PlaylistDialog) {
    let rows = ui.playlist_rows.clone();
    d.set_entries(ModelRc::from(rows.clone()));
    {
        let (w, mgr) = (d.as_weak(), ui.ctx.mgr.clone());
        d.on_fetch(move || {
            let Some(d) = w.upgrade() else { return };
            let url = d.get_url().trim().to_string();
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                d.set_message(t("Paste the playlist address (https://…/playlist?list=…)").into());
                return;
            }
            d.set_message(SharedString::new());
            d.set_busy(true);
            d.set_status(t("Getting the list of videos… (a few seconds)").into());
            let w2 = d.as_weak();
            let base = mgr.settings().dir_for("Video");
            std::thread::spawn(move || {
                let result = swiftget_engine::ytdlp::playlist(&url);
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(d) = w2.upgrade() else { return };
                    d.set_busy(false);
                    match result {
                        Ok(p) if p.entries.is_empty() => {
                            d.set_status(t("No videos were found at this address.").into());
                        }
                        Ok(p) => {
                            let list: Vec<LinkRow> = p
                                .entries
                                .iter()
                                .enumerate()
                                .map(|(i, e)| LinkRow {
                                    checked: true,
                                    name: e.title.clone().into(),
                                    kind: "video".into(),
                                    ext: fmt_len(e.duration).into(),
                                    text: format!(
                                        "{}",
                                        row_number(i, p.entries.len(), d.get_reverse_number())
                                    )
                                    .into(),
                                    url: e.url.clone().into(),
                                    // the playlist dialog shows the
                                    // duration in `ext`, no size column
                                    size: SharedString::new(),
                                })
                                .collect();
                            if let Some(m) =
                                d.get_entries().as_any().downcast_ref::<VecModel<LinkRow>>()
                            {
                                m.set_vec(list);
                                d.set_checked_text(checked_text(m).into());
                            }
                            d.set_playlist_title(p.title.clone().into());
                            d.set_status(tf("{} videos", &[&p.entries.len()]).into());
                            let folder = base.join(filename::sanitize(&p.title));
                            d.set_folder(folder.display().to_string().into());
                        }
                        Err(e) => d.set_status(
                            tf(
                                "Could not get the list: {}",
                                &[&crate::err_text(&format!("{e:#}"))],
                            )
                            .into(),
                        ),
                    }
                });
            });
        });
    }
    {
        // "Number from the oldest video" toggled after a fetch: re-map
        // the shown numbers in place, the list order stays as fetched.
        let (w, rows) = (d.as_weak(), rows.clone());
        d.on_renumber(move || {
            let Some(d) = w.upgrade() else { return };
            let total = rows.row_count();
            let reverse = d.get_reverse_number();
            for i in 0..total {
                if let Some(mut r) = rows.row_data(i) {
                    r.text = format!("{}", row_number(i, total, reverse)).into();
                    rows.set_row_data(i, r);
                }
            }
        });
    }
    {
        let (w, rows) = (d.as_weak(), rows.clone());
        d.on_toggle(move |i| {
            if let Some(mut r) = rows.row_data(i.max(0) as usize) {
                r.checked = !r.checked;
                rows.set_row_data(i as usize, r);
            }
            if let Some(d) = w.upgrade() {
                d.set_checked_text(checked_text(&rows).into());
            }
        });
    }
    {
        let (w, rows) = (d.as_weak(), rows.clone());
        d.on_check_all(move |on| {
            for i in 0..rows.row_count() {
                if let Some(mut r) = rows.row_data(i) {
                    r.checked = on;
                    rows.set_row_data(i, r);
                }
            }
            if let Some(d) = w.upgrade() {
                d.set_checked_text(checked_text(&rows).into());
            }
        });
    }
    {
        let w = d.as_weak();
        d.on_browse(move || {
            let Some(d) = w.upgrade() else { return };
            if let Some(dir) = crate::winfx::file_box().pick_folder() {
                d.set_folder(dir.display().to_string().into());
            }
        });
    }
    {
        let ui = ui.clone();
        d.on_download(move || {
            let Some(d) = ui.playlist.upgrade() else {
                return;
            };
            let chosen: Vec<LinkRow> = (0..ui.playlist_rows.row_count())
                .filter_map(|i| ui.playlist_rows.row_data(i))
                .filter(|r| r.checked)
                .collect();
            if chosen.is_empty() {
                d.set_message(t("Check the videos you want first").into());
                return;
            }
            let folder = d.get_folder().trim().to_string();
            if folder.is_empty() {
                d.set_message(t("Choose where to save the videos").into());
                return;
            }
            let folder = PathBuf::from(folder);
            let quality = QUALITIES[d.get_quality().clamp(0, QUALITIES.len() as i32 - 1) as usize]
                .to_string();
            let ext = match quality.as_str() {
                "audio" => "m4a",
                "mp3" => "mp3",
                _ => "mp4",
            };
            let title = d.get_playlist_title().to_string();
            let numbered = d.get_numbered();
            let width = chosen.len().to_string().len().max(2);
            // skip videos whose exact target file is already in the
            // folder (same sanitize + number + extension as the add
            // loop below, so the two can never disagree). Matching is
            // by target file name only — no history store exists.
            let mut skipped = 0usize;
            let mut to_add: Vec<&LinkRow> = Vec::with_capacity(chosen.len());
            for r in &chosen {
                if d.get_skip_downloaded()
                    && folder
                        .join(target_file_name(r, numbered, width, ext))
                        .exists()
                {
                    skipped += 1;
                } else {
                    to_add.push(r);
                }
            }
            if to_add.is_empty() {
                d.set_message(tf("{} videos already downloaded — skipped", &[&skipped]).into());
                return;
            }
            // its own queue: N videos at a time, all into the folder
            let name = if title.trim().is_empty() {
                "Playlist".to_string()
            } else {
                title.trim().chars().take(60).collect()
            };
            let queue = match ui.ctx.mgr.add_queue(
                &name,
                d.get_parallel().clamp(1, 8) as u32,
                Some(folder.clone()),
            ) {
                Ok(q) => q,
                Err(e) => {
                    d.set_message(format!("{e:#}").into());
                    return;
                }
            };
            for r in &to_add {
                let file = target_file_name(r, numbered, width, ext);
                ui.ctx.mgr.add(NewDownload {
                    url: r.url.to_string(),
                    file_name: Some(file),
                    save_dir: Some(folder.clone()),
                    source_page: Some(r.url.to_string()),
                    pick_quality: Some(quality.clone()),
                    queue: Some(queue),
                    description: Some(title.clone()).filter(|t| !t.is_empty()),
                    start: false,
                    ..Default::default()
                });
            }
            if d.get_start_now() {
                ui.ctx.mgr.start_queue(queue);
            }
            if let Some(m) = ui.ctx.main.upgrade() {
                m.set_view(format!("queue:{queue}").into());
            }
            let _ = std::fs::create_dir_all(&folder);
            if skipped > 0 {
                // keep the dialog open so the report is seen. The rows
                // just added are unchecked, so Download again only
                // re-checks the skipped ones — which skip again.
                for i in 0..ui.playlist_rows.row_count() {
                    if let Some(mut r) = ui.playlist_rows.row_data(i) {
                        if r.checked && to_add.iter().any(|a| a.url == r.url) {
                            r.checked = false;
                            ui.playlist_rows.set_row_data(i, r);
                        }
                    }
                }
                d.set_checked_text(checked_text(&ui.playlist_rows).into());
                d.set_message(
                    tf(
                        "Added {} videos — {} already downloaded, skipped",
                        &[&to_add.len(), &skipped],
                    )
                    .into(),
                );
                refresh(&ui);
                return;
            }
            let _ = d.hide();
            refresh(&ui);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn row(name: &str, text: &str) -> LinkRow {
        LinkRow {
            checked: true,
            name: name.into(),
            kind: "video".into(),
            ext: SharedString::new(),
            text: text.into(),
            url: format!("https://example.com/watch?v={text}").into(),
            size: SharedString::new(),
        }
    }

    #[test]
    fn numbering_forward() {
        let nums: Vec<_> = (0..5).map(|i| row_number(i, 5, false)).collect();
        assert_eq!(nums, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn numbering_reverse_oldest_is_one() {
        // newest on top is #5, oldest at the bottom is #1
        let nums: Vec<_> = (0..5).map(|i| row_number(i, 5, true)).collect();
        assert_eq!(nums, vec![5, 4, 3, 2, 1]);
        assert_eq!(row_number(0, 5, true), 5);
        assert_eq!(row_number(4, 5, true), 1);
    }

    #[test]
    fn numbering_single_video() {
        assert_eq!(row_number(0, 1, false), 1);
        assert_eq!(row_number(0, 1, true), 1);
    }

    #[test]
    fn target_name_numbered_padded() {
        assert_eq!(
            target_file_name(&row("My Video", "3"), true, 2, "mp4"),
            "03 - My Video.mp4"
        );
    }

    #[test]
    fn target_name_width_padding() {
        // 105 videos -> width 3
        assert_eq!(
            target_file_name(&row("Clip", "7"), true, 3, "mp4"),
            "007 - Clip.mp4"
        );
        assert_eq!(
            target_file_name(&row("Clip", "105"), true, 3, "mp4"),
            "105 - Clip.mp4"
        );
    }

    #[test]
    fn target_name_unnumbered() {
        assert_eq!(
            target_file_name(&row("My Video", "3"), false, 2, "mp4"),
            "My Video.mp4"
        );
    }

    #[test]
    fn target_name_sanitized() {
        assert_eq!(
            target_file_name(&row("A: B/C?", "1"), true, 2, "mp4"),
            "01 - A_ B_C_.mp4"
        );
    }

    #[test]
    fn skip_file_present_and_absent() {
        let dir = std::env::temp_dir().join(format!("ezoo-h4-skip-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let present = row("Done", "1");
        let absent = row("Todo", "2");
        fs::write(dir.join(target_file_name(&present, true, 2, "mp4")), b"x").unwrap();
        assert!(dir
            .join(target_file_name(&present, true, 2, "mp4"))
            .exists());
        assert!(!dir.join(target_file_name(&absent, true, 2, "mp4")).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn skip_agrees_with_numbering() {
        // reverse numbering: the oldest row is #1, so its file is
        // "01 - Oldest.mp4" for both the add loop and the skip check
        let dir = std::env::temp_dir().join(format!("ezoo-h4-agree-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let oldest = row("Oldest", &row_number(4, 5, true).to_string());
        let newest = row("Newest", &row_number(0, 5, true).to_string());
        assert_eq!(oldest.text.as_str(), "1");
        assert_eq!(newest.text.as_str(), "5");
        fs::write(dir.join(target_file_name(&oldest, true, 2, "mp4")), b"x").unwrap();
        assert!(dir.join("01 - Oldest.mp4").exists());
        assert!(!dir.join(target_file_name(&newest, true, 2, "mp4")).exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
