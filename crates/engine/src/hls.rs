//! HLS (.m3u8) video streams: download every part with several connections,
//! decrypt AES-128 parts when needed, then join them into one video file.
//!
//! DRM protected streams (SAMPLE-AES, Widevine, FairPlay...) are refused on
//! purpose. Live streams (no #EXT-X-ENDLIST) are refused too.

use crate::http::{request, ConnGuard, Progress, Shared};
use crate::http_download::Cancelled;
use crate::m3u8::{best_variant, parse_media};
use crate::model::RequestHeaders;
use anyhow::{anyhow, bail, Context};
use reqwest::header::RANGE;
use reqwest::StatusCode;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

/// An HLS stream download: playlist URL, request data, output paths, and limits.
#[derive(Clone)]
pub struct HlsJob {
    pub url: String,
    pub headers: RequestHeaders,
    pub user_agent: String,
    /// final output file (written as .sgpart first by the caller)
    pub output: PathBuf,
    pub temp_dir: PathBuf,
    pub connections: u32,
    pub retries: u32,
    pub limiter: Arc<crate::limiter::SpeedLimiter>,
}

fn decrypt(key: &[u8], iv: &[u8; 16], data: &mut Vec<u8>) -> anyhow::Result<()> {
    use aes::Aes128;
    use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
    let dec = cbc::Decryptor::<Aes128>::new_from_slices(key, iv)
        .map_err(|_| anyhow!("invalid AES key length"))?;
    let len = dec
        .decrypt_padded_mut::<Pkcs7>(data)
        .map_err(|_| anyhow!("could not decrypt stream part"))?
        .len();
    data.truncate(len);
    Ok(())
}

/// Fetch a whole file, or one slice of it (start, end): when a slice is
/// asked for, the server should answer 206; when it answers 200 instead,
/// the slice is cut out of the whole file. Returns the bytes and the
/// address they really came from (after redirects).
async fn fetch_full(
    shared: &Shared,
    job: &HlsJob,
    url: &str,
    range: Option<(u64, u64)>,
) -> anyhow::Result<(Vec<u8>, String)> {
    let mut rb = request(&shared.client(), url, &job.headers, &job.user_agent);
    if let Some((start, end)) = range {
        rb = rb.header(RANGE, format!("bytes={start}-{}", end - 1));
    }
    let resp = rb.send().await?;
    if !resp.status().is_success() {
        bail!("HTTP {} for {}", resp.status().as_u16(), url);
    }
    let status = resp.status();
    let final_url = resp.url().to_string();
    let data = resp.bytes().await?.to_vec();
    if let Some((start, end)) = range {
        if status == StatusCode::PARTIAL_CONTENT {
            return Ok((data, final_url));
        }
        let start = start as usize;
        let end = end.min(data.len() as u64) as usize;
        if data.len() < start {
            bail!("the server sent fewer bytes than the slice asked for");
        }
        return Ok((data[start..end].to_vec(), final_url));
    }
    Ok((data, final_url))
}

async fn fetch_text(shared: &Shared, job: &HlsJob, url: &str) -> anyhow::Result<(String, String)> {
    let (b, final_url) = fetch_full(shared, job, url, None).await?;
    Ok((String::from_utf8_lossy(&b).into_owned(), final_url))
}

fn part_file(dir: &Path, i: usize) -> PathBuf {
    dir.join(format!("{i:06}.part"))
}

pub async fn run(
    shared: Arc<Shared>,
    job: HlsJob,
    progress: Arc<Progress>,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let job = Arc::new(job);
    // 1. playlist (follow a master playlist to its best quality)
    let mut url = job.url.clone();
    let (mut text, final_url) = fetch_text(&shared, &job, &url)
        .await
        .context("could not load playlist")?;
    url = final_url;
    if !text.trim_start().starts_with("#EXTM3U") {
        bail!("not a valid HLS playlist");
    }
    if text.contains("#EXT-X-STREAM-INF") {
        url = best_variant(&url, &text)?.ok_or_else(|| anyhow!("no video variant found"))?;
        (text, url) = fetch_text(&shared, &job, &url).await?;
    }
    let playlist = parse_media(&url, &text)?;
    if !playlist.ended {
        bail!("live streams are not supported (the playlist has no end)");
    }
    if playlist.parts.is_empty() {
        bail!("the playlist has no parts");
    }
    std::fs::create_dir_all(&job.temp_dir)?;

    // 2. keys (usually one for the whole stream)
    let mut keys: HashMap<String, Vec<u8>> = HashMap::new();
    for p in &playlist.parts {
        if let Some(k) = &p.key {
            if !keys.contains_key(&k.uri) {
                let (bytes, _) = fetch_full(&shared, &job, &k.uri, None)
                    .await
                    .context("could not load key")?;
                keys.insert(k.uri.clone(), bytes);
            }
        }
    }
    let keys = Arc::new(keys);
    let parts = Arc::new(playlist.parts);

    // 3. parts, several at a time; finished parts survive pause/resume
    let total = parts.len();
    progress.parts_total.store(total as u32, Ordering::Relaxed);
    let mut done_bytes = 0u64;
    let mut done = 0u32;
    for i in 0..total {
        if let Ok(m) = std::fs::metadata(part_file(&job.temp_dir, i)) {
            done += 1;
            done_bytes += m.len();
        }
    }
    progress.parts_done.store(done, Ordering::Relaxed);
    progress.bytes.store(done_bytes, Ordering::Relaxed);

    let next = Arc::new(AtomicUsize::new(0));
    let child = cancel.child_token();
    let mut set = JoinSet::new();
    for _ in 0..job.connections.clamp(1, 16) {
        let (shared, job, progress, child, next, parts, keys) = (
            shared.clone(),
            job.clone(),
            progress.clone(),
            child.clone(),
            next.clone(),
            parts.clone(),
            keys.clone(),
        );
        set.spawn(async move {
            loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= parts.len() {
                    return Ok::<(), anyhow::Error>(());
                }
                let path = part_file(&job.temp_dir, i);
                if path.exists() {
                    continue;
                }
                let mut attempt = 0;
                loop {
                    let fut = async {
                        let _g = ConnGuard::new(&progress);
                        let (mut data, _) =
                            fetch_full(&shared, &job, &parts[i].url, parts[i].range).await?;
                        shared.limiter.acquire(data.len()).await;
                        job.limiter.acquire(data.len()).await;
                        if let Some(k) = &parts[i].key {
                            let key = keys.get(&k.uri).ok_or_else(|| anyhow!("missing key"))?;
                            let iv =
                                k.iv.unwrap_or_else(|| (parts[i].sequence as u128).to_be_bytes());
                            decrypt(key, &iv, &mut data)?;
                        }
                        let tmp = path.with_extension("tmp");
                        std::fs::write(&tmp, &data)?;
                        std::fs::rename(&tmp, &path)?;
                        Ok::<u64, anyhow::Error>(data.len() as u64)
                    };
                    let r = tokio::select! {
                        r = fut => r,
                        _ = child.cancelled() => return Err(Cancelled.into()),
                    };
                    match r {
                        Ok(n) => {
                            progress.bytes.fetch_add(n, Ordering::Relaxed);
                            progress.parts_done.fetch_add(1, Ordering::Relaxed);
                            break;
                        }
                        Err(e) => {
                            attempt += 1;
                            if attempt > job.retries {
                                return Err(e);
                            }
                            tokio::time::sleep(Duration::from_secs((attempt as u64 * 2).min(20)))
                                .await;
                        }
                    }
                }
            }
        });
    }
    let mut first_error = None;
    while let Some(r) = set.join_next().await {
        let r = r
            .map_err(|e| anyhow!("worker crashed: {e}"))
            .and_then(|r| r);
        if let Err(e) = r {
            if first_error.is_none() && !cancel.is_cancelled() {
                first_error = Some(e);
            }
            child.cancel();
        }
    }
    if cancel.is_cancelled() {
        return Err(Cancelled.into());
    }
    if let Some(e) = first_error {
        return Err(e);
    }

    // 4. join everything into one file
    progress.assembling.store(true, Ordering::Relaxed);
    let init = match &playlist.init_url {
        Some(u) => Some(fetch_full(&shared, &job, u, playlist.init_range).await?.0),
        None => None,
    };
    let temp_dir = job.temp_dir.clone();
    let output = job.output.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let mut out = std::io::BufWriter::new(std::fs::File::create(&output)?);
        if let Some(init) = init {
            out.write_all(&init)?;
        }
        for i in 0..total {
            let mut f = std::fs::File::open(part_file(&temp_dir, i))?;
            std::io::copy(&mut f, &mut out)?;
        }
        out.flush()?;
        drop(out);
        let _ = std::fs::remove_dir_all(&temp_dir);
        Ok(())
    })
    .await??;
    progress.assembling.store(false, Ordering::Relaxed);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limiter::SpeedLimiter;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const MASTER: &str = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=5000000,RESOLUTION=1920x1080\nhigh.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360\nlow.m3u8\n";
    const HIGH: &str = "#EXTM3U\n#EXT-X-TARGETDURATION:10\n#EXTINF:10.0,\nhigh0.ts\n#EXTINF:10.0,\nhigh1.ts\n#EXT-X-ENDLIST\n";
    const LOW: &str = "#EXTM3U\n#EXT-X-TARGETDURATION:10\n#EXTINF:10.0,\nlow0.ts\n#EXTINF:10.0,\nlow1.ts\n#EXT-X-ENDLIST\n";

    /// A tiny server for the playlists and parts above.
    async fn stream_server() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = s.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                    let (body, ctype): (Vec<u8>, &str) = match path.as_str() {
                        "/master.m3u8" => (MASTER.into(), "application/vnd.apple.mpegurl"),
                        "/high.m3u8" => (HIGH.into(), "application/vnd.apple.mpegurl"),
                        "/low.m3u8" => (LOW.into(), "application/vnd.apple.mpegurl"),
                        "/high0.ts" => (b"HIGH-ZERO".to_vec(), "video/mp2t"),
                        "/high1.ts" => (b"HIGH-ONE".to_vec(), "video/mp2t"),
                        "/low0.ts" => (b"LOW-ZERO".to_vec(), "video/mp2t"),
                        "/low1.ts" => (b"LOW-ONE".to_vec(), "video/mp2t"),
                        _ => (b"not found".to_vec(), "text/plain"),
                    };
                    let status = if body == b"not found" {
                        "404 Not Found"
                    } else {
                        "200 OK"
                    };
                    let head = format!(
                        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: {ctype}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = s.write_all(head.as_bytes()).await;
                    let _ = s.write_all(&body).await;
                });
            }
        });
        format!("http://{addr}")
    }

    fn shared() -> Arc<Shared> {
        Arc::new(Shared::new(
            reqwest::Client::builder().no_proxy().build().unwrap(),
            SpeedLimiter::new(0),
        ))
    }

    /// Q7 steering (Phase 6): a variant's own playlist downloads exactly
    /// that variant; the master on its own still downloads the best one.
    #[tokio::test]
    async fn variant_url_downloads_that_variant_only() {
        let base = stream_server().await;
        let dir = std::env::temp_dir().join(format!("ezoo-hls-steer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let shared = shared();
        // the picked (low) variant's playlist: only its parts land in the file
        let job = HlsJob {
            url: format!("{base}/low.m3u8"),
            headers: RequestHeaders::default(),
            user_agent: "test".to_string(),
            output: dir.join("low.bin"),
            temp_dir: dir.join("low-parts"),
            connections: 2,
            retries: 1,
            limiter: Arc::new(SpeedLimiter::new(0)),
        };
        run(
            shared.clone(),
            job,
            Arc::new(Progress::default()),
            CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read(dir.join("low.bin")).unwrap(),
            b"LOW-ZEROLOW-ONE"
        );
        // the master on its own: the best variant wins, as before
        let job = HlsJob {
            url: format!("{base}/master.m3u8"),
            headers: RequestHeaders::default(),
            user_agent: "test".to_string(),
            output: dir.join("best.bin"),
            temp_dir: dir.join("best-parts"),
            connections: 2,
            retries: 1,
            limiter: Arc::new(SpeedLimiter::new(0)),
        };
        run(
            shared,
            job,
            Arc::new(Progress::default()),
            CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read(dir.join("best.bin")).unwrap(),
            b"HIGH-ZEROHIGH-ONE"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
