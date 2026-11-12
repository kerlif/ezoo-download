//! Data types shared by the engine and the UI.

use crate::segments::Segment;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Identifier of a download in the queue.
pub type DownloadId = u64;

/// The state of a download in the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Queued,
    Connecting,
    Downloading,
    Assembling,
    Paused,
    Completed,
    Failed,
}

impl Status {
    /// Whether the download is actively working (connecting, downloading, assembling).
    pub fn is_running(self) -> bool {
        matches!(
            self,
            Status::Connecting | Status::Downloading | Status::Assembling
        )
    }
    /// The English status word shown in the UI.
    pub fn label(self) -> &'static str {
        match self {
            Status::Queued => "Queued",
            Status::Connecting => "Connecting…",
            Status::Downloading => "Downloading",
            Status::Assembling => "Assembling…",
            Status::Paused => "Paused",
            Status::Completed => "Complete",
            Status::Failed => "Error",
        }
    }
}

/// How a download's bytes are fetched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadKind {
    /// Normal file over HTTP/HTTPS (segmented when the server allows ranges)
    Http,
    /// HLS stream (.m3u8): many small parts joined into one video file
    Hls,
    /// Separate video and audio streams (YouTube style, "DASH"):
    /// both are downloaded, then merged into one file with ffmpeg
    Dash,
    /// a .torrent file or a magnet link, shared with other computers (aria2)
    Torrent,
}

/// What a download does after the data is all here (its second bar).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// the picture and the sound into one file
    Join,
    /// a stream into an MP4 file
    ToMp4,
    /// the sound into an MP3 file
    ToMp3,
    /// the file to its folder on another disk
    Move,
    /// the part of a video cut out
    Cut,
}

impl Step {
    pub fn from_code(c: u8) -> Option<Step> {
        Some(match c {
            1 => Step::Join,
            2 => Step::ToMp4,
            3 => Step::ToMp3,
            4 => Step::Move,
            5 => Step::Cut,
            _ => return None,
        })
    }
    pub fn code(self) -> u8 {
        match self {
            Step::Join => 1,
            Step::ToMp4 => 2,
            Step::ToMp3 => 3,
            Step::Move => 4,
            Step::Cut => 5,
        }
    }
}

/// A part of a video: from `start` to `end` seconds. `exact` re-encodes
/// (frame-accurate); otherwise the cut is a stream copy and starts at the
/// nearest keyframe at or before `start`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    pub start: f64,
    pub end: f64,
    pub exact: bool,
}

/// Longest part: a day. Shortest: half a second (shorter cuts are
/// usually a typo, and ffmpeg's stream copy of them is often empty).
pub const CLIP_MAX_END: f64 = 24.0 * 3600.0;
pub const CLIP_MIN_LEN: f64 = 0.5;

impl Clip {
    /// Checked construction: an invalid part is an error here, never a
    /// silent whole-file download further down the line.
    pub fn new(start: f64, end: f64, exact: bool) -> Result<Clip, String> {
        let c = Clip { start, end, exact };
        c.validate()?;
        Ok(c)
    }

    /// Re-check a part read back from disk or from the extension: the
    /// same rules as construction, so a stored part cannot smuggle an
    /// invalid range into a run.
    pub fn validate(&self) -> Result<(), String> {
        if !self.start.is_finite() || !self.end.is_finite() {
            return Err("the start and end times must be numbers".into());
        }
        if self.start < 0.0 {
            return Err("the start time cannot be negative".into());
        }
        if self.end > CLIP_MAX_END {
            return Err("the end time cannot be more than 24 hours".into());
        }
        if self.end - self.start < CLIP_MIN_LEN {
            return Err("the part must be at least half a second long".into());
        }
        Ok(())
    }

    pub fn len(&self) -> f64 {
        (self.end - self.start).max(0.0)
    }
}

/// "movie.mp4" + 80..205s -> "movie [01-20 → 03-25].mp4": the file
/// name says which part it is, so a cut never overwrites its source.
pub fn clip_name(name: &str, clip: &Clip) -> String {
    fn mmss(secs: f64) -> String {
        let total = secs.max(0.0).round() as u64;
        format!("{:02}-{:02}", total / 60, total % 60)
    }
    let (stem, ext) = crate::filename::split_ext(name);
    let stem = if stem.is_empty() { "video" } else { stem };
    if ext.is_empty() {
        format!("{stem} [{} → {}]", mmss(clip.start), mmss(clip.end))
    } else {
        format!("{stem} [{} → {}].{ext}", mmss(clip.start), mmss(clip.end))
    }
}

/// Extra request data, usually handed over by the browser extension so that
/// downloads behind a login work exactly like in the browser.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestHeaders {
    pub referer: Option<String>,
    /// the site's login cookies: encrypted on the disk (secret.rs)
    #[serde(default, with = "crate::secret::opt")]
    pub cookies: Option<String>,
    pub user_agent: Option<String>,
    #[serde(default)]
    pub extra: Vec<(String, String)>,
    /// user name + password for the server ("Use authorization")
    /// encrypted on the disk (secret.rs)
    #[serde(default, with = "crate::secret::pair")]
    pub auth: Option<(String, String)>,
}

/// What we learn about a URL before downloading it.
#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub final_url: String,
    pub file_name: String,
    pub total_size: Option<u64>,
    /// The size came from a plain Content-Length with no byte-range
    /// confirmation, so it is untrusted: the server may be lying (a
    /// wrong length is shown with "~" and never trusted for planning).
    /// Sizes confirmed by a 206 Content-Range total are verified.
    pub total_size_unverified: bool,
    pub resumable: bool,
    pub content_type: Option<String>,
    pub kind: DownloadKind,
    /// the server's "Last-Modified" date
    pub last_modified: Option<DateTime<Utc>>,
    /// the server's ETag for the file, when it sends one
    pub etag: Option<String>,
    /// the server refused the request as it was (HTTP 403 and the like) and
    /// took it with these headers instead: the download uses them too
    pub headers_used: Option<RequestHeaders>,
}

impl ProbeResult {
    /// The address opens a web page, not a file.
    pub fn is_web_page(&self) -> bool {
        let ct = self
            .content_type
            .as_deref()
            .unwrap_or("")
            .to_ascii_lowercase();
        let ext = crate::filename::extension(&self.file_name);
        ct.starts_with("text/html") && !matches!(ext.as_str(), "htm" | "html" | "mht" | "mhtml")
    }
}

/// A request to add a download.
#[derive(Debug, Clone, Default)]
pub struct NewDownload {
    pub url: String,
    pub file_name: Option<String>,
    pub save_dir: Option<PathBuf>,
    pub headers: RequestHeaders,
    pub connections: Option<u32>,
    /// Result of an earlier probe (from the add dialog), if any
    pub probe: Option<ProbeResult>,
    /// Separate audio stream to merge with `url` (YouTube style downloads)
    pub audio_url: Option<String>,
    /// YouTube page + chosen quality, to get fresh links when old ones expire
    pub source_page: Option<String>,
    pub source_key: Option<String>,
    /// the quality the overlay offered for this download (e.g. an HLS
    /// variant like "1080p"), so a later quality window can offer the
    /// same choice again (Q26 plumbing; the full window is a later phase)
    pub quality_key: Option<String>,
    /// the chosen stream variant's own playlist (HLS): when set, the
    /// runner fetches this instead of the master playlist, so the picked
    /// quality is what actually downloads (Q7 steering, Phase 6).
    /// None = the best variant, as before. DASH does not use it: the
    /// DASH runner picks by `quality_key` instead.
    pub stream_variant_url: Option<String>,
    /// put it in this download queue
    pub queue: Option<u64>,
    /// the user's note from the download dialog
    pub description: Option<String>,
    /// a video page (playlist item): find its links when it starts, in this
    /// quality ("best", "audio", or a height like "720")
    pub pick_quality: Option<String>,
    /// started at once while the "Download File Info" window is open
    /// (its name and folder can change until the user confirms)
    pub early: bool,
    pub start: bool,
    /// torrents: the files to get, 1-based ("1,3,4"); None = all
    pub torrent_select: Option<String>,
    /// cut only this part of the video (None = the whole file)
    pub clip: Option<Clip>,
}

/// Persisted state of one download.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadRecord {
    pub id: DownloadId,
    pub url: String,
    pub final_url: Option<String>,
    pub file_name: String,
    pub save_dir: PathBuf,
    pub kind: DownloadKind,
    pub total_size: Option<u64>,
    pub resumable: bool,
    #[serde(default)]
    pub segments: Vec<Segment>,
    /// Dash downloads: the audio stream and its own segment table
    #[serde(default)]
    pub audio_url: Option<String>,
    #[serde(default)]
    pub audio_segments: Vec<Segment>,
    #[serde(default)]
    pub video_size: Option<u64>,
    #[serde(default)]
    pub source_page: Option<String>,
    #[serde(default)]
    pub source_key: Option<String>,
    #[serde(default)]
    pub quality_key: Option<String>,
    /// the chosen HLS variant playlist (see `NewDownload`); cleared when
    /// the link is refreshed, since it came from the old master
    #[serde(default)]
    pub stream_variant_url: Option<String>,
    #[serde(default)]
    pub queue: Option<u64>,
    /// name was only guessed from the URL; the server's real name wins
    #[serde(default)]
    pub auto_name: bool,
    #[serde(default)]
    pub description: Option<String>,
    /// this download's own speed limit in KB/s
    #[serde(default)]
    pub speed_limit_kb: Option<u64>,
    /// shut down / sleep / exit when this download finishes
    #[serde(default)]
    pub on_complete: crate::config::AfterDone,
    /// bytes done (used for HLS and for display after restart)
    #[serde(default)]
    pub downloaded: u64,
    #[serde(default)]
    pub parts_total: u32,
    #[serde(default)]
    pub parts_done: u32,
    pub status: Status,
    pub error: Option<String>,
    #[serde(default)]
    pub headers: RequestHeaders,
    pub connections: u32,
    pub category: String,
    pub added_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    /// temporary folder for the parts (Options > Save To); None = next to the file
    #[serde(default)]
    pub temp_dir: Option<PathBuf>,
    /// position inside its queue (Queue > Files in the queue: move up / down)
    #[serde(default)]
    pub queue_pos: u64,
    /// last time the download was started
    #[serde(default)]
    pub last_try: Option<DateTime<Utc>>,
    /// server's file date (Last-Modified)
    #[serde(default)]
    pub server_time: Option<DateTime<Utc>>,
    /// server's ETag for the file, when it sends one
    #[serde(default)]
    pub etag: Option<String>,
    /// automatic retries done by its queue
    #[serde(default)]
    pub retry_count: u32,
    /// video page whose links are found when it starts (playlists)
    #[serde(default)]
    pub pick_quality: Option<String>,
    /// fixed name of the temporary files: the download started while the
    /// "Download File Info" window was still open, so its final name and
    /// folder may still change
    #[serde(default)]
    pub work_stem: Option<String>,
    /// torrents: the files to get, 1-based ("1,3,4"); None = all
    #[serde(default)]
    pub torrent_select: Option<String>,
    /// torrents: the .torrent file said it is private (`Some(true)`),
    /// public (`Some(false)`), or nobody knows (`None` — a magnet
    /// link). Only a known-public torrent gets the public trackers.
    #[serde(default)]
    pub torrent_private: Option<bool>,
    /// torrents: the files it writes, relative to `save_dir` (learnt while
    /// it runs; a magnet link only knows them then)
    #[serde(default)]
    pub torrent_files: Vec<String>,
    /// cut only this part of the video (None = the whole file)
    #[serde(default)]
    pub clip: Option<Clip>,
}

impl DownloadRecord {
    pub fn target_path(&self) -> PathBuf {
        self.save_dir.join(&self.file_name)
    }
    /// where the temporary files live
    pub fn work_dir(&self) -> PathBuf {
        match &self.temp_dir {
            Some(t) if !t.as_os_str().is_empty() => t.clone(),
            _ => self.save_dir.clone(),
        }
    }
    pub fn part_path(&self) -> PathBuf {
        if let Some(stem) = &self.work_stem {
            return self
                .work_dir()
                .join(format!("{stem}.{}", crate::filename::PART_EXT));
        }
        match &self.temp_dir {
            // ids keep same-named files from different folders apart
            Some(t) if !t.as_os_str().is_empty() => {
                crate::filename::part_path(t, &format!("{}-{}", self.id, self.file_name))
            }
            _ => crate::filename::part_path(&self.save_dir, &self.file_name),
        }
    }
    /// base name of the temporary files (with the id when they share a temp folder)
    fn temp_stem(&self) -> String {
        if let Some(stem) = &self.work_stem {
            return stem.clone();
        }
        if self.work_dir() == self.save_dir {
            self.file_name.clone()
        } else {
            format!("{}-{}", self.id, self.file_name)
        }
    }
    pub fn hls_temp_dir(&self) -> PathBuf {
        self.work_dir()
            .join(format!(".{}.sgparts", self.temp_stem()))
    }
    pub fn video_part_path(&self) -> PathBuf {
        self.work_dir()
            .join(format!(".{}.video.sgpart", self.temp_stem()))
    }
    pub fn audio_part_path(&self) -> PathBuf {
        self.work_dir()
            .join(format!(".{}.audio.sgpart", self.temp_stem()))
    }
    /// Torrents: every file it writes (only those: the torrent's folder may
    /// have the name of a folder that was there before).
    pub fn torrent_data_paths(&self) -> Vec<PathBuf> {
        if !self.torrent_files.is_empty() {
            return self
                .torrent_files
                .iter()
                .map(|f| self.save_dir.join(f))
                .collect();
        }
        let Some(info) = std::fs::read(&self.url)
            .ok()
            .and_then(|d| crate::torrent::parse_torrent(&d).ok())
        else {
            return Vec::new();
        };
        let single = info.files.len() == 1 && info.files[0].path == info.name;
        info.files
            .iter()
            .map(|f| {
                let rel: PathBuf = f.path.split('/').map(crate::filename::sanitize).collect();
                if single {
                    self.save_dir.join(&self.file_name)
                } else {
                    self.save_dir.join(&self.file_name).join(rel)
                }
            })
            .collect()
    }

    /// Every temporary file/folder this download may leave behind.
    pub fn temp_paths(&self) -> Vec<PathBuf> {
        if self.kind == DownloadKind::Torrent {
            // aria2's notes of what it has
            return vec![self.save_dir.join(format!("{}.aria2", self.file_name))];
        }
        vec![
            self.part_path(),
            self.hls_temp_dir(),
            self.video_part_path(),
            self.audio_part_path(),
        ]
    }
    pub fn request_url(&self) -> &str {
        self.final_url.as_deref().unwrap_or(&self.url)
    }
}

/// Snapshot of a download for the UI.
#[derive(Debug, Clone)]
pub struct DownloadView {
    pub id: DownloadId,
    pub url: String,
    pub file_name: String,
    pub save_path: PathBuf,
    pub category: String,
    pub kind: DownloadKind,
    pub status: Status,
    pub error: Option<String>,
    pub total_size: Option<u64>,
    pub downloaded: u64,
    /// bytes per second
    pub speed: f64,
    pub eta_secs: Option<u64>,
    pub active_connections: u32,
    /// people sharing this torrent (torrents only, 0 otherwise)
    pub seeders: u32,
    pub resumable: bool,
    pub parts_total: u32,
    pub parts_done: u32,
    /// (start, pos, end, active, takeover) of every segment, for the
    /// connections view. `takeover` is the stall monitor's flag: the
    /// segment's connection is being closed and the segment re-requested
    /// from `pos`. Read-only exposure of the engine's segment table.
    pub segments: Vec<(u64, u64, u64, bool, bool)>,
    pub queue: Option<u64>,
    pub finished_at: Option<DateTime<Utc>>,
    pub description: Option<String>,
    pub speed_limit_kb: Option<u64>,
    pub on_complete: crate::config::AfterDone,
    pub added_at: DateTime<Utc>,
    pub last_try: Option<DateTime<Utc>>,
    pub headers: RequestHeaders,
    /// page the download came from
    pub parent_page: Option<String>,
    /// the picked quality key ("1080p", ...) and, for HLS, the picked
    /// variant's own playlist (None = the best variant)
    pub quality_key: Option<String>,
    pub stream_variant_url: Option<String>,
    /// the video page + quality the download came from (yt-dlp option
    /// key, e.g. "1080p|MP4"): the AV1 tip reads the codec from it
    pub source_key: Option<String>,
    pub queue_pos: u64,
    /// the connection dropped: waiting for the internet to come back
    pub waiting: bool,
    /// after the data: what it does now and how far (0..1)
    pub step: Option<(Step, f32)>,
}

impl DownloadView {
    pub fn referer(&self) -> &str {
        self.headers.referer.as_deref().unwrap_or("")
    }
    pub fn progress(&self) -> f32 {
        if self.status == Status::Completed {
            return 1.0;
        }
        match self.total_size {
            Some(t) if t > 0 => (self.downloaded as f64 / t as f64).min(1.0) as f32,
            _ if self.parts_total > 0 => self.parts_done as f32 / self.parts_total as f32,
            _ => 0.0,
        }
    }
}

/// Per-segment state for the connections view, from the segment table
/// alone (no new engine data): 0 idle, 1 downloading, 2 done, 3 stalled.
/// A flagged segment stays stalled until a worker claims it again, which
/// clears the flag; a finished segment is done even if it was flagged.
pub fn segment_state(finished: bool, takeover: bool, active: bool, running: bool) -> i32 {
    if finished {
        2
    } else if takeover {
        3
    } else if active && running {
        1
    } else {
        0
    }
}

/// Human readable size: 1.5 MB
pub fn fmt_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.2} {}", UNITS[u])
    }
}

pub fn fmt_duration(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h > 0 {
        format!("{h}h {m:02}m")
    } else if m > 0 {
        format!("{m}m {s:02}s")
    } else {
        format!("{s}s")
    }
}

#[cfg(test)]
mod tests {
    use super::segment_state;

    #[test]
    fn segment_state_covers_every_table_combination() {
        // done wins over a stale flag; a flag wins over active/running
        assert_eq!(segment_state(true, true, true, true), 2);
        assert_eq!(segment_state(true, false, false, false), 2);
        assert_eq!(segment_state(false, true, true, true), 3);
        assert_eq!(segment_state(false, true, false, false), 3);
        assert_eq!(segment_state(false, false, true, true), 1);
        // active but paused/queued: idle, not downloading
        assert_eq!(segment_state(false, false, true, false), 0);
        assert_eq!(segment_state(false, false, false, true), 0);
        assert_eq!(segment_state(false, false, false, false), 0);
    }
}
