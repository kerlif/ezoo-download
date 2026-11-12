//! Joining a video-only and an audio-only file into one playable file.
//! Uses ffmpeg in "copy" mode: no re-encoding, so it takes seconds and keeps
//! the original quality.

use anyhow::{bail, Context};
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio_util::sync::CancellationToken;

/// One join of video + audio is not worth moving the `moov` atom to
/// the front for: with a big file the extra pass reads and rewrites
/// the whole file again. Below this size, "play while still loading"
/// is worth it.
pub const FASTSTART_UP_TO: u64 = 1 << 30;

/// Run ffmpeg with its stderr piped, watching `output` grow: `on` is
/// called with 0..1 as the file reaches `expected` bytes (the inputs'
/// sizes added up — an estimate, not a promise). Cancelling kills
/// ffmpeg at once instead of waiting for a big join to finish, and the
/// last line ffmpeg wrote is the error when it fails.
///
/// The child is a plain `std::process::Child`, polled with `try_wait`
/// (no tokio `process` feature needed): a thread drains stderr so a
/// chatty ffmpeg can never block on a full pipe.
pub async fn run_watched(
    mut cmd: Command,
    output: &Path,
    expected: u64,
    on: &(dyn Fn(f64) + Send + Sync),
    cancel: Option<&CancellationToken>,
) -> anyhow::Result<()> {
    use std::io::Read;
    cmd.stderr(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::null());
    let mut child = cmd.spawn().context("could not start ffmpeg")?;
    let mut stderr = child.stderr.take().expect("stderr was piped");
    let stderr_task = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf);
        buf
    });
    let status = loop {
        if let Some(status) = child.try_wait().context("ffmpeg ended oddly")? {
            break status;
        }
        if cancel.is_some_and(|t| t.is_cancelled()) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stderr_task.join();
            return Err(crate::http_download::Cancelled.into());
        }
        let done = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
        on((done as f64 / expected.max(1) as f64).min(1.0));
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    };
    let bytes = stderr_task.join().unwrap_or_default();
    if !status.success() {
        // a crash (signal on Unix, NTSTATUS on Windows) is not an
        // ordinary failure: the caller may retry with another ffmpeg
        // build (Q71 — the pinned bundled build segfaults on TS)
        if let Some(how) = crash_description(&status) {
            return Err(FfmpegCrashed(how).into());
        }
        let err = String::from_utf8_lossy(&bytes);
        let last = err
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("unknown error");
        bail!("ffmpeg failed: {last}");
    }
    on(1.0);
    Ok(())
}

/// ffmpeg died from a signal (Unix) or abnormal termination
/// (Windows) instead of exiting with an error code. Carried as the
/// anyhow error payload so the retry wrapper can tell a crash from
/// an ordinary failure ("ffmpeg failed: …") and from a cancel.
#[derive(Debug)]
pub struct FfmpegCrashed(pub String);

impl std::fmt::Display for FfmpegCrashed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ffmpeg crashed ({})", self.0)
    }
}

impl std::error::Error for FfmpegCrashed {}

/// How ffmpeg died, when it died abnormally instead of exiting with
/// an error code: the Unix signal ("signal 11"), or a Windows
/// NTSTATUS crash code ("exit code 0xc0000005"). `None` for an
/// ordinary exit, successful or not.
pub(crate) fn crash_description(status: &std::process::ExitStatus) -> Option<String> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.signal().map(|s| format!("signal {s}"))
    }
    #[cfg(windows)]
    {
        // ffmpeg exits with a small code on an ordinary failure; a
        // crash is an NTSTATUS code (0xC0000000+), negative as i32
        status
            .code()
            .filter(|c| *c < 0)
            .map(|c| format!("exit code {:#x}", c as u32))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = status;
        None
    }
}

/// ffmpeg from PATH only — never the bundled copy, never one next
/// to the app. The crash fallback (Q71): the bundled build
/// segfaults on some MPEG-TS inputs, the system build does not.
/// The probe is the same `--version`/`-version` check `find_tool`
/// uses for PATH.
pub fn find_system_ffmpeg() -> Option<PathBuf> {
    let exe_name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    let mut probe = Command::new(exe_name);
    probe.arg("--version");
    hide_window(&mut probe);
    match probe.output() {
        Ok(o) if o.status.success() => Some(PathBuf::from(exe_name)),
        // ffmpeg prints its version with -version and fails on --version
        Ok(_) => {
            let mut p = Command::new(exe_name);
            p.arg("-version");
            hide_window(&mut p);
            p.output()
                .ok()
                .filter(|o| o.status.success())
                .map(|_| PathBuf::from(exe_name))
        }
        _ => None,
    }
}

/// A bare name (`ffmpeg`), resolved from PATH at spawn time — not a
/// concrete file. `find_tool` returns one when no bundled copy and
/// no copy next to the app exists.
fn is_path_lookup(p: &Path) -> bool {
    p.parent().is_some_and(|par| par.as_os_str().is_empty())
}

/// Whether `first` already IS the system ffmpeg: retrying with it
/// would just run the same crashing copy again.
pub(crate) fn same_as_system(first: &Path, system: &Path) -> bool {
    first == system || is_path_lookup(first)
}

/// Run ffmpeg watched, retrying ONCE with the system ffmpeg from
/// PATH when the first binary dies from a signal (Q71). The default
/// order is unchanged: bundled first, system only as a crash
/// fallback — an ordinary failure ("ffmpeg failed: …") never
/// retries. `build` rebuilds the command for a given ffmpeg binary
/// (the cut path builds a shell wrapper around it); `first` is the
/// binary `find_ffmpeg` resolved.
pub(crate) async fn run_ffmpeg_watched(
    build: &(dyn Fn(&Path) -> anyhow::Result<Command> + Send + Sync),
    first: &Path,
    output: &Path,
    expected: u64,
    on: &(dyn Fn(f64) + Send + Sync),
    cancel: Option<&CancellationToken>,
) -> anyhow::Result<()> {
    run_ffmpeg_watched_with(
        build,
        first,
        output,
        expected,
        on,
        cancel,
        find_system_ffmpeg,
    )
    .await
}

async fn run_ffmpeg_watched_with(
    build: &(dyn Fn(&Path) -> anyhow::Result<Command> + Send + Sync),
    first: &Path,
    output: &Path,
    expected: u64,
    on: &(dyn Fn(f64) + Send + Sync),
    cancel: Option<&CancellationToken>,
    find_fallback: impl Fn() -> Option<PathBuf> + Send,
) -> anyhow::Result<()> {
    match run_watched(build(first)?, output, expected, on, cancel).await {
        Ok(()) => Ok(()),
        Err(e) => {
            let Some(how) = e.downcast_ref::<FfmpegCrashed>().map(|c| c.0.clone()) else {
                return Err(e);
            };
            let Some(system) = find_fallback() else {
                bail!(
                    "{}",
                    crate::tools::ffmpeg_crashed_message(
                        cfg!(windows),
                        crate::tools::in_flatpak(),
                        &how
                    )
                );
            };
            if same_as_system(first, &system) {
                return Err(e);
            }
            run_watched(build(&system)?, output, expected, on, cancel).await
        }
    }
}

/// How big a file is, or 0 when it cannot be read.
fn file_len(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

pub async fn merge(
    video: &Path,
    audio: &Path,
    output: &Path,
    final_name: &str,
    on: &(dyn Fn(f64) + Send + Sync),
    cancel: Option<&CancellationToken>,
) -> anyhow::Result<()> {
    let Some(ffmpeg) = find_ffmpeg() else {
        bail!("{}", missing_message());
    };
    let format = format_for(final_name);
    let expected = file_len(video) + file_len(audio);
    // the command is rebuilt per binary: a crash retries with the
    // system ffmpeg (Q71), same words, another binary
    let build = |ff: &Path| {
        let mut cmd = Command::new(ff);
        cmd.args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
            .arg(video)
            .arg("-i")
            .arg(audio)
            .args(["-map", "0:v:0", "-map", "1:a:0", "-c", "copy"]);
        if format == "mp4" && expected < FASTSTART_UP_TO {
            // play while still loading, like normal web videos; a big file
            // is not worth the extra pass over the whole file to move it
            cmd.args(["-movflags", "+faststart"]);
        }
        cmd.args(["-f", format]).arg(output);
        hide_window(&mut cmd);
        Ok(cmd)
    };
    run_ffmpeg_watched(&build, &ffmpeg, output, expected, on, cancel)
        .await
        .context("ffmpeg could not merge video and audio")
}

/// A helper program (ffmpeg, yt-dlp): the copy in the app's "tools"
/// folder wins, then one next to EzooDownload.exe, then PATH.
pub fn find_tool(name: &str) -> Option<PathBuf> {
    let exe_name = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    if let Some(p) = crate::tools::bundled(name) {
        return Some(p);
    }
    if let Ok(me) = std::env::current_exe() {
        let local = me.with_file_name(&exe_name);
        if local.exists() {
            return Some(local);
        }
        let in_folder = me.with_file_name(name).join("bin").join(&exe_name);
        if in_folder.exists() {
            return Some(in_folder);
        }
    }
    let mut probe = Command::new(&exe_name);
    probe.arg("--version");
    hide_window(&mut probe);
    match probe.output() {
        Ok(o) if o.status.success() => Some(PathBuf::from(exe_name)),
        // ffmpeg prints its version with -version and fails on --version
        Ok(_) if name == "ffmpeg" => {
            let mut p = Command::new(&exe_name);
            p.arg("-version");
            hide_window(&mut p);
            p.output()
                .ok()
                .filter(|o| o.status.success())
                .map(|_| PathBuf::from(&exe_name))
        }
        _ => None,
    }
}

pub fn find_ffmpeg() -> Option<PathBuf> {
    find_tool("ffmpeg")
}

/// ffmpeg muxer name for the final file extension. The output path ends in
/// `.sgpart` while downloading, so ffmpeg can't guess it from the name.
pub fn format_for(file_name: &str) -> &'static str {
    match crate::filename::extension(file_name).as_str() {
        "webm" => "webm",
        "mkv" => "matroska",
        "m4a" | "mp4" | "m4v" | "mov" => "mp4",
        _ => "matroska",
    }
}

/// What to say when ffmpeg is not there. The message is
/// platform-specific (see `tools::ffmpeg_missing_message`); on Windows
/// it is the exact wording the UI maps to its translation.
fn missing_message() -> &'static str {
    crate::tools::ffmpeg_missing_message(cfg!(windows), crate::tools::in_flatpak(), false)
        .unwrap_or("ffmpeg is missing")
}

/// A joined stream (HLS, a .ts file) into an ordinary MP4 file: plays in
/// every player, with seeking. Nothing is re-encoded.
pub async fn ts_to_mp4(
    input: &Path,
    output: &Path,
    on: &(dyn Fn(f64) + Send + Sync),
    cancel: Option<&CancellationToken>,
) -> anyhow::Result<()> {
    let Some(ffmpeg) = find_ffmpeg() else {
        bail!("{}", missing_message());
    };
    // the container shrinks a little; a rough size is enough to watch
    let expected = file_len(input) * 95 / 100;
    let build = |ff: &Path| {
        let mut cmd = Command::new(ff);
        cmd.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "mpegts",
            "-i",
        ])
        .arg(input)
        .args(["-map", "0:v?", "-map", "0:a?", "-c", "copy"]);
        if expected < FASTSTART_UP_TO {
            cmd.args(["-movflags", "+faststart"]);
        }
        cmd.args(["-f", "mp4"]).arg(output);
        hide_window(&mut cmd);
        Ok(cmd)
    };
    let result = run_ffmpeg_watched(&build, &ffmpeg, output, expected, on, cancel).await;
    if result.is_err() {
        let _ = std::fs::remove_file(output);
    }
    result.context("ffmpeg could not make the MP4 file")
}

/// "Play while downloading" for picture + sound in two files: one playable
/// file of what is there so far (the first `video_ready` / `audio_ready`
/// bytes, nothing is re-encoded, takes a moment). `output` gets a
/// Matroska (.mkv) file: it takes every kind of picture and sound.
pub fn preview_snapshot(
    video: &Path,
    video_ready: u64,
    audio: Option<(&Path, u64)>,
    output: &Path,
) -> anyhow::Result<()> {
    let Some(ffmpeg) = find_ffmpeg() else {
        bail!("{}", missing_message());
    };
    // only the part with no gap ("subfile": ffmpeg reads just those bytes)
    let part = |p: &Path, n: u64| {
        let mut s = std::ffi::OsString::from(format!("subfile,,start,0,end,{n},,:"));
        s.push(p.as_os_str());
        s
    };
    let mut cmd = Command::new(&ffmpeg);
    cmd.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-protocol_whitelist",
        "file,subfile",
        "-i",
    ])
    .arg(part(video, video_ready));
    if let Some((a, n)) = audio {
        cmd.arg("-i")
            .arg(part(a, n))
            .args(["-map", "0:v:0", "-map", "1:a:0"]);
    } else {
        cmd.args(["-map", "0:v:0"]);
    }
    cmd.args(["-c", "copy", "-f", "matroska"]).arg(output);
    hide_window(&mut cmd);
    let out = cmd.output().context("could not start ffmpeg")?;
    // the cut at the end makes ffmpeg complain; what it wrote is fine
    let made = std::fs::metadata(output)
        .map(|m| m.len() > 1024)
        .unwrap_or(false);
    if !made {
        let err = String::from_utf8_lossy(&out.stderr);
        let last = err
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("unknown error");
        bail!("ffmpeg could not prepare the preview: {last}");
    }
    Ok(())
}

/// Turn a downloaded sound (or video) file into an MP3 file. The
/// progress callback watches the output grow; the expected size is
/// the input scaled by the audio bitrate (128k in, `kbps` out).
pub async fn to_mp3_watched(
    input: &Path,
    output: &Path,
    kbps: u64,
    title: Option<&str>,
    on: &(dyn Fn(f64) + Send + Sync),
    cancel: Option<&CancellationToken>,
) -> anyhow::Result<()> {
    let Some(ffmpeg) = find_ffmpeg() else {
        bail!("{}", missing_message());
    };
    let expected = file_len(input) * kbps.max(1) / 128;
    let title = title
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| t.chars().take(200).collect::<String>());
    let build = |ff: &Path| {
        let mut cmd = Command::new(ff);
        cmd.args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
            .arg(input)
            .args([
                "-vn",
                "-map",
                "0:a:0",
                "-map_metadata",
                "-1",
                "-c:a",
                "libmp3lame",
                "-b:a",
            ])
            .arg(format!("{kbps}k"))
            // the name shows on phones, car radios and players
            .args(["-id3v2_version", "3"]);
        if let Some(t) = &title {
            cmd.arg("-metadata").arg(format!("title={t}"));
        }
        cmd.args(["-f", "mp3"]).arg(output);
        hide_window(&mut cmd);
        Ok(cmd)
    };
    run_ffmpeg_watched(&build, &ffmpeg, output, expected, on, cancel)
        .await
        .context("ffmpeg could not make the MP3 file")
}

pub(crate) fn hide_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_of_half_downloaded_video_and_sound() {
        let Some(ffmpeg) = find_ffmpeg() else { return };
        let dir = std::env::temp_dir().join(format!("edm-preview-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let (v, a) = (dir.join(".x.video.sgpart"), dir.join(".x.audio.sgpart"));
        let gen = |args: &[&str], out: &Path| {
            let ok = Command::new(&ffmpeg)
                .args(["-hide_banner", "-loglevel", "error", "-y"])
                .args(args)
                .arg(out)
                .status()
                .unwrap();
            assert!(ok.success());
        };
        // like YouTube: fragmented MP4, picture and sound apart
        gen(
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x240:rate=25:duration=20",
                "-c:v",
                "mpeg4",
                "-an",
                "-movflags",
                "+frag_keyframe+empty_moov",
                "-f",
                "mp4",
            ],
            &v,
        );
        gen(
            &[
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=20",
                "-c:a",
                "aac",
                "-vn",
                "-movflags",
                "+frag_keyframe+empty_moov",
                "-f",
                "mp4",
            ],
            &a,
        );
        // the files are full size already (sparse), only half is real
        let (vl, al) = (
            std::fs::metadata(&v).unwrap().len(),
            std::fs::metadata(&a).unwrap().len(),
        );
        let out = dir.join("x (preview).mkv");
        preview_snapshot(&v, vl / 2, Some((&a, al / 2)), &out).unwrap();
        let probe = Command::new(&ffmpeg).arg("-i").arg(&out).output().unwrap();
        let info = String::from_utf8_lossy(&probe.stderr);
        assert!(
            info.contains("Video: mpeg4") && info.contains("Audio: aac"),
            "{info}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn stream_to_mp4() {
        let Some(ffmpeg) = find_ffmpeg() else { return };
        let dir = std::env::temp_dir().join(format!("edm-ts-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let ts = dir.join("x.ts.sgpart");
        let ok = Command::new(&ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x240:rate=25:duration=2",
                "-f",
                "lavfi",
                "-i",
                "sine=duration=2",
                "-c:v",
                "mpeg4",
                "-c:a",
                "aac",
                "-f",
                "mpegts",
            ])
            .arg(&ts)
            .status()
            .unwrap();
        assert!(ok.success());
        let out = dir.join("x.mp4.tmp");
        ts_to_mp4(&ts, &out, &|_| {}, None).await.unwrap();
        let probe = Command::new(&ffmpeg).arg("-i").arg(&out).output().unwrap();
        let info = String::from_utf8_lossy(&probe.stderr);
        assert!(
            info.contains("mp4") && info.contains("Audio: aac"),
            "{info}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn makes_mp3() {
        let Some(ffmpeg) = find_ffmpeg() else { return };
        let dir = std::env::temp_dir().join("edm-mp3-test");
        let _ = std::fs::create_dir_all(&dir);
        let a = dir.join("a.m4a.sgpart");
        let ok = Command::new(&ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=2",
                "-c:a",
                "aac",
                "-f",
                "mp4",
            ])
            .arg(&a)
            .status()
            .unwrap();
        assert!(ok.success());
        let out = dir.join("song.mp3.tmp");
        to_mp3_watched(&a, &out, 192, Some("Song"), &|_| {}, None)
            .await
            .unwrap();
        let probe = Command::new(&ffmpeg).arg("-i").arg(&out).output().unwrap();
        let info = String::from_utf8_lossy(&probe.stderr);
        assert!(
            info.contains("Audio: mp3") && info.contains("Song"),
            "{info}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Real merge, only when ffmpeg is installed (skipped otherwise).
    /// The progress callback is watched: it climbs 0..1 and ends at 1.
    #[tokio::test]
    async fn merges_video_and_audio() {
        let Some(ffmpeg) = find_ffmpeg() else { return };
        let dir = std::env::temp_dir().join("edm-mux-test");
        let _ = std::fs::create_dir_all(&dir);
        let (v, a) = (dir.join("v.mp4"), dir.join("a.m4a"));
        let gen = |args: &[&str]| {
            let ok = Command::new(&ffmpeg)
                .args(["-hide_banner", "-loglevel", "error", "-y"])
                .args(args)
                .status()
                .unwrap();
            assert!(ok.success());
        };
        gen(&[
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x240:rate=25:duration=2",
            "-c:v",
            "mpeg4",
            "-an",
            v.to_str().unwrap(),
        ]);
        gen(&[
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=2",
            "-c:a",
            "aac",
            "-vn",
            a.to_str().unwrap(),
        ]);
        // the output still has the .sgpart name while downloading
        let out = dir.join("Clip [1080p].mp4.sgpart");
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen2 = seen.clone();
        merge(
            &v,
            &a,
            &out,
            "Clip [1080p].mp4",
            &move |f| {
                seen2.lock().unwrap().push(f);
            },
            None,
        )
        .await
        .unwrap();
        let seen = seen.lock().unwrap();
        assert!(!seen.is_empty(), "the join reported no progress");
        assert!(
            seen.iter().all(|f| (0.0..=1.0).contains(f)),
            "progress out of 0..1: {seen:?}"
        );
        assert_eq!(*seen.last().unwrap(), 1.0, "the join ends at 1.0");
        let probe = Command::new(&ffmpeg).arg("-i").arg(&out).output().unwrap();
        let info = String::from_utf8_lossy(&probe.stderr);
        assert!(
            info.contains("Video: mpeg4") && info.contains("Audio: aac"),
            "{info}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn formats() {
        assert_eq!(format_for("a [1080p].mp4"), "mp4");
        assert_eq!(format_for("a.webm"), "webm");
        assert_eq!(format_for("a.mkv"), "matroska");
    }

    /// Signal detection (Q71): a child that kills itself with
    /// SIGSEGV carries signal 11; an ordinary failure carries none.
    #[test]
    #[cfg(unix)]
    fn crash_is_detected_by_signal() {
        let segv = Command::new("sh")
            .args(["-c", "kill -11 $$"])
            .status()
            .unwrap();
        assert_eq!(crash_description(&segv), Some("signal 11".to_string()));
        let fail = Command::new("false").status().unwrap();
        assert_eq!(crash_description(&fail), None);
        let ok = Command::new("true").status().unwrap();
        assert_eq!(crash_description(&ok), None);
    }

    /// The system probe never returns the bundled copy (a bare PATH
    /// name); when it finds one, that binary answers --version or
    /// -version. Skipped silently where no system ffmpeg exists.
    #[test]
    fn system_ffmpeg_probe_answers() {
        let Some(p) = find_system_ffmpeg() else {
            return;
        };
        assert!(
            p.parent().is_some_and(|par| par.as_os_str().is_empty()),
            "system ffmpeg is a PATH lookup, not a file: {}",
            p.display()
        );
        let ok = Command::new(&p)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
            || Command::new(&p)
                .arg("-version")
                .output()
                .is_ok_and(|o| o.status.success());
        assert!(ok, "system ffmpeg at {} does not answer", p.display());
    }

    /// Whether two ffmpeg paths are the same binary for retry
    /// purposes: a bare PATH name always is the system one.
    #[test]
    fn same_binary_check() {
        assert!(same_as_system(Path::new("ffmpeg"), Path::new("ffmpeg")));
        assert!(same_as_system(
            Path::new("ffmpeg"),
            Path::new("/usr/bin/ffmpeg")
        ));
        assert!(!same_as_system(
            Path::new("/opt/ezoo/tools/ffmpeg"),
            Path::new("ffmpeg")
        ));
        assert!(!same_as_system(
            Path::new("/opt/ezoo/tools/ffmpeg"),
            Path::new("/usr/bin/ffmpeg")
        ));
    }

    /// A fake "bundled" ffmpeg that logs and dies by SIGSEGV, and a
    /// fake "system" ffmpeg that logs and writes the output (its
    /// last argument): the retry must run the second one, once.
    #[cfg(unix)]
    fn crashing_and_working_fakes(dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let log = dir.join("ran.log");
        let crashing = dir.join("fake-bundled-ffmpeg");
        std::fs::write(
            &crashing,
            format!(
                "#!/bin/sh\necho bundled >> \"{l}\"\nkill -11 $$\n",
                l = log.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&crashing, std::fs::Permissions::from_mode(0o755)).unwrap();
        let system = dir.join("fake-system-ffmpeg");
        std::fs::write(
            &system,
            format!(
                "#!/bin/sh\necho system >> \"{l}\"\nfor last; do :; done\nprintf 'x' > \"$last\"\n",
                l = log.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&system, std::fs::Permissions::from_mode(0o755)).unwrap();
        (crashing, system, log)
    }

    /// A crash retries once with the system ffmpeg, and only once.
    #[tokio::test]
    #[cfg(unix)]
    async fn crashed_ffmpeg_retries_with_the_system_one() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let dir = std::env::temp_dir().join(format!("edm-retry-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (crashing, system, log) = crashing_and_working_fakes(&dir);
        let out = dir.join("out.mp4");
        let runs = std::sync::Arc::new(AtomicUsize::new(0));
        let runs2 = runs.clone();
        let build = |ff: &Path| {
            runs2.fetch_add(1, Ordering::SeqCst);
            let mut cmd = Command::new(ff);
            cmd.arg(&out);
            Ok(cmd)
        };
        let system2 = system.clone();
        run_ffmpeg_watched_with(&build, &crashing, &out, 1, &|_| {}, None, move || {
            Some(system2.clone())
        })
        .await
        .unwrap();
        assert_eq!(runs.load(Ordering::SeqCst), 2, "first run + one retry");
        assert_eq!(
            std::fs::read_to_string(&log).unwrap(),
            "bundled\nsystem\n",
            "the retry ran the other binary"
        );
        assert!(out.exists(), "the retry produced the output");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A crash with no system ffmpeg to retry with fails with the
    /// clear message (install ffmpeg via the package manager).
    #[tokio::test]
    #[cfg(unix)]
    async fn crashed_ffmpeg_without_a_system_one_says_so_plainly() {
        let dir = std::env::temp_dir().join(format!("edm-retry-nofallback-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (crashing, _, _) = crashing_and_working_fakes(&dir);
        let out = dir.join("out.mp4");
        let build = |ff: &Path| {
            let mut cmd = Command::new(ff);
            cmd.arg(&out);
            Ok(cmd)
        };
        let err = run_ffmpeg_watched_with(&build, &crashing, &out, 1, &|_| {}, None, || None)
            .await
            .unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("bundled ffmpeg crashed"), "{msg}");
        assert!(msg.contains("system package manager"), "{msg}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// When the fallback IS the binary that crashed, there is no
    /// retry: the original crash error comes back, once.
    #[tokio::test]
    #[cfg(unix)]
    async fn crash_of_the_same_binary_is_reported_not_retried() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let dir = std::env::temp_dir().join(format!("edm-retry-same-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (crashing, _, _) = crashing_and_working_fakes(&dir);
        let out = dir.join("out.mp4");
        let runs = std::sync::Arc::new(AtomicUsize::new(0));
        let runs2 = runs.clone();
        let build = |ff: &Path| {
            runs2.fetch_add(1, Ordering::SeqCst);
            let mut cmd = Command::new(ff);
            cmd.arg(&out);
            Ok(cmd)
        };
        let crashing2 = crashing.clone();
        let err = run_ffmpeg_watched_with(&build, &crashing, &out, 1, &|_| {}, None, move || {
            Some(crashing2.clone())
        })
        .await
        .unwrap_err();
        assert_eq!(
            runs.load(Ordering::SeqCst),
            1,
            "no retry with the same binary"
        );
        let msg = format!("{err:#}");
        assert!(msg.contains("ffmpeg crashed (signal 11)"), "{msg}");
        assert!(!msg.contains("package manager"), "{msg}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An ordinary failure (exit 1, no signal) never retries, even
    /// when a fallback exists.
    #[tokio::test]
    async fn ordinary_failure_never_retries() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let dir = std::env::temp_dir().join(format!("edm-retry-ordinary-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("out.mp4");
        let runs = std::sync::Arc::new(AtomicUsize::new(0));
        let runs2 = runs.clone();
        let build = |_ff: &Path| {
            runs2.fetch_add(1, Ordering::SeqCst);
            Ok(Command::new("false"))
        };
        let err =
            run_ffmpeg_watched_with(&build, Path::new("ffmpeg"), &out, 1, &|_| {}, None, || {
                Some(PathBuf::from("ffmpeg"))
            })
            .await
            .unwrap_err();
        assert_eq!(
            runs.load(Ordering::SeqCst),
            1,
            "no retry on an ordinary failure"
        );
        assert!(format!("{err:#}").contains("ffmpeg failed"), "{err:#}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
