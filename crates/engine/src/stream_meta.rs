//! Stream metadata for the capture window: duration, qualities and
//! estimated size of HLS (.m3u8) and DASH (.mpd) streams, probed with the
//! captured headers (User-Agent, Referer, Cookie).
//!
//! Metadata only: nothing here downloads a stream, and the captured
//! headers are used for these requests alone — they are never stored,
//! logged, or put into any result. The whole probe is bounded by a
//! ~5 second deadline; a slow server yields "unknown", never a hang.
//! DRM-signaled streams are reported, never downloaded.

use crate::http::request;
use crate::m3u8;
use crate::model::{DownloadKind, RequestHeaders};
use crate::mpd;
use anyhow::{anyhow, bail};
use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::{Client, StatusCode};
use std::time::{Duration, Instant};

/// How an estimated size was produced, shown in the UI next to the size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeSource {
    /// From AVERAGE-BANDWIDTH / BANDWIDTH tags in the playlist/manifest.
    Bitrate,
    /// From sampling 2-3 segments and extrapolating.
    Sampled,
    /// No bitrate and no usable sample: no estimate at all.
    Unknown,
}

/// One quality of a stream, as the capture window's selector shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct VariantInfo {
    pub width: u32,
    pub height: u32,
    pub frame_rate: Option<f32>,
    pub codecs: String,
    /// Video bitrate in bit/s (0 = not tagged and not sampled).
    pub video_bps: u64,
    /// Audio bitrate in bit/s, kept separate from the video bitrate.
    pub audio_bps: u64,
    pub duration_secs: Option<f64>,
    pub estimated_size: Option<u64>,
    pub size_source: SizeSource,
    /// The media playlist (HLS) or manifest (DASH) this variant came from.
    pub url: String,
    pub is_audio_only: bool,
}

impl VariantInfo {
    /// ((video bps + audio bps) x duration) / 8, always an estimate.
    fn estimate(&mut self) {
        let bps = self.video_bps.saturating_add(self.audio_bps);
        self.estimated_size = match (bps, self.duration_secs) {
            (b, Some(d)) if b > 0 && d > 0.0 => Some(((b as f64 * d) / 8.0).round() as u64),
            _ => None,
        };
    }
}

/// What the capture window shows for one captured stream link.
#[derive(Debug, Clone, PartialEq)]
pub struct StreamMeta {
    pub kind: DownloadKind,
    pub duration_secs: Option<f64>,
    pub variants: Vec<VariantInfo>,
    /// A DRM signal and what it is, in plain words: the window refuses
    /// the stream and shows this instead of starting a download.
    pub drm: Option<String>,
}

/// Total budget for one probe, from the capture window's point of view.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
/// Segments sampled when no bitrate tag is present.
const SAMPLE_SEGMENTS: usize = 3;
/// A playlist/manifest larger than this is refused (it is metadata, not
/// a download).
const MAX_TEXT_BYTES: usize = 4 << 20;

async fn fetch_text(
    client: &Client,
    url: &str,
    headers: &RequestHeaders,
    default_ua: &str,
    deadline: Instant,
) -> anyhow::Result<String> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    let resp = tokio::time::timeout(remaining, request(client, url, headers, default_ua).send())
        .await
        .map_err(|_| anyhow!("the stream details took too long to load"))??;
    if !resp.status().is_success() {
        bail!("server answered HTTP {}", resp.status().as_u16());
    }
    let bytes = tokio::time::timeout(remaining, resp.bytes())
        .await
        .map_err(|_| anyhow!("the stream details took too long to load"))??;
    if bytes.len() > MAX_TEXT_BYTES {
        bail!("the stream's list is too big");
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// The size of one segment without downloading it: a ranged GET for a
/// single byte reports the whole segment in Content-Range; a HEAD
/// Content-Length is the fallback. `None` when neither answers.
async fn segment_size(
    client: &Client,
    url: &str,
    headers: &RequestHeaders,
    default_ua: &str,
    deadline: Instant,
) -> Option<u64> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return None;
    }
    let rb = request(client, url, headers, default_ua).header(RANGE, "bytes=0-0");
    if let Ok(resp) = tokio::time::timeout(remaining, rb.send()).await.ok()? {
        if resp.status() == StatusCode::PARTIAL_CONTENT {
            let total = resp
                .headers()
                .get(CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.rsplit('/').next())
                .and_then(|s| s.trim().parse::<u64>().ok());
            if total.is_some() {
                return total;
            }
        }
        // The server ignored the range: its Content-Length, if any, is
        // the segment size, but the body must not be read here.
        if resp.status().is_success() {
            return resp.content_length();
        }
    }
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return None;
    }
    let resp = tokio::time::timeout(
        remaining,
        crate::http::request_method(client, url, headers, default_ua, reqwest::Method::HEAD).send(),
    )
    .await
    .ok()?
    .ok()?;
    if resp.status().is_success() {
        resp.content_length()
    } else {
        None
    }
}

/// Bitrate from sampled segments: total sampled bytes over the sampled
/// durations. At least one segment must answer with a size.
async fn sample_bps(
    client: &Client,
    urls: &[String],
    durations: &[f64],
    headers: &RequestHeaders,
    default_ua: &str,
    deadline: Instant,
) -> Option<u64> {
    let mut bytes = 0u64;
    let mut secs = 0.0f64;
    for (url, dur) in urls.iter().zip(durations.iter()).take(SAMPLE_SEGMENTS) {
        if *dur <= 0.0 {
            continue;
        }
        if let Some(size) = segment_size(client, url, headers, default_ua, deadline).await {
            bytes = bytes.saturating_add(size);
            secs += dur;
        }
    }
    if bytes > 0 && secs > 0.0 {
        Some(((bytes as f64 * 8.0) / secs).round() as u64)
    } else {
        None
    }
}

/// Probe an HLS or DASH link for capture-window metadata. A link that is
/// neither (or whose playlist cannot be read) is an error: the caller
/// falls back to the plain probe result.
pub async fn probe_stream(
    client: &Client,
    url: &str,
    headers: &RequestHeaders,
    default_ua: &str,
) -> anyhow::Result<StreamMeta> {
    let deadline = Instant::now() + PROBE_TIMEOUT;
    let text = fetch_text(client, url, headers, default_ua, deadline).await?;
    let trimmed = text.trim_start();
    if trimmed.starts_with("#EXTM3U") {
        probe_hls(client, url, headers, default_ua, &text, deadline).await
    } else if trimmed.starts_with('<') {
        probe_dash(client, url, headers, default_ua, &text, deadline).await
    } else {
        bail!("not a stream playlist or manifest")
    }
}

async fn probe_hls(
    client: &Client,
    url: &str,
    headers: &RequestHeaders,
    default_ua: &str,
    text: &str,
    deadline: Instant,
) -> anyhow::Result<StreamMeta> {
    // A media playlist on its own is one variant; a master lists several.
    let variants = m3u8::parse_master(url, text)?;
    let renditions = m3u8::parse_audio_renditions(url, text);
    let mut meta = StreamMeta {
        kind: DownloadKind::Hls,
        duration_secs: None,
        variants: Vec::new(),
        drm: None,
    };
    if variants.is_empty() {
        let media = m3u8::parse_media_meta(url, text)?;
        if let Some(drm) = media.drm {
            meta.drm = Some(format!(
                "this stream is DRM protected ({drm}) and cannot be downloaded"
            ));
            return Ok(meta);
        }
        let mut variant = VariantInfo {
            width: 0,
            height: 0,
            frame_rate: None,
            codecs: String::new(),
            video_bps: 0,
            audio_bps: 0,
            duration_secs: Some(media.duration_secs),
            estimated_size: None,
            size_source: SizeSource::Unknown,
            url: url.to_string(),
            is_audio_only: false,
        };
        if let Some(bps) = sample_bps(
            client,
            &media.segment_urls,
            &media.segment_durations,
            headers,
            default_ua,
            deadline,
        )
        .await
        {
            variant.video_bps = bps;
            variant.size_source = SizeSource::Sampled;
        }
        variant.estimate();
        meta.duration_secs = Some(media.duration_secs);
        meta.variants.push(variant);
        return Ok(meta);
    }

    let mut duration: Option<f64> = None;
    let mut drm: Option<String> = None;
    // Cap the variants fetched: each needs its own media playlist, and
    // the whole probe must finish inside the deadline.
    for v in variants.iter().take(8) {
        let media_text = match fetch_text(client, &v.url, headers, default_ua, deadline).await {
            Ok(t) => t,
            Err(_) => continue,
        };
        let Ok(media) = m3u8::parse_media_meta(&v.url, &media_text) else {
            continue;
        };
        if let Some(d) = media.drm {
            drm = Some(format!(
                "this stream is DRM protected ({d}) and cannot be downloaded"
            ));
            continue;
        }
        duration = Some(match duration {
            Some(d) => d.max(media.duration_secs),
            None => media.duration_secs,
        });
        let mut variant = VariantInfo {
            width: v.resolution.map(|(w, _)| w).unwrap_or(0),
            height: v.resolution.map(|(_, h)| h).unwrap_or(0),
            frame_rate: v.frame_rate,
            codecs: v.codecs.clone(),
            video_bps: v.bandwidth,
            audio_bps: 0,
            duration_secs: Some(media.duration_secs),
            estimated_size: None,
            size_source: if v.bandwidth > 0 {
                SizeSource::Bitrate
            } else {
                SizeSource::Unknown
            },
            url: v.url.clone(),
            is_audio_only: false,
        };
        // A separate audio group adds its own bandwidth: the audio
        // rendition's playlist is sampled when no tag gives it.
        if let Some(group) = &v.audio_group {
            if let Some(rend) = renditions
                .iter()
                .find(|r| &r.group_id == group)
                .and_then(|r| r.uri.clone())
            {
                if let Ok(audio_text) =
                    fetch_text(client, &rend, headers, default_ua, deadline).await
                {
                    if let Ok(audio) = m3u8::parse_media_meta(&rend, &audio_text) {
                        if let Some(bps) = sample_bps(
                            client,
                            &audio.segment_urls,
                            &audio.segment_durations,
                            headers,
                            default_ua,
                            deadline,
                        )
                        .await
                        {
                            variant.audio_bps = bps;
                        }
                    }
                }
            }
        }
        if variant.video_bps == 0 {
            if let Some(bps) = sample_bps(
                client,
                &media.segment_urls,
                &media.segment_durations,
                headers,
                default_ua,
                deadline,
            )
            .await
            {
                variant.video_bps = bps;
                variant.size_source = SizeSource::Sampled;
            }
        }
        variant.estimate();
        meta.variants.push(variant);
    }
    if let Some(d) = drm {
        meta.drm = Some(d);
        meta.variants.clear();
        meta.duration_secs = None;
        return Ok(meta);
    }
    if meta.variants.is_empty() {
        bail!("no usable variant in the playlist");
    }
    meta.duration_secs = duration;
    // Best quality first, like the download path picks it.
    meta.variants
        .sort_by_key(|v| std::cmp::Reverse((v.video_bps, (v.width as u64) * (v.height as u64))));
    Ok(meta)
}

async fn probe_dash(
    client: &Client,
    url: &str,
    headers: &RequestHeaders,
    default_ua: &str,
    text: &str,
    deadline: Instant,
) -> anyhow::Result<StreamMeta> {
    let dash = mpd::parse_metadata(text)?;
    let mut meta = StreamMeta {
        kind: DownloadKind::Dash,
        duration_secs: if dash.duration_secs > 0.0 {
            Some(dash.duration_secs)
        } else {
            None
        },
        variants: Vec::new(),
        drm: None,
    };
    if let Some(drm) = dash.drm {
        meta.drm = Some(format!(
            "this stream is DRM protected ({drm}) and cannot be downloaded"
        ));
        return Ok(meta);
    }
    // First-period segment URLs for sampling, from the download parser.
    // A manifest the parser cannot expand still reports its tags.
    let expanded = mpd::parse(url, text).ok();
    // The best audio bitrate pairs with every video quality; audio-only
    // streams get their own entries when there is no video at all.
    let best_audio_bps = dash.audios.iter().map(|a| a.bandwidth).max().unwrap_or(0);
    let mut variants: Vec<VariantInfo> = Vec::new();
    for rep in &dash.videos {
        let mut variant = VariantInfo {
            width: rep.width,
            height: rep.height,
            frame_rate: rep.frame_rate,
            codecs: rep.codecs.clone(),
            video_bps: rep.bandwidth,
            audio_bps: best_audio_bps,
            duration_secs: meta.duration_secs,
            estimated_size: None,
            size_source: if rep.bandwidth > 0 || best_audio_bps > 0 {
                SizeSource::Bitrate
            } else {
                SizeSource::Unknown
            },
            url: url.to_string(),
            is_audio_only: false,
        };
        if variant.video_bps == 0 {
            if let Some(track) = expanded.as_ref().and_then(|m| {
                m.videos
                    .iter()
                    .find(|t| t.id == rep.id && t.bandwidth == rep.bandwidth)
            }) {
                let seg_urls: Vec<String> = track.segments.iter().map(|s| s.url.clone()).collect();
                let seg_dur = meta.duration_secs.map(|d| {
                    let n = track.segments.len().max(1) as f64;
                    vec![d / n; seg_urls.len()]
                });
                if let Some(durs) = seg_dur {
                    if let Some(bps) =
                        sample_bps(client, &seg_urls, &durs, headers, default_ua, deadline).await
                    {
                        variant.video_bps = bps;
                        variant.size_source = SizeSource::Sampled;
                    }
                }
            }
        }
        variant.estimate();
        variants.push(variant);
    }
    if variants.is_empty() {
        for rep in &dash.audios {
            let mut variant = VariantInfo {
                width: 0,
                height: 0,
                frame_rate: None,
                codecs: rep.codecs.clone(),
                video_bps: 0,
                audio_bps: rep.bandwidth,
                duration_secs: meta.duration_secs,
                estimated_size: None,
                size_source: if rep.bandwidth > 0 {
                    SizeSource::Bitrate
                } else {
                    SizeSource::Unknown
                },
                url: url.to_string(),
                is_audio_only: true,
            };
            variant.estimate();
            variants.push(variant);
        }
    }
    variants
        .sort_by_key(|v| std::cmp::Reverse((v.video_bps, (v.width as u64) * (v.height as u64))));
    meta.variants = variants;
    Ok(meta)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_math() {
        let mut v = VariantInfo {
            width: 1920,
            height: 1080,
            frame_rate: Some(30.0),
            codecs: "avc1".into(),
            video_bps: 3_000_000,
            audio_bps: 128_000,
            duration_secs: Some(64.0),
            estimated_size: None,
            size_source: SizeSource::Bitrate,
            url: String::new(),
            is_audio_only: false,
        };
        v.estimate();
        assert_eq!(v.estimated_size, Some(25_024_000));
        v.duration_secs = None;
        v.estimate();
        assert_eq!(v.estimated_size, None);
    }
}
