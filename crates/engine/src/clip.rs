//! Cut a part out of a video with ffmpeg.
//!
//! Two kinds of cut:
//! * quick (the usual one): stream copy, takes seconds and keeps the
//!   original quality, but a video can only start on a keyframe, so the
//!   part may start a second or two before the time asked — the UI says
//!   so plainly instead of claiming frame accuracy;
//! * exact: the picture is re-encoded (H.264 via libx264) and the part
//!   starts on the very second asked. Slower, and it works the CPU.
//!
//! The input is either a file on disk or a URL served by our own
//! loopback server ([`crate::clipserve`]): ffmpeg then fetches only
//! the bytes it seeks to, so the rest of the video is never downloaded.
//! That URL carries a one-use token — it is passed to ffmpeg through
//! an environment variable, never on the command line where any local
//! user could read it from the process list (see `served_command`).
//!
//! A failed or cancelled cut never leaves a partial file behind: the
//! output is removed on any error, so there is no corrupt-looking
//! clip for the user to open by mistake.

use crate::model::Clip;
use anyhow::{anyhow, bail, Context};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tokio_util::sync::CancellationToken;

/// What ffmpeg cuts from: a file, or a loopback URL whose token must
/// never appear in a process listing.
#[derive(Debug, Clone)]
pub enum CutInput {
    File(PathBuf),
    /// URL from [`crate::clipserve`] (contains the one-use token).
    Served(String),
}

/// How the cut went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Done {
    /// as asked
    AsAsked,
    /// the exact cut could not be done for this video (its picture
    /// codec cannot be re-encoded here, or the re-encode failed):
    /// it was cut the quick way instead — the caller must tell the
    /// user, never silently.
    QuickInstead,
}

/// What the part becomes.
#[derive(Debug, Clone, PartialEq)]
pub enum CutOutput {
    /// picture and sound (MP4)
    Video,
    /// MP3 at this many kb/s, with this title
    Mp3(u64, Option<String>),
    /// sound alone in an MP4 file (.m4a)
    M4a,
}

impl CutOutput {
    /// From the file's name (and "Audio · MP3").
    pub fn for_name(name: &str, mp3: Option<(u64, String)>) -> CutOutput {
        if let Some((kbps, title)) = mp3 {
            return CutOutput::Mp3(kbps, Some(title));
        }
        match crate::filename::extension(name).as_str() {
            "mp3" => CutOutput::Mp3(crate::ytdlp::mp3_kbps(), None),
            "m4a" | "aac" => CutOutput::M4a,
            _ => CutOutput::Video,
        }
    }

    fn is_video(&self) -> bool {
        *self == CutOutput::Video
    }
}

/// What ffmpeg says about the first video stream of an input.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Picture {
    pub codec: String,
    pub width: u32,
    pub height: u32,
}

/// "Stream #0:0[0x1](und): Video: h264 (High) (avc1 / 0x31637661),
/// yuv420p(tv), 1280x720 [SAR 1:1 DAR 16:9], ..." -> the codec + size.
pub fn parse_picture(ffmpeg_says: &str) -> Option<Picture> {
    let line = ffmpeg_says
        .lines()
        .find(|l| l.contains("Stream #") && l.contains(": Video: "))?;
    let after = line.split(": Video: ").nth(1)?;
    let codec = after.split([' ', ',']).next().unwrap_or("").to_string();
    let size = after
        .split([',', ' '])
        .map(str::trim)
        .find_map(|w| {
            let (a, b) = w.split_once('x')?;
            Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?))
        })
        .unwrap_or((0, 0));
    Some(Picture {
        codec,
        width: size.0,
        height: size.1,
    })
}

/// "Duration: 00:03:25.48," -> 205.48
pub fn parse_duration(ffmpeg_says: &str) -> Option<f64> {
    let at = ffmpeg_says.find("Duration: ")? + "Duration: ".len();
    let text = ffmpeg_says[at..].split(',').next()?.trim();
    let mut secs = 0.0;
    for part in text.split(':') {
        secs = secs * 60.0 + part.trim().parse::<f64>().ok()?;
    }
    (secs > 0.0).then_some(secs)
}

/// Picture codecs our ffmpeg can decode (so the exact cut can
/// re-encode them).
fn readable(codec: &str) -> bool {
    matches!(codec, "h264" | "hevc" | "vp8" | "vp9" | "mpeg4")
}

/// The H.264 encoder of this ffmpeg, probed fresh on every call:
/// caching the answer would keep a stale choice if ffmpeg is
/// replaced while the program runs. The fork's bundled build has
/// libx264 and no libopenh264, so libx264 comes first.
fn h264_maker(ffmpeg: &Path) -> Option<&'static str> {
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-hide_banner", "-encoders"]);
    crate::mux::hide_window(&mut cmd);
    let list = cmd
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    ["libx264", "libopenh264", "h264_mf"]
        .into_iter()
        .find(|e| list.contains(&format!(" {e} ")))
}

/// Picture bitrate of the exact cut (kb/s) for a picture of this size.
fn video_kbps(p: &Picture) -> u64 {
    let pixels = (p.width as u64 * p.height as u64).max(640 * 360);
    (pixels * 8000 / (1920 * 1080)).clamp(1200, 40_000)
}

fn base_command(ffmpeg: &Path) -> Command {
    let mut cmd = Command::new(ffmpeg);
    crate::mux::hide_window(&mut cmd);
    cmd.stdin(Stdio::null());
    cmd
}

/// The command that probes `input` (`ffmpeg -i <input>`): a plain
/// file goes on the command line; a served URL goes through an
/// environment variable under `sh`, so the token never appears in
/// the process list. The ffmpeg binary itself rides in
/// `EZOO_FFMPEG`, so the resolved (bundled) build runs — never a
/// bare `ffmpeg` from PATH. On Windows there is no `sh`, so the URL
/// goes on the command line there — the token is still one-use and
/// is revoked as soon as the cut ends.
fn probe_command(ffmpeg: &Path, input: &CutInput) -> Command {
    match input {
        CutInput::File(path) => {
            let mut cmd = base_command(ffmpeg);
            cmd.args(["-hide_banner", "-nostdin", "-i"]).arg(path);
            cmd
        }
        CutInput::Served(url) => {
            if cfg!(unix) {
                let mut cmd = Command::new("sh");
                cmd.args([
                    "-c",
                    "exec \"$EZOO_FFMPEG\" -hide_banner -nostdin -i \"$EZOO_CLIP_URL\"",
                ]);
                cmd.env("EZOO_FFMPEG", ffmpeg);
                cmd.env("EZOO_CLIP_URL", url);
                crate::mux::hide_window(&mut cmd);
                cmd
            } else {
                let mut cmd = base_command(ffmpeg);
                cmd.args(["-hide_banner", "-nostdin", "-i", url]);
                cmd
            }
        }
    }
}

/// What ffmpeg sees in `input`: how long it plays and its picture.
/// One retry with the system ffmpeg when the first binary crashes
/// on the input (Q71: the pinned bundled build segfaults on TS, so
/// without the retry an exact cut would silently degrade to a quick
/// cut for every TS file).
pub async fn look(
    ffmpeg: &Path,
    input: &CutInput,
) -> anyhow::Result<(Option<f64>, Option<Picture>)> {
    // whether the binary we were handed already IS the system one:
    // then a crash has no other binary to retry with
    let first_is_system =
        crate::mux::find_system_ffmpeg().is_some_and(|s| crate::mux::same_as_system(ffmpeg, &s));
    let mut current = ffmpeg.to_path_buf();
    for attempt in 0..2 {
        let mut cmd = probe_command(&current, input);
        cmd.stdout(Stdio::null()).stderr(Stdio::piped());
        let out = tokio::task::spawn_blocking(move || cmd.output())
            .await?
            .context("could not start ffmpeg")?;
        // "At least one output file must be specified": expected here,
        // the input info is on stderr either way.
        let said = String::from_utf8_lossy(&out.stderr);
        if said.contains("Input #0") {
            return Ok((parse_duration(&said), parse_picture(&said)));
        }
        let crashed = crate::mux::crash_description(&out.status);
        if attempt == 0 && crashed.is_some() {
            if let Some(system) = crate::mux::find_system_ffmpeg() {
                if !crate::mux::same_as_system(&current, &system) {
                    current = system;
                    continue;
                }
            }
        }
        let last = said
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("unknown error");
        if let Some(how) = crashed {
            // the bundled build crashed and no system ffmpeg exists
            // to retry with: say so plainly; a crashing system
            // ffmpeg gets the plain fact instead
            if attempt == 0 && !first_is_system {
                bail!(
                    "{}",
                    crate::tools::ffmpeg_crashed_message(
                        cfg!(windows),
                        crate::tools::in_flatpak(),
                        &how
                    )
                );
            }
            bail!("ffmpeg crashed while reading the video ({how})");
        }
        bail!("ffmpeg could not read the video: {last}");
    }
    unreachable!("the probe loop runs at most twice")
}

/// The ffmpeg words for one cut (without the program, the inputs'
/// place and the output file): `-ss` before each input, `-t` after.
/// `faststart` moves the `moov` atom to the front — worth it only
/// for small outputs; a big one would be read and rewritten whole a
/// second time, so the caller passes false for those.
pub fn cut_args(
    inputs: &[CutInput],
    from: f64,
    len: f64,
    exact_with: Option<(&str, u64)>,
    out: &CutOutput,
    faststart: bool,
) -> Vec<String> {
    let mut a: Vec<String> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-y",
        "-nostats",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for input in inputs {
        let arg = match input {
            CutInput::File(p) => p.to_string_lossy().into_owned(),
            CutInput::Served(u) => u.clone(),
        };
        a.extend(["-ss".into(), format!("{from:.3}"), "-i".into(), arg]);
    }
    a.extend(["-t".into(), format!("{len:.3}")]);
    let two = inputs.len() > 1;
    if out.is_video() {
        if two {
            a.extend(["-map", "0:v:0", "-map", "1:a:0?"].map(String::from));
        } else {
            a.extend(["-map", "0:v:0?", "-map", "0:a:0?"].map(String::from));
        }
        match exact_with {
            Some((maker, kbps)) => {
                a.extend([
                    "-c:v".into(),
                    maker.into(),
                    "-b:v".into(),
                    format!("{kbps}k"),
                    "-maxrate".into(),
                    format!("{}k", kbps * 3 / 2),
                ]);
                if maker == "libx264" {
                    a.extend(["-preset", "veryfast"].map(String::from));
                }
                a.extend(
                    ["-vf", "format=yuv420p", "-c:a", "aac", "-b:a", "192k"].map(String::from),
                );
            }
            None => a.extend(["-c", "copy", "-avoid_negative_ts", "make_zero"].map(String::from)),
        }
        if faststart {
            a.extend(["-movflags", "+faststart"].map(String::from));
        }
        a.extend(["-f", "mp4"].map(String::from));
    } else {
        let sound = if two { "1:a:0" } else { "0:a:0" };
        a.extend([
            "-vn".into(),
            "-map".into(),
            sound.into(),
            "-map_metadata".into(),
            "-1".into(),
        ]);
        match out {
            CutOutput::Mp3(kbps, title) => {
                a.extend([
                    "-c:a".into(),
                    "libmp3lame".into(),
                    "-b:a".into(),
                    format!("{kbps}k"),
                    "-id3v2_version".into(),
                    "3".into(),
                ]);
                if let Some(t) = title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
                    a.extend([
                        "-metadata".into(),
                        format!("title={}", t.chars().take(200).collect::<String>()),
                    ]);
                }
                a.extend(["-f", "mp3"].map(String::from));
            }
            CutOutput::M4a => {
                a.extend(["-c:a", "copy"].map(String::from));
                if faststart {
                    a.extend(["-movflags", "+faststart"].map(String::from));
                }
                a.extend(["-f", "mp4"].map(String::from));
            }
            CutOutput::Video => unreachable!(),
        }
    }
    a
}

/// The command that runs one cut: a plain file (or files) goes
/// straight to ffmpeg; a single served URL goes through `sh` with
/// the URL in an environment variable and the resolved ffmpeg in
/// `EZOO_FFMPEG` (see `probe_command`).
/// A served cut with a second input is refused: the second input
/// would be a plain remote address, and mixing it into the shell
/// line is not worth the quoting risk — that case stays for later.
fn cut_command(ffmpeg: &Path, inputs: &[CutInput], args: &[String]) -> anyhow::Result<Command> {
    let served = inputs.iter().any(|i| matches!(i, CutInput::Served(_)));
    if !served {
        let mut cmd = base_command(ffmpeg);
        cmd.args(args);
        return Ok(cmd);
    }
    if inputs.len() != 1 {
        bail!("a cut from a web address with a separate audio stream is not supported yet");
    }
    // args are the six common flags, then "-ss <from> -i <url>
    // -t <len> <rest...>": the -ss/-t numbers go into the shell
    // line (numbers only), the rest rides along as separate
    // arguments (paths with spaces stay whole). The leading flags
    // are checked against the constants cut_args() writes — if
    // that prefix ever changes, this fails loudly instead of
    // silently dropping flags from the cut.
    const COMMON: [&str; 6] = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-y",
        "-nostats",
    ];
    if args.len() < 12
        || args[..6].iter().map(String::as_str).collect::<Vec<_>>() != COMMON
        || args[6] != "-ss"
        || args[8] != "-i"
        || args[10] != "-t"
    {
        bail!("unexpected cut arguments");
    }
    let (from, len) = (args[7].clone(), args[11].clone());
    let rest = &args[12..];
    if cfg!(unix) {
        let mut cmd = Command::new("sh");
        cmd.args([
            "-c",
            "s=$1; t=$2; shift 2; exec \"$EZOO_FFMPEG\" -hide_banner -loglevel error -nostdin -y -nostats -ss \"$s\" -i \"$EZOO_CLIP_URL\" -t \"$t\" \"$@\"",
            "sh",
            &from,
            &len,
        ]);
        cmd.args(rest);
        cmd.env("EZOO_FFMPEG", ffmpeg);
        if let CutInput::Served(url) = &inputs[0] {
            cmd.env("EZOO_CLIP_URL", url);
        }
        crate::mux::hide_window(&mut cmd);
        Ok(cmd)
    } else {
        let mut cmd = base_command(ffmpeg);
        cmd.args(args);
        Ok(cmd)
    }
}

/// Cut `clip` out of `inputs` into `output`. `expected` is the
/// output size in bytes when it can be guessed (for progress; 0 =
/// unknown, progress jumps 0 → 1 at the end). `done(fraction)`
/// while it works; `cancel` stops ffmpeg at once.
///
/// An exact cut that cannot be done falls back to a quick cut and
/// reports [`Done::QuickInstead`] — the caller tells the user; it
/// never happens silently. Any other failure, and a cancel, remove
/// the partial output, so no half-written clip is left behind.
#[allow(clippy::too_many_arguments)]
pub async fn cut(
    ffmpeg: &Path,
    inputs: &[CutInput],
    clip: &Clip,
    out: &CutOutput,
    output: &Path,
    expected: u64,
    done: impl Fn(f64) + Send + Sync,
    cancel: &CancellationToken,
) -> anyhow::Result<Done> {
    clip.validate().map_err(|e| anyhow!(e))?;
    if inputs.is_empty() {
        bail!("nothing to cut");
    }
    let from = clip.start;
    let len = clip.len();
    let faststart = expected > 0 && expected < crate::mux::FASTSTART_UP_TO;
    let mut result = Done::AsAsked;
    let mut exact_with = None;
    if clip.exact && out.is_video() {
        let pic = look(ffmpeg, &inputs[0]).await.ok().and_then(|(_, p)| p);
        match pic {
            Some(p) if readable(&p.codec) => {
                exact_with = h264_maker(ffmpeg).map(|maker| (maker, video_kbps(&p)));
                if exact_with.is_none() {
                    // no H.264 encoder in this ffmpeg: the quick cut
                    // is the only cut there is
                    result = Done::QuickInstead;
                }
            }
            _ => result = Done::QuickInstead,
        }
    }
    match run_once(
        ffmpeg, inputs, from, len, exact_with, out, faststart, output, expected, &done, cancel,
    )
    .await
    {
        Ok(()) => Ok(result),
        Err(e) if e.is::<crate::http_download::Cancelled>() || cancel.is_cancelled() => {
            let _ = std::fs::remove_file(output);
            Err(e)
        }
        Err(_e) if exact_with.is_some() => {
            // the re-encode failed: one quick cut instead, and the
            // caller tells the user it happened
            let _ = std::fs::remove_file(output);
            run_once(
                ffmpeg, inputs, from, len, None, out, faststart, output, expected, &done, cancel,
            )
            .await
            .inspect_err(|_| {
                let _ = std::fs::remove_file(output);
            })?;
            Ok(Done::QuickInstead)
        }
        Err(e) => {
            let _ = std::fs::remove_file(output);
            Err(e)
        }
    }
}

/// One ffmpeg run for [`cut`]: build the words, run them watched
/// (progress + cancel + last stderr line), nothing else.
#[allow(clippy::too_many_arguments)]
async fn run_once(
    ffmpeg: &Path,
    inputs: &[CutInput],
    from: f64,
    len: f64,
    exact_with: Option<(&str, u64)>,
    out: &CutOutput,
    faststart: bool,
    output: &Path,
    expected: u64,
    done: &(dyn Fn(f64) + Send + Sync),
    cancel: &CancellationToken,
) -> anyhow::Result<()> {
    let args = cut_args(inputs, from, len, exact_with, out, faststart);
    // the command is rebuilt per binary: a crash retries with the
    // system ffmpeg (Q71) — for a served cut the shell line reads
    // the binary from $EZOO_FFMPEG, so the rebuild swaps it there.
    // The output file itself rides along as a plain argument after
    // the shell line (never the URL — that stays in the environment).
    let build = |ff: &Path| {
        let mut cmd = cut_command(ff, inputs, &args)?;
        cmd.arg(output);
        Ok(cmd)
    };
    crate::mux::run_ffmpeg_watched(&build, ffmpeg, output, expected, done, Some(cancel)).await
}

/// Parse "mm:ss" or plain seconds ("90", "1:30", "01:02:03") from
/// a time field; `None` when it does not parse. At most three
/// parts ("1:2:3:4" is not a time), and the result must be a finite
/// number ("inf" parses as a float but is not a time).
pub fn parse_time(text: &str) -> Option<f64> {
    let text = text.trim().replace(',', ".");
    if text.is_empty() {
        return None;
    }
    if let Ok(secs) = text.parse::<f64>() {
        return (secs >= 0.0 && secs.is_finite()).then_some(secs);
    }
    let parts: Vec<&str> = text.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    let mut secs = 0.0;
    let mut any = false;
    for part in parts {
        let v: f64 = part.trim().parse().ok()?;
        if !(0.0..60.0).contains(&v) && any {
            return None;
        }
        secs = secs * 60.0 + v;
        any = true;
    }
    any.then_some(secs)
}

/// "205.5" -> "03:25"
pub fn format_time(secs: f64) -> String {
    let total = secs.max(0.0).round() as u64;
    format!("{:02}:{:02}", total / 60, total % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as ProcCommand;

    #[test]
    fn clip_validation_rejects_bad_ranges() {
        assert!(Clip::new(0.0, 10.0, false).is_ok());
        assert!(Clip::new(0.0, 0.5, false).is_ok());
        assert!(Clip::new(0.0, 0.4, false).is_err());
        assert!(Clip::new(5.0, 5.0, false).is_err());
        assert!(Clip::new(10.0, 5.0, false).is_err());
        assert!(Clip::new(-1.0, 5.0, false).is_err());
        assert!(Clip::new(0.0, 24.0 * 3600.0 + 1.0, false).is_err());
        assert!(Clip::new(f64::NAN, 5.0, false).is_err());
        assert!(Clip::new(0.0, f64::INFINITY, false).is_err());
    }

    #[test]
    fn clip_name_says_which_part() {
        let c = Clip::new(80.0, 205.0, false).unwrap();
        assert_eq!(
            crate::model::clip_name("movie.mp4", &c),
            "movie [01-20 → 03-25].mp4"
        );
        assert_eq!(crate::model::clip_name("song", &c), "song [01-20 → 03-25]");
        assert_eq!(crate::model::clip_name("", &c), "video [01-20 → 03-25]");
    }

    #[test]
    fn what_ffmpeg_says() {
        let said = "Input #0, mov,mp4, from 'x':\n  Duration: 00:03:25.48, start: 0.000000, bitrate: 1200 kb/s\n  Stream #0:0[0x1](und): Video: h264 (High) (avc1 / 0x31637661), yuv420p(tv, bt709), 1280x720 [SAR 1:1 DAR 16:9], 1000 kb/s, 30 fps\n  Stream #0:1: Audio: aac";
        assert_eq!(parse_duration(said), Some(205.48));
        assert_eq!(
            parse_picture(said),
            Some(Picture {
                codec: "h264".into(),
                width: 1280,
                height: 720
            })
        );
        assert!(parse_picture("Stream #0:0: Audio: opus").is_none());
        assert_eq!(parse_duration("Duration: N/A, bitrate"), None);
    }

    #[test]
    fn quick_and_exact_words() {
        let ins = vec![
            CutInput::File("v.mp4".into()),
            CutInput::File("a.m4a".into()),
        ];
        let q = cut_args(&ins, 10.0, 5.0, None, &CutOutput::Video, true).join(" ");
        assert!(q.contains("-ss 10.000 -i v.mp4 -ss 10.000 -i a.m4a -t 5.000"));
        assert!(q.contains("-map 0:v:0 -map 1:a:0? -c copy"));
        assert!(q.contains("-movflags +faststart"));
        let big = cut_args(&ins, 10.0, 5.0, None, &CutOutput::Video, false).join(" ");
        assert!(
            !big.contains("+faststart"),
            "big outputs skip the extra pass"
        );
        let e = cut_args(
            &ins[..1],
            10.0,
            5.0,
            Some(("libx264", 4000)),
            &CutOutput::Video,
            true,
        )
        .join(" ");
        assert!(e.contains("-c:v libx264 -b:v 4000k -maxrate 6000k -preset veryfast"));
        assert!(e.contains("-c:a aac -b:a 192k"));
        let m = cut_args(
            &ins[..1],
            1.0,
            2.0,
            None,
            &CutOutput::Mp3(192, Some("Song".into())),
            false,
        )
        .join(" ");
        assert!(
            m.contains("-vn -map 0:a:0")
                && m.contains("libmp3lame -b:a 192k")
                && m.contains("title=Song")
        );
        assert_eq!(
            video_kbps(&Picture {
                codec: "h264".into(),
                width: 1920,
                height: 1080
            }),
            8000
        );
    }

    #[test]
    fn time_fields_parse() {
        assert_eq!(parse_time("90"), Some(90.0));
        assert_eq!(parse_time("1:30"), Some(90.0));
        assert_eq!(parse_time("01:02:03"), Some(3723.0));
        assert_eq!(parse_time(" 2:05.5 "), Some(125.5));
        assert_eq!(parse_time(""), None);
        assert_eq!(parse_time("abc"), None);
        assert_eq!(parse_time("-5"), None);
        assert_eq!(parse_time("1:60"), None);
        // P4-bis: a fourth part is not a time, and "inf" is not a
        // finite time either (it parses as a float, so it must be
        // caught here, not only by Clip::validate further down)
        assert_eq!(parse_time("1:2:3:4"), None);
        assert_eq!(parse_time("inf"), None);
        assert_eq!(parse_time("1:-30"), None);
        assert_eq!(format_time(205.0), "03:25");
        assert_eq!(format_time(5.0), "00:05");
    }

    /// The staged ffmpeg (when present) reports the encoders the
    /// exact cut needs; skipped on machines without ffmpeg.
    #[test]
    fn staged_ffmpeg_has_the_exact_cut_encoder() {
        let Some(ffmpeg) = crate::mux::find_ffmpeg() else {
            return;
        };
        let maker = h264_maker(&ffmpeg);
        assert_eq!(maker, Some("libx264"), "bundled ffmpeg must offer libx264");
    }

    /// The probe retries with the system ffmpeg when the first
    /// binary crashes on the input (Q71): a crashing fake stands
    /// in for the bundled build, the real system ffmpeg for the
    /// fallback — the probe must still read the file.
    #[tokio::test]
    #[cfg(unix)]
    async fn probe_retries_after_a_crash() {
        use std::os::unix::fs::PermissionsExt;
        let Some(system) = crate::mux::find_system_ffmpeg() else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("edm-look-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let src = sample_video(&system);
        let crashing = dir.join("fake-ffmpeg");
        std::fs::write(&crashing, "#!/bin/sh\nkill -11 $$\n").unwrap();
        std::fs::set_permissions(&crashing, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (dur, pic) = look(&crashing, &CutInput::File(src)).await.unwrap();
        assert!(dur.is_some(), "the retry probed the file");
        assert!(pic.is_some(), "the retry read the picture");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn sample_video(ffmpeg: &Path) -> PathBuf {
        let dir = std::env::temp_dir().join("edm-clip-test");
        let _ = std::fs::create_dir_all(&dir);
        let src = dir.join("sample.mp4");
        if !src.exists() {
            let ok = ProcCommand::new(ffmpeg)
                .args([
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc=size=320x240:rate=10:duration=10",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:duration=10",
                    // a keyframe every second: a stream-copy cut can
                    // start near the time asked instead of at 0
                    "-c:v",
                    "libx264",
                    "-g",
                    "10",
                    "-c:a",
                    "aac",
                    "-shortest",
                ])
                .arg(&src)
                .status()
                .expect("ffmpeg runs");
            assert!(ok.success());
        }
        src
    }

    /// A served cut runs the resolved ffmpeg, not bare `ffmpeg`
    /// from PATH: the shell line names `$EZOO_FFMPEG` and the
    /// binary's path rides in the environment next to the URL.
    /// A fake ffmpeg script records what ran it, so the test
    /// proves the resolved path is the one executed.
    #[test]
    #[cfg(unix)]
    fn served_cut_uses_the_resolved_ffmpeg() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join("edm-clip-test");
        let _ = std::fs::create_dir_all(&dir);
        let log = dir.join("fake-ffmpeg.log");
        let fake = dir.join("fake-ffmpeg");
        std::fs::write(
            &fake,
            format!(
                "#!/bin/sh\necho \"ran: $*\" >> \"{l}\"\necho \"EZOO_FFMPEG=$EZOO_FFMPEG\" >> \"{l}\"\necho \"EZOO_CLIP_URL=$EZOO_CLIP_URL\" >> \"{l}\"\n",
                l = log.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let url = "http://127.0.0.1:9/c/one-use-token";

        // the probe: the shell line itself must name the env var,
        // never a bare ffmpeg, and the fake binary must run
        let mut probe = probe_command(&fake, &CutInput::Served(url.into()));
        let words: Vec<String> = probe
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(
            words.iter().any(|w| w.contains("\"$EZOO_FFMPEG\"")),
            "the probe shell line runs the resolved ffmpeg: {words:?}"
        );
        assert!(
            !words.iter().any(|w| w.contains("exec ffmpeg")),
            "no bare ffmpeg from PATH in the probe shell line: {words:?}"
        );
        let _ = std::fs::remove_file(&log);
        probe.stdout(Stdio::null()).stderr(Stdio::null());
        assert!(probe.output().expect("the probe runs").status.success());
        let logged = std::fs::read_to_string(&log).expect("the resolved ffmpeg ran");
        assert!(
            logged.contains(&format!("EZOO_FFMPEG={}", fake.display())),
            "the probe ran the resolved binary: {logged}"
        );
        assert!(
            logged.contains(&format!("EZOO_CLIP_URL={url}")),
            "the probe passed the URL by environment: {logged}"
        );

        // the cut: same discipline for the -ss/-t shell line
        let inputs = vec![CutInput::Served(url.into())];
        let args = cut_args(&inputs, 1.0, 2.0, None, &CutOutput::Video, false);
        let mut cut = cut_command(&fake, &inputs, &args).unwrap();
        let words: Vec<String> = cut
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(words.iter().any(|w| w.contains("\"$EZOO_FFMPEG\"")));
        assert!(!words.iter().any(|w| w.contains("exec ffmpeg")));
        let _ = std::fs::remove_file(&log);
        cut.arg(dir.join("out.mp4"));
        cut.stdout(Stdio::null()).stderr(Stdio::null());
        assert!(cut.output().expect("the cut runs").status.success());
        let logged = std::fs::read_to_string(&log).expect("the resolved ffmpeg ran");
        assert!(logged.contains(&format!("EZOO_FFMPEG={}", fake.display())));
        assert!(logged.contains(&format!("EZOO_CLIP_URL={url}")));
        assert!(
            logged.contains("-ss 1.000") && logged.contains("-t 2.000"),
            "the cut got its times: {logged}"
        );
        let _ = std::fs::remove_file(&log);
        let _ = std::fs::remove_file(&fake);
    }

    /// Real quick cut from a served URL (not a file): the whole
    /// served-cut path — URL, resolved ffmpeg, shell line — end to
    /// end, against a minimal local HTTP server. Skipped without
    /// ffmpeg.
    #[tokio::test]
    async fn served_cut_of_a_real_file() {
        use std::io::{Read, Write};
        let Some(ffmpeg) = crate::mux::find_ffmpeg() else {
            return;
        };
        let src = sample_video(&ffmpeg);
        let data = std::sync::Arc::new(std::fs::read(&src).unwrap());
        // a minimal HTTP server with byte ranges: ffmpeg seeks the
        // file (its index sits at the end), so plain 200 replies to
        // a ranged ask come back as a corrupt cut — the server must
        // answer 206 like the real clipserve does
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop2 = stop.clone();
        std::thread::spawn(move || {
            listener.set_nonblocking(true).unwrap();
            while !stop2.load(std::sync::atomic::Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut s, _)) => {
                        let _ = s.set_nonblocking(false);
                        let mut buf = [0u8; 4096];
                        let mut got = Vec::new();
                        loop {
                            match s.read(&mut buf) {
                                Ok(0) => break,
                                Ok(n) => {
                                    got.extend_from_slice(&buf[..n]);
                                    if got.windows(4).any(|w| w == b"\r\n\r\n") {
                                        break;
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                        let head = String::from_utf8_lossy(&got);
                        let mut start = 0usize;
                        let mut end = data.len();
                        let mut ranged = false;
                        for line in head.lines() {
                            if let Some(r) = line
                                .strip_prefix("Range: bytes=")
                                .or_else(|| line.strip_prefix("range: bytes="))
                            {
                                let (a, b) = r.split_once('-').unwrap_or((r, ""));
                                start = a.parse().unwrap_or(0).min(data.len());
                                end = b
                                    .parse::<usize>()
                                    .map(|e| (e + 1).min(data.len()))
                                    .unwrap_or(data.len());
                                ranged = true;
                            }
                        }
                        let (status, extra) = if ranged {
                            (
                                "206 Partial Content",
                                format!(
                                    "Content-Range: bytes {start}-{}/{}\r\n",
                                    end.saturating_sub(1),
                                    data.len()
                                ),
                            )
                        } else {
                            ("200 OK", String::new())
                        };
                        let head = format!(
                            "HTTP/1.1 {status}\r\nContent-Type: video/mp4\r\nAccept-Ranges: bytes\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n",
                            end - start
                        );
                        let _ = s.write_all(head.as_bytes());
                        let _ = s.write_all(&data[start..end]);
                    }
                    Err(_) => std::thread::sleep(std::time::Duration::from_millis(20)),
                }
            }
        });
        let url = format!("http://127.0.0.1:{port}/sample.mp4");
        let out = src.with_file_name("served.mp4");
        let _ = std::fs::remove_file(&out);
        let clip = Clip::new(2.0, 5.0, false).unwrap();
        let done = cut(
            &ffmpeg,
            &[CutInput::Served(url)],
            &clip,
            &CutOutput::Video,
            &out,
            std::fs::metadata(&src).map(|m| m.len()).unwrap_or(0) / 3,
            &|_| {},
            &CancellationToken::new(),
        )
        .await;
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let done = done.unwrap();
        assert_eq!(done, Done::AsAsked);
        let (dur, pic) = look(&ffmpeg, &CutInput::File(out.clone())).await.unwrap();
        assert!(pic.is_some(), "the served cut must be a readable video");
        let dur = dur.unwrap_or(0.0);
        assert!(
            (2.0..=4.0).contains(&dur),
            "served quick cut lasts about the part: {dur}"
        );
        let _ = std::fs::remove_file(&out);
    }

    /// Real quick cut on a generated file; skipped without ffmpeg.
    #[tokio::test]
    async fn quick_cut_of_a_real_file() {
        let Some(ffmpeg) = crate::mux::find_ffmpeg() else {
            return;
        };
        let src = sample_video(&ffmpeg);
        let out = src.with_file_name("quick.mp4");
        let _ = std::fs::remove_file(&out);
        let clip = Clip::new(2.0, 5.0, false).unwrap();
        let done = cut(
            &ffmpeg,
            &[CutInput::File(src.clone())],
            &clip,
            &CutOutput::Video,
            &out,
            std::fs::metadata(&src).map(|m| m.len()).unwrap_or(0) / 3,
            &|_| {},
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(done, Done::AsAsked);
        let (_, pic) = look(&ffmpeg, &CutInput::File(out.clone())).await.unwrap();
        assert!(pic.is_some(), "the cut file must be a readable video");
        let (dur, _) = look(&ffmpeg, &CutInput::File(out.clone())).await.unwrap();
        let dur = dur.unwrap_or(0.0);
        assert!(
            (2.0..=4.0).contains(&dur),
            "quick cut lasts about the part: {dur}"
        );
        let _ = std::fs::remove_file(&out);
    }

    /// Real exact cut on a generated file; skipped without ffmpeg.
    #[tokio::test]
    async fn exact_cut_is_frame_accurate() {
        let Some(ffmpeg) = crate::mux::find_ffmpeg() else {
            return;
        };
        let src = sample_video(&ffmpeg);
        let out = src.with_file_name("exact.mp4");
        let _ = std::fs::remove_file(&out);
        let clip = Clip::new(2.5, 5.5, true).unwrap();
        let done = cut(
            &ffmpeg,
            &[CutInput::File(src.clone())],
            &clip,
            &CutOutput::Video,
            &out,
            std::fs::metadata(&src).map(|m| m.len()).unwrap_or(0) / 3,
            &|_| {},
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(done, Done::AsAsked);
        let (dur, _) = look(&ffmpeg, &CutInput::File(out.clone())).await.unwrap();
        let dur = dur.unwrap_or(0.0);
        assert!((dur - 3.0).abs() < 0.4, "exact cut lasts the part: {dur}");
        let _ = std::fs::remove_file(&out);
    }

    /// Cancelling mid-cut leaves no output file behind.
    #[tokio::test]
    async fn cancelled_cut_leaves_no_output() {
        let Some(ffmpeg) = crate::mux::find_ffmpeg() else {
            return;
        };
        let src = sample_video(&ffmpeg);
        // a long exact cut of a short file loops the work: cancel
        // while ffmpeg is still re-encoding
        let out = src.with_file_name("cancelled.mp4");
        let _ = std::fs::remove_file(&out);
        let cancel = CancellationToken::new();
        let c2 = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            c2.cancel();
        });
        let clip = Clip::new(0.0, 10.0, true).unwrap();
        let r = cut(
            &ffmpeg,
            &[CutInput::File(src)],
            &clip,
            &CutOutput::Video,
            &out,
            0,
            &|_| {},
            &cancel,
        )
        .await;
        assert!(r.is_err(), "a cancelled cut fails");
        assert!(!out.exists(), "no partial clip is left behind");
    }
}
