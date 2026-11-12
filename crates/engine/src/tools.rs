//! The helper programs that come with Ezoo Download Manager: yt-dlp (finds the video
//! links) and aria2 (torrents) from their GitHub releases, and our own small
//! ffmpeg build (joins video + audio, MP3; built by tools/ffmpeg/build.sh). They live in the "tools" folder
//! of the install folder. Once a day, silently, Ezoo Download Manager checks for new
//! versions and updates them, and puts them back if one went missing.

use crate::http::SendOrdered;
use crate::mux::hide_window;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

const YTDLP_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";
/// the SHA-256 of every file of that release, published by yt-dlp
const YTDLP_SUMS_URL: &str =
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/SHA2-256SUMS";
const YTDLP_LINUX_URL: &str =
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux";
/// none of the helpers is anywhere near this big
const MAX_TOOL_BYTES: u64 = 400 << 20;
/// aria2 for torrents: one fixed, known build (it changes rarely), checked
/// against this SHA-256 before use
pub const ARIA2_URL: &str = "https://github.com/aria2/aria2/releases/download/release-1.37.0/aria2-1.37.0-win-64bit-build1.zip";
pub const ARIA2_ZIP_SHA256: &str =
    "67d015301eef0b612191212d564c5bb0a14b5b9c4796b76454276a4d28d9b288";
pub const ARIA2_EXE_SHA256: &str =
    "be2099c214f63a3cb4954b09a0becd6e2e34660b886d4c898d260febfe9d70c2";

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Folders searched for the tools, best first.
pub fn tool_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        v.push(dir.join("tools"));
        v.push(dir.clone());
        // Linux packages: the .deb/.rpm install the bundled ffmpeg in
        // /usr/lib/ezoo-download while the program itself is /usr/bin/EzooDownload,
        // so it sits one level up from the program's folder. The AppImage
        // keeps ffmpeg next to the program, already covered above.
        // Windows keeps the single tools folder; nothing changes there.
        #[cfg(target_os = "linux")]
        if let Some(parent) = dir.parent() {
            v.push(parent.join("lib").join("ezoo-download"));
        }
    }
    if let Some(p) = directories::ProjectDirs::from("com", "Ezoo", "EzooDownload") {
        v.push(p.data_dir().join("tools"));
    }
    v
}

/// A folder of ours for the helpers' temporary files.
pub fn work_dir() -> Option<PathBuf> {
    let dir = directories::ProjectDirs::from("com", "Ezoo", "EzooDownload")?
        .data_dir()
        .join("tmp");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// A tool shipped with the app (not one somewhere on PATH).
pub fn bundled(name: &str) -> Option<PathBuf> {
    tool_dirs()
        .into_iter()
        .map(|d| d.join(exe(name)))
        .find(|p| p.exists())
}

/// Whether this copy runs inside a Flatpak sandbox (Flatpak sets
/// `FLATPAK_ID` to the app ID; same check as
/// `selfupdate::detect_package`).
pub fn in_flatpak() -> bool {
    std::env::var_os("FLATPAK_ID").is_some()
}

/// Message to show when `aria2c` cannot be found, or `None` when the
/// tool is present and no message is needed. Pure: the caller passes
/// the platform facts in, so every branch is unit-testable.
/// - Windows: the app fetches aria2 itself (the existing wording).
/// - Linux outside a Flatpak: the user installs it with the system
///   package manager.
/// - Flatpak: torrents are not available in this build when aria2c
///   is absent (it is bundled only if the Flatpak module ships it).
pub fn aria2_missing_message(
    is_windows: bool,
    is_flatpak: bool,
    bundled: bool,
) -> Option<&'static str> {
    if bundled {
        return None;
    }
    if is_windows {
        return Some("aria2c.exe is missing. Ezoo Download Manager gets it by itself in a minute (internet needed), then try again");
    }
    if is_flatpak {
        return Some(
            "Torrents are not available in this Flatpak build: aria2c is not bundled with it.",
        );
    }
    Some("aria2c is missing. Install it with your system package manager (for example: sudo apt install aria2 on Debian/Ubuntu, or sudo dnf install aria2 on Fedora), then try again.")
}

/// Message to show when `ffmpeg` cannot be found, or `None` when the
/// tool is present and no message is needed. Pure, same shape as
/// `aria2_missing_message`.
/// - Windows: the existing wording (the setup carries ffmpeg; the UI
///   maps it to its translation — keep the text byte-identical).
/// - Linux outside a Flatpak: reinstall the package (ffmpeg is bundled
///   inside) or install the system ffmpeg.
/// - Flatpak: the bundled ffmpeg is missing from this build.
pub fn ffmpeg_missing_message(
    is_windows: bool,
    is_flatpak: bool,
    bundled: bool,
) -> Option<&'static str> {
    if bundled {
        return None;
    }
    if is_windows {
        return Some("ffmpeg.exe is missing from the \"tools\" folder of Ezoo Download Manager and could not be downloaded again (no internet, or an antivirus removes it): install Ezoo Download Manager again, or allow ffmpeg.exe in the antivirus");
    }
    if is_flatpak {
        return Some("ffmpeg is missing from this Flatpak build, so video and audio cannot be merged. Reinstall the Flatpak from the release, then try again.");
    }
    Some("ffmpeg is missing. Reinstall the Linux package (ffmpeg is bundled inside the .deb, .rpm and AppImage), or install ffmpeg with your system package manager, then try again.")
}

/// Message to show when the bundled ffmpeg CRASHES on a file (Q71)
/// and no system ffmpeg is there to retry with. Pure, same shape as
/// `ffmpeg_missing_message`; `how` is the crash ("signal 11" on
/// Unix, "exit code 0xc0000005" on Windows). Unlike the missing
/// message, the wording is new — no UI mapping exists for it yet,
/// and it deliberately never contains "ffmpeg.exe is missing".
/// - Windows: the bundled build crashed; reinstall or allow it.
/// - Linux outside a Flatpak: install ffmpeg with the package manager.
/// - Flatpak: no system ffmpeg is reachable from the sandbox.
pub fn ffmpeg_crashed_message(is_windows: bool, is_flatpak: bool, how: &str) -> String {
    if is_windows {
        format!("The bundled ffmpeg crashed while handling this file ({how}). Reinstall Ezoo Download Manager, or allow ffmpeg.exe in the antivirus, then try again.")
    } else if is_flatpak {
        format!("The bundled ffmpeg crashed while handling this file ({how}). This Flatpak build cannot use a system ffmpeg — try the .deb, .rpm or AppImage build instead.")
    } else {
        format!("The bundled ffmpeg crashed while handling this file ({how}). Install ffmpeg with your system package manager (for example: sudo apt install ffmpeg on Debian/Ubuntu, or sudo dnf install ffmpeg on Fedora), then try again.")
    }
}

/// Where new copies go: the install folder's "tools" when we may write
/// there, else our data folder.
fn writable_dir(name: &str) -> Option<PathBuf> {
    // keep updating the copy that is already used
    if let Some(p) = bundled(name) {
        if let Some(dir) = p.parent() {
            if can_write(dir) {
                return Some(dir.to_path_buf());
            }
        }
    }
    tool_dirs()
        .into_iter()
        .find(|d| std::fs::create_dir_all(d).is_ok() && can_write(d))
}

fn can_write(dir: &Path) -> bool {
    let probe = dir.join(".ezoo-write-test");
    let ok = std::fs::write(&probe, b"x").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

fn due(stamp: &Path, every: Duration) -> bool {
    std::fs::metadata(stamp)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .map(|age| age > every)
        .unwrap_or(true)
}

/// The SHA-256 of `name` in a checksum list ("<sha256>  <name>" per line).
pub fn sum_for(list: &str, name: &str) -> Option<String> {
    list.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let (sum, file) = (it.next()?, it.next()?);
        (file.trim_start_matches('*') == name
            && sum.len() == 64
            && sum.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| sum.to_ascii_lowercase())
    })
}

async fn get_text(client: &reqwest::Client, url: &str) -> anyhow::Result<String> {
    let text = client
        .get(url)
        .header(
            reqwest::header::USER_AGENT,
            crate::config::DEFAULT_USER_AGENT,
        )
        .send_ordered()
        .await?
        .error_for_status()?
        .text()
        .await?;
    if text.len() > 1 << 20 {
        anyhow::bail!("unexpected answer from {url}");
    }
    Ok(text)
}

/// Download `url` to `dest`. It is used only when its SHA-256 is `sha256`
/// (from the checksum list its makers publish): a changed or broken file is
/// thrown away and never run.
async fn download(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    sha256: &str,
) -> anyhow::Result<()> {
    use sha2::{Digest, Sha256};
    let mut resp = client
        .get(url)
        .header(
            reqwest::header::USER_AGENT,
            crate::config::DEFAULT_USER_AGENT,
        )
        .send_ordered()
        .await?
        .error_for_status()?;
    use std::io::Write;
    let tmp = dest.with_extension("download");
    let mut file = std::fs::File::create(&tmp)?;
    let mut h = Sha256::new();
    let mut total: u64 = 0;
    while let Some(chunk) = resp.chunk().await? {
        total += chunk.len() as u64;
        if total > MAX_TOOL_BYTES {
            drop(file);
            let _ = std::fs::remove_file(&tmp);
            anyhow::bail!("the download of {url} is far bigger than expected");
        }
        h.update(&chunk);
        file.write_all(&chunk)?;
    }
    drop(file);
    let got: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if !got.eq_ignore_ascii_case(sha256) {
        let _ = std::fs::remove_file(&tmp);
        anyhow::bail!("the download of {url} does not match its published SHA-256");
    }
    replace(&tmp, dest)
}

/// Put `new` in place of `target`, even if `target` is running right now
/// (Windows lets us rename a running program, not delete it). On Unix
/// the result must be runnable: a file downloaded from the internet
/// has no execute bit, and without it the tool silently never runs (Q49).
fn replace(new: &Path, target: &Path) -> anyhow::Result<()> {
    if target.exists() {
        let old = target.with_extension("old");
        let _ = std::fs::remove_file(&old);
        if std::fs::remove_file(target).is_err() {
            std::fs::rename(target, &old)?;
        }
    }
    std::fs::rename(new, target)?;
    make_executable(target);
    Ok(())
}

/// Mark a tool runnable on Unix (mode 0755). A no-op on Windows, and
/// best-effort everywhere: a failure to set the bit is reported by the
/// caller failing to run the tool, not by an error here.
pub fn make_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

/// yt-dlp missing: get it. Present: `yt-dlp -U` (at most once a day).
async fn yt_dlp(client: &reqwest::Client, data_dir: &Path) {
    if bundled("yt-dlp").is_none() {
        if cfg!(windows) {
            if let Some(dir) = writable_dir("yt-dlp") {
                if let Ok(sums) = get_text(client, YTDLP_SUMS_URL).await {
                    if let Some(sum) = sum_for(&sums, "yt-dlp.exe") {
                        let _ = download(client, YTDLP_URL, &dir.join(exe("yt-dlp")), &sum).await;
                    }
                }
            }
        } else {
            // Linux: the same release page carries the standalone build,
            // checked against the published sums like the Windows one.
            if let Some(dir) = writable_dir("yt-dlp") {
                if let Ok(sums) = get_text(client, YTDLP_SUMS_URL).await {
                    if let Some(sum) = sum_for(&sums, "yt-dlp_linux") {
                        let _ =
                            download(client, YTDLP_LINUX_URL, &dir.join(exe("yt-dlp")), &sum).await;
                    }
                }
            }
        }
        return;
    }
    // a bundled copy from an older version may have lost its execute
    // bit when it was downloaded (Q49): heal it on every check, so
    // the tool runs instead of failing silently
    if let Some(tool) = bundled("yt-dlp") {
        make_executable(&tool);
    }
    let stamp = data_dir.join("yt-dlp-updated");
    if !due(&stamp, Duration::from_secs(20 * 3600)) {
        return;
    }
    let Some(tool) = bundled("yt-dlp") else {
        return;
    };
    let ok = tokio::task::spawn_blocking(move || {
        let mut cmd = Command::new(&tool);
        if let Some(dir) = work_dir() {
            cmd.env("TEMP", &dir).env("TMP", &dir).current_dir(&dir);
        }
        cmd.args(["--ignore-config", "-U"]);
        hide_window(&mut cmd);
        cmd.output().map(|o| o.status.success()).unwrap_or(false)
    })
    .await
    .unwrap_or(false);
    if ok {
        let _ = std::fs::write(&stamp, b"ok");
    }
}

/// ffmpeg comes with the install (our own small build on Windows, the
/// bundled static build on Linux) and is updated with the program. Copies
/// of the big public build that older versions downloaded by themselves
/// are removed, so ours is the one used.
fn ours_ffmpeg() -> Option<PathBuf> {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))?;
    let tools = dir.join("tools").join(exe("ffmpeg"));
    if tools.exists() {
        return Some(tools);
    }
    // Linux .deb/.rpm layout: /usr/lib/ezoo-download/ffmpeg next to /usr/bin.
    #[cfg(target_os = "linux")]
    if let Some(parent) = dir.parent() {
        let lib = parent.join("lib").join("ezoo-download").join(exe("ffmpeg"));
        if lib.exists() {
            return Some(lib);
        }
    }
    None
}

fn ffmpeg_cleanup() {
    if ours_ffmpeg().is_none() {
        return;
    }
    if let Some(p) = directories::ProjectDirs::from("com", "Ezoo", "EzooDownload") {
        let dir = p.data_dir().join("tools");
        for f in [
            exe("ffmpeg"),
            "ffmpeg-version.txt".into(),
            "ffmpeg-LICENSE.txt".into(),
        ] {
            let _ = std::fs::remove_file(dir.join(f));
        }
    }
}

/// ffmpeg missing (deleted by mistake or by an antivirus): fetch our
/// release's copy, but only when its SHA-256 is the one this build was
/// made with (baked in by the release workflow). Never from anywhere else:
/// the address comes from our own repository constant. On Linux the
/// installer already carries ffmpeg, so nothing is fetched there.
const FFMPEG_SHA256: Option<&str> = option_env!("EZOO_FFMPEG_SHA256");
pub async fn restore_ffmpeg(client: &reqwest::Client) -> bool {
    if ours_ffmpeg().is_some() || bundled("ffmpeg").is_some() {
        return true;
    }
    let Some(hash) = FFMPEG_SHA256.filter(|h| h.len() == 64) else {
        return false;
    };
    if !cfg!(windows) {
        return false; // Linux ships ffmpeg inside the packages
    }
    let version = env!("CARGO_PKG_VERSION");
    let repo = crate::selfupdate::REPO;
    let url = format!("https://github.com/{repo}/releases/download/v{version}/ffmpeg.exe");
    let latest = format!("https://github.com/{repo}/releases/latest/download/ffmpeg.exe");
    let Some(dir) = writable_dir("ffmpeg") else {
        return false;
    };
    let dest = dir.join(exe("ffmpeg"));
    download(client, &url, &dest, hash).await.is_ok()
        || download(client, &latest, &dest, hash).await.is_ok()
}

/// aria2c missing (an older install): get the known build.
async fn aria2(client: &reqwest::Client) -> anyhow::Result<()> {
    if !cfg!(windows) || bundled("aria2c").is_some() {
        return Ok(());
    }
    let Some(dir) = writable_dir("aria2c") else {
        return Ok(());
    };
    let work = std::env::temp_dir().join(format!("ezoo-aria2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;
    let zip = work.join("aria2.zip");
    download(client, ARIA2_URL, &zip, ARIA2_ZIP_SHA256).await?;
    let (zip2, work2) = (zip.clone(), work.clone());
    let unpacked = tokio::task::spawn_blocking(move || {
        let mut cmd = Command::new(crate::system::windows_tool("tar.exe"));
        cmd.arg("-xf").arg(&zip2).arg("-C").arg(&work2);
        hide_window(&mut cmd);
        cmd.output().map(|o| o.status.success()).unwrap_or(false)
    })
    .await
    .unwrap_or(false);
    let found = unpacked.then(|| find_file(&work, &exe("aria2c"))).flatten();
    let result = match found {
        Some(f) if sha256_of(&f).as_deref() == Some(ARIA2_EXE_SHA256) => {
            let staged = dir.join("aria2c.new");
            std::fs::copy(&f, &staged)?;
            replace(&staged, &dir.join(exe("aria2c")))
        }
        Some(_) => Err(anyhow::anyhow!(
            "aria2c.exe does not match its known SHA-256"
        )),
        None => Err(anyhow::anyhow!("aria2c.exe was not found in the download")),
    };
    let _ = std::fs::remove_dir_all(&work);
    result
}

fn sha256_of(path: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let data = std::fs::read(path).ok()?;
    Some(
        Sha256::digest(&data)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if let Some(f) = find_file(&p, name) {
                return Some(f);
            }
        } else if p
            .file_name()
            .map(|n| n.eq_ignore_ascii_case(name))
            .unwrap_or(false)
        {
            return Some(p);
        }
    }
    None
}

/// Check both tools now (only does work when a day has passed or one is
/// missing). Never shows anything; errors just mean "try again later".
pub async fn update_all(client: reqwest::Client, data_dir: PathBuf) {
    yt_dlp(&client, &data_dir).await;
    let _ = restore_ffmpeg(&client).await;
    ffmpeg_cleanup();
    let _ = aria2(&client).await;
    // leftovers of earlier updates
    for dir in tool_dirs() {
        for name in ["yt-dlp", "ffmpeg", "aria2c"] {
            let _ = std::fs::remove_file(dir.join(exe(name)).with_extension("old"));
        }
    }
}

/// Run `update_all` now and then every few hours while the app is open.
pub fn spawn_updater(rt: &tokio::runtime::Handle, client: reqwest::Client, data_dir: PathBuf) {
    rt.spawn(async move {
        // let the app start first
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            update_all(client.clone(), data_dir.clone()).await;
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_messages_are_platform_specific() {
        // bundled: no message on any platform — the tool is there
        assert_eq!(aria2_missing_message(true, false, true), None);
        assert_eq!(aria2_missing_message(false, false, true), None);
        assert_eq!(aria2_missing_message(false, true, true), None);
        assert_eq!(ffmpeg_missing_message(true, false, true), None);
        assert_eq!(ffmpeg_missing_message(false, false, true), None);
        assert_eq!(ffmpeg_missing_message(false, true, true), None);
        // Windows keeps the fetch wording (aria2) and the exact old
        // wording (ffmpeg — the UI maps it to its translation)
        assert_eq!(
            aria2_missing_message(true, false, false),
            Some("aria2c.exe is missing. Ezoo Download Manager gets it by itself in a minute (internet needed), then try again")
        );
        assert_eq!(
            ffmpeg_missing_message(true, false, false),
            Some("ffmpeg.exe is missing from the \"tools\" folder of Ezoo Download Manager and could not be downloaded again (no internet, or an antivirus removes it): install Ezoo Download Manager again, or allow ffmpeg.exe in the antivirus")
        );
        // Linux outside a Flatpak: install with the package manager,
        // never "the app gets it by itself"
        let m = aria2_missing_message(false, false, false).unwrap();
        assert!(m.contains("aria2c is missing"));
        assert!(m.contains("package manager"));
        assert!(!m.contains("gets it by itself"));
        let m = ffmpeg_missing_message(false, false, false).unwrap();
        assert!(m.contains("ffmpeg is missing"));
        assert!(m.contains("system package manager"));
        assert!(!m.contains("antivirus"));
        // Flatpak: the tool is not available in this build
        let m = aria2_missing_message(false, true, false).unwrap();
        assert!(m.contains("not available in this Flatpak build"));
        let m = ffmpeg_missing_message(false, true, false).unwrap();
        assert!(m.contains("missing from this Flatpak build"));
        assert!(!m.contains("antivirus"));
    }

    #[test]
    fn crashed_message_points_at_the_package_manager() {
        // Linux: the bundled build crashed, install the system one
        let m = ffmpeg_crashed_message(false, false, "signal 11");
        assert!(m.contains("bundled ffmpeg crashed"), "{m}");
        assert!(m.contains("signal 11"), "{m}");
        assert!(m.contains("system package manager"), "{m}");
        assert!(m.contains("sudo apt install ffmpeg"), "{m}");
        // Flatpak: no system ffmpeg is reachable from the sandbox
        let m = ffmpeg_crashed_message(false, true, "signal 11");
        assert!(m.contains("bundled ffmpeg crashed"), "{m}");
        assert!(m.contains("Flatpak"), "{m}");
        assert!(!m.contains("package manager"), "{m}");
        // Windows: same shape, and it must never trip the UI's
        // missing-ffmpeg mapping ("ffmpeg.exe is missing")
        let m = ffmpeg_crashed_message(true, false, "exit code 0xc0000005");
        assert!(m.contains("bundled ffmpeg crashed"), "{m}");
        assert!(m.contains("0xc0000005"), "{m}");
        assert!(!m.contains("ffmpeg.exe is missing"), "{m}");
    }

    #[test]
    fn replace_and_find() {
        let dir = std::env::temp_dir().join(format!("ezoo-tools-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a/b")).unwrap();
        std::fs::write(dir.join("a/b/ffmpeg.exe"), b"new").unwrap();
        assert!(find_file(&dir, "FFMPEG.EXE").is_some());
        let target = dir.join("tool.exe");
        std::fs::write(&target, b"old").unwrap();
        let new = dir.join("tool.new");
        std::fs::write(&new, b"fresh").unwrap();
        replace(&new, &target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"fresh");
        // Q49: a replaced tool is runnable on Unix, even though the
        // downloaded file had no execute bit
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o755, "replaced tool is not executable: {mode:o}");
            // and a copy that lost its bit is healed the same way
            let lost = dir.join("lost-bit");
            std::fs::write(&lost, b"x").unwrap();
            std::fs::set_permissions(&lost, std::fs::Permissions::from_mode(0o644)).unwrap();
            make_executable(&lost);
            assert_eq!(
                std::fs::metadata(&lost).unwrap().permissions().mode() & 0o777,
                0o755
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
