//! Parsing of HLS playlists (.m3u8).

use anyhow::{anyhow, bail};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Key {
    pub uri: String,
    pub iv: Option<[u8; 16]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub url: String,
    pub sequence: u64,
    pub key: Option<Key>,
}

#[derive(Debug, Default, PartialEq)]
pub struct MediaPlaylist {
    pub init_url: Option<String>,
    pub parts: Vec<Part>,
    pub ended: bool,
}

/// Parse `KEY=VALUE,KEY="quoted,value"` attribute lists.
pub fn parse_attrs(s: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut key = String::new();
    let mut val = String::new();
    let mut in_key = true;
    let mut quoted = false;
    for c in s.chars() {
        if in_key {
            if c == '=' {
                in_key = false;
            } else if c != ',' {
                key.push(c);
            }
        } else if c == '"' {
            quoted = !quoted;
        } else if c == ',' && !quoted {
            out.insert(key.trim().to_ascii_uppercase(), val.clone());
            key.clear();
            val.clear();
            in_key = true;
        } else {
            val.push(c);
        }
    }
    if !key.trim().is_empty() {
        out.insert(key.trim().to_ascii_uppercase(), val);
    }
    out
}

fn resolve(base: &str, rel: &str) -> anyhow::Result<String> {
    let base = url::Url::parse(base)?;
    Ok(base.join(rel.trim())?.to_string())
}

/// For a master playlist, return the URL of the highest quality variant.
pub fn best_variant(base: &str, text: &str) -> anyhow::Result<Option<String>> {
    let mut best: Option<(u64, String)> = None;
    let mut pending_bw: Option<u64> = None;
    for line in text.lines().map(str::trim) {
        if let Some(attrs) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            let a = parse_attrs(attrs);
            pending_bw = Some(a.get("BANDWIDTH").and_then(|b| b.parse().ok()).unwrap_or(0));
        } else if !line.is_empty() && !line.starts_with('#') {
            if let Some(bw) = pending_bw.take() {
                if best.as_ref().map(|(b, _)| bw > *b).unwrap_or(true) {
                    best = Some((bw, resolve(base, line)?));
                }
            }
        }
    }
    Ok(best.map(|(_, u)| u))
}

/// One quality of a master playlist.
#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub url: String,
    /// AVERAGE-BANDWIDTH when present, else BANDWIDTH (0 = not tagged).
    pub bandwidth: u64,
    pub average_bandwidth: Option<u64>,
    pub resolution: Option<(u32, u32)>,
    pub frame_rate: Option<f32>,
    pub codecs: String,
    /// EXT-X-MEDIA audio group this variant refers to, if any.
    pub audio_group: Option<String>,
}

/// An audio rendition of a master playlist (EXT-X-MEDIA TYPE=AUDIO).
#[derive(Debug, Clone, PartialEq)]
pub struct AudioRendition {
    pub group_id: String,
    pub uri: Option<String>,
    pub name: String,
}

fn parse_resolution(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.split_once('x')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

/// Parse a master playlist into its variants (resolution, frame rate,
/// codecs, bandwidth). A media playlist yields no variants.
pub fn parse_master(base: &str, text: &str) -> anyhow::Result<Vec<Variant>> {
    let mut out = Vec::new();
    let mut pending: Option<Variant> = None;
    for line in text.lines().map(str::trim) {
        if let Some(attrs) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            let a = parse_attrs(attrs);
            let bw = a.get("BANDWIDTH").and_then(|b| b.parse().ok()).unwrap_or(0);
            let avg = a.get("AVERAGE-BANDWIDTH").and_then(|b| b.parse().ok());
            pending = Some(Variant {
                url: String::new(),
                bandwidth: avg.unwrap_or(bw),
                average_bandwidth: avg,
                resolution: a.get("RESOLUTION").and_then(|r| parse_resolution(r)),
                frame_rate: a.get("FRAME-RATE").and_then(|f| f.parse().ok()),
                codecs: a.get("CODECS").cloned().unwrap_or_default(),
                audio_group: a.get("AUDIO").cloned(),
            });
        } else if !line.is_empty() && !line.starts_with('#') {
            if let Some(mut v) = pending.take() {
                v.url = resolve(base, line)?;
                out.push(v);
            }
        }
    }
    Ok(out)
}

/// Audio renditions of a master playlist.
pub fn parse_audio_renditions(base: &str, text: &str) -> Vec<AudioRendition> {
    let mut out = Vec::new();
    for line in text.lines().map(str::trim) {
        let Some(attrs) = line.strip_prefix("#EXT-X-MEDIA:") else {
            continue;
        };
        let a = parse_attrs(attrs);
        if a.get("TYPE").map(|t| t.to_ascii_uppercase()).as_deref() != Some("AUDIO") {
            continue;
        }
        let uri = a.get("URI").and_then(|u| resolve(base, u).ok());
        out.push(AudioRendition {
            group_id: a.get("GROUP-ID").cloned().unwrap_or_default(),
            uri,
            name: a.get("NAME").cloned().unwrap_or_default(),
        });
    }
    out
}

/// Whether a KEYFORMAT value signals DRM (FairPlay / Widevine / PlayReady).
/// AES-128 uses no KEYFORMAT or "identity" and stays supported: its key
/// is delivered in the playlist, so using it is not circumvention.
pub fn is_drm_keyformat(format: &str) -> bool {
    let f = format.to_ascii_lowercase();
    f.contains("fairplay")
        || f.contains("playready")
        || f.contains("widevine")
        || f.contains("streamingkeydelivery")
        || f.contains("edef8ba9")
        || f.contains("9a04f079")
        || f.contains("94ce86fb")
        || f.contains("1077efec")
}

/// Why a media playlist is DRM-protected, if it is: the EXT-X-KEY method
/// or KEYFORMAT. `None` means clear or AES-128 with a playlist key.
pub fn drm_reason(text: &str) -> Option<String> {
    for line in text.lines().map(str::trim) {
        let Some(attrs) = line.strip_prefix("#EXT-X-KEY:") else {
            continue;
        };
        let a = parse_attrs(attrs);
        let method = a
            .get("METHOD")
            .map(|m| m.to_ascii_uppercase())
            .unwrap_or_default();
        if let Some(kf) = a.get("KEYFORMAT") {
            if is_drm_keyformat(kf) {
                return Some(format!("DRM key format {kf}"));
            }
        }
        match method.as_str() {
            "NONE" | "" | "AES-128" => {}
            other => return Some(format!("DRM key method {other}")),
        }
    }
    None
}

/// Metadata of a media playlist: total duration, segment URLs with their
/// EXTINF durations, and any DRM signal. Unlike [`parse_media`] this does
/// not fail on DRM: the caller reports it instead of downloading.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaMeta {
    pub duration_secs: f64,
    pub segment_urls: Vec<String>,
    pub segment_durations: Vec<f64>,
    pub drm: Option<String>,
}

pub fn parse_media_meta(base: &str, text: &str) -> anyhow::Result<MediaMeta> {
    if text.len() > MAX_PLAYLIST_BYTES {
        bail!("the stream's list is too big");
    }
    let mut meta = MediaMeta {
        drm: drm_reason(text),
        ..Default::default()
    };
    let mut pending: Option<f64> = None;
    for line in text.lines().map(str::trim) {
        if line.is_empty() {
            continue;
        }
        if let Some(v) = line.strip_prefix("#EXTINF:") {
            let num: String = v
                .split(',')
                .next()
                .unwrap_or("")
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            pending = num.parse().ok();
        } else if !line.starts_with('#') {
            if meta.segment_urls.len() >= MAX_PARTS {
                bail!("the stream has too many parts");
            }
            let dur = pending.take().unwrap_or(0.0);
            meta.duration_secs += dur;
            meta.segment_urls.push(resolve(base, line)?);
            meta.segment_durations.push(dur);
        }
    }
    Ok(meta)
}

fn parse_iv(s: &str) -> Option<[u8; 16]> {
    let hex = s.trim().trim_start_matches("0x").trim_start_matches("0X");
    if hex.len() != 32 || !hex.is_ascii() {
        return None;
    }
    let mut out = [0u8; 16];
    for i in 0..16 {
        out[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

/// A real stream has a few thousand parts; a list far beyond this is an
/// attack on memory and disk, not a video.
pub const MAX_PARTS: usize = 200_000;
pub const MAX_PLAYLIST_BYTES: usize = 32 << 20;

pub fn parse_media(base: &str, text: &str) -> anyhow::Result<MediaPlaylist> {
    if text.len() > MAX_PLAYLIST_BYTES {
        bail!("the stream's list is too big");
    }
    let mut pl = MediaPlaylist::default();
    let mut seq: u64 = 0;
    let mut key: Option<Key> = None;
    for line in text.lines().map(str::trim) {
        if line.is_empty() {
            continue;
        }
        if let Some(v) = line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:") {
            seq = v.trim().parse().unwrap_or(0);
        } else if let Some(attrs) = line.strip_prefix("#EXT-X-KEY:") {
            let a = parse_attrs(attrs);
            let method = a
                .get("METHOD")
                .map(|s| s.to_ascii_uppercase())
                .unwrap_or_default();
            key = match method.as_str() {
                "NONE" | "" => None,
                "AES-128" => {
                    let uri = a
                        .get("URI")
                        .ok_or_else(|| anyhow!("AES-128 key without URI"))?;
                    Some(Key {
                        uri: resolve(base, uri)?,
                        iv: a.get("IV").and_then(|v| parse_iv(v)),
                    })
                }
                other => bail!("this stream is DRM protected ({other}) and cannot be downloaded"),
            };
        } else if let Some(attrs) = line.strip_prefix("#EXT-X-MAP:") {
            let a = parse_attrs(attrs);
            if let Some(uri) = a.get("URI") {
                pl.init_url = Some(resolve(base, uri)?);
            }
        } else if line.starts_with("#EXT-X-ENDLIST") {
            pl.ended = true;
        } else if !line.starts_with('#') {
            if pl.parts.len() >= MAX_PARTS {
                bail!("the stream has too many parts");
            }
            pl.parts.push(Part {
                url: resolve(base, line)?,
                sequence: seq,
                key: key.clone(),
            });
            seq = seq.wrapping_add(1);
        }
    }
    Ok(pl)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_lists_are_refused() {
        let mut huge = String::from("#EXTM3U\n");
        for i in 0..=MAX_PARTS {
            huge.push_str(&format!("p{i}.ts\n"));
        }
        assert!(parse_media("https://x.example/a/list.m3u8", &huge).is_err());
        assert!(parse_media(
            "https://x.example/a/list.m3u8",
            &"#".repeat(MAX_PLAYLIST_BYTES + 1)
        )
        .is_err());
        assert!(parse_media("https://x.example/a/list.m3u8", "#EXTM3U\np1.ts\n").is_ok());
    }

    #[test]
    fn attrs_with_quotes() {
        let a = parse_attrs(
            "METHOD=AES-128,URI=\"https://k/key?a=1,2\",IV=0x00000000000000000000000000000001",
        );
        assert_eq!(a["METHOD"], "AES-128");
        assert_eq!(a["URI"], "https://k/key?a=1,2");
        assert_eq!(parse_iv(&a["IV"]).unwrap()[15], 1);
    }

    #[test]
    fn master_picks_highest_bandwidth() {
        let m = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360\nlow/index.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=5000000,RESOLUTION=1920x1080\nhigh/index.m3u8\n";
        let u = best_variant("https://cdn.x/v/master.m3u8", m)
            .unwrap()
            .unwrap();
        assert_eq!(u, "https://cdn.x/v/high/index.m3u8");
    }

    #[test]
    fn media_playlist() {
        let m = "#EXTM3U\n#EXT-X-MEDIA-SEQUENCE:7\n#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\"\n#EXTINF:4,\na.ts\n#EXTINF:4,\nb.ts\n#EXT-X-ENDLIST\n";
        let p = parse_media("https://cdn.x/v/index.m3u8", m).unwrap();
        assert!(p.ended);
        assert_eq!(p.parts.len(), 2);
        assert_eq!(p.parts[1].url, "https://cdn.x/v/b.ts");
        assert_eq!(p.parts[1].sequence, 8);
        assert_eq!(
            p.parts[0].key.as_ref().unwrap().uri,
            "https://cdn.x/v/key.bin"
        );
    }

    #[test]
    fn master_variants_and_audio_group() {
        let m = "#EXTM3U\n#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"English\",URI=\"audio.m3u8\"\n#EXT-X-STREAM-INF:BANDWIDTH=5000000,AVERAGE-BANDWIDTH=4200000,RESOLUTION=1920x1080,FRAME-RATE=30,CODECS=\"avc1.640028,mp4a.40.2\",AUDIO=\"aud\"\nhigh/index.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360\nlow/index.m3u8\n";
        let vs = parse_master("https://cdn.x/v/master.m3u8", m).unwrap();
        assert_eq!(vs.len(), 2);
        assert_eq!(vs[0].bandwidth, 4200000, "AVERAGE-BANDWIDTH wins");
        assert_eq!(vs[0].average_bandwidth, Some(4200000));
        assert_eq!(vs[0].resolution, Some((1920, 1080)));
        assert_eq!(vs[0].frame_rate, Some(30.0));
        assert_eq!(vs[0].audio_group.as_deref(), Some("aud"));
        assert_eq!(vs[1].bandwidth, 800000, "BANDWIDTH fallback");
        assert_eq!(vs[0].url, "https://cdn.x/v/high/index.m3u8");
        let rend = parse_audio_renditions("https://cdn.x/v/master.m3u8", m);
        assert_eq!(rend.len(), 1);
        assert_eq!(rend[0].uri.as_deref(), Some("https://cdn.x/v/audio.m3u8"));
    }

    #[test]
    fn media_meta_sums_extinf() {
        let m =
            "#EXTM3U\n#EXTINF:4.0,\na.ts\n#EXTINF:6.5,\nb.ts\n#EXTINF:2,\nc.ts\n#EXT-X-ENDLIST\n";
        let meta = parse_media_meta("https://cdn.x/v/index.m3u8", m).unwrap();
        assert_eq!(meta.duration_secs, 12.5);
        assert_eq!(meta.segment_urls.len(), 3);
        assert_eq!(meta.segment_durations, vec![4.0, 6.5, 2.0]);
        assert!(meta.drm.is_none());
    }

    #[test]
    fn media_meta_reports_drm_keyformats() {
        let fairplay = "#EXTM3U\n#EXT-X-KEY:METHOD=SAMPLE-AES,URI=\"skd://x\",KEYFORMAT=\"com.apple.streamingkeydelivery\"\na.ts\n";
        let meta = parse_media_meta("https://cdn.x/index.m3u8", fairplay).unwrap();
        assert!(meta.drm.is_some(), "FairPlay KEYFORMAT must be reported");
        let widevine = "#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"https://k/key\",KEYFORMAT=\"urn:uuid:edef8ba9-79d6-4ace-a3c8-27dcd51d21ed\"\na.ts\n";
        let meta = parse_media_meta("https://cdn.x/index.m3u8", widevine).unwrap();
        assert!(meta.drm.is_some(), "Widevine KEYFORMAT must be reported");
        let aes = "#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\"\n#EXTINF:4,\na.ts\n";
        let meta = parse_media_meta("https://cdn.x/index.m3u8", aes).unwrap();
        assert!(meta.drm.is_none(), "AES-128 stays supported");
    }

    #[test]
    fn drm_is_refused() {
        let m = "#EXTM3U\n#EXT-X-KEY:METHOD=SAMPLE-AES,URI=\"skd://x\"\na.ts\n#EXT-X-ENDLIST\n";
        assert!(parse_media("https://cdn.x/index.m3u8", m).is_err());
    }
}
