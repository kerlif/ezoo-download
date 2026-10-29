//! Capture-window probing against a LOCAL test server only (127.0.0.1,
//! ephemeral port, std TcpListener): no external site is ever contacted.
//! Covers the Phase 4 gate fixtures (a)-(i).

use reqwest::Client;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::thread;
use swiftget_engine::http::probe;
use swiftget_engine::model::RequestHeaders;
use swiftget_engine::stream_meta::{probe_stream, SizeSource};

const MASTER: &str = "#EXTM3U\n#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"English\",URI=\"/audio.m3u8\"\n#EXT-X-STREAM-INF:BANDWIDTH=250000,AVERAGE-BANDWIDTH=200000,RESOLUTION=1280x720,FRAME-RATE=30,CODECS=\"avc1.64001f,mp4a.40.2\",AUDIO=\"aud\"\n/high.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=80000,RESOLUTION=640x360,FRAME-RATE=25,CODECS=\"avc1.42c01e\"\n/low.m3u8\n";
const HIGH: &str = "#EXTM3U\n#EXTINF:4.0,\n/seg-h1.ts\n#EXTINF:4.0,\n/seg-h2.ts\n#EXTINF:2.0,\n/seg-h3.ts\n#EXT-X-ENDLIST\n";
const LOW: &str = "#EXTM3U\n#EXTINF:5.0,\n/seg-l1.ts\n#EXTINF:5.0,\n/seg-l2.ts\n#EXT-X-ENDLIST\n";
const AUDIO: &str = "#EXTM3U\n#EXTINF:5.0,\n/seg-a1.ts\n#EXTINF:5.0,\n/seg-a2.ts\n#EXT-X-ENDLIST\n";
const MEDIA_ONLY: &str = "#EXTM3U\n#EXTINF:4.0,\n/seg-m1.ts\n#EXTINF:4.0,\n/seg-m2.ts\n#EXTINF:4.0,\n/seg-m3.ts\n#EXT-X-ENDLIST\n";
const DRM_HLS: &str = "#EXTM3U\n#EXT-X-KEY:METHOD=SAMPLE-AES,URI=\"skd://x\",KEYFORMAT=\"com.apple.streamingkeydelivery\"\n#EXTINF:4.0,\n/seg-h1.ts\n#EXT-X-ENDLIST\n";

fn dash_manifest(video_bw: &str, audio_bw: &str) -> String {
    let rep = |id: &str, bw: &str, extra: &str| {
        format!("<Representation id=\"{id}\" bandwidth=\"{bw}\" {extra}/>")
    };
    format!(
        "<MPD type=\"static\" mediaPresentationDuration=\"PT1M\">\
         <Period duration=\"PT30S\"><AdaptationSet contentType=\"video\">\
         <SegmentTemplate timescale=\"1000\" duration=\"4000\" initialization=\"init.mp4\" media=\"v-$RepresentationID$-$Number$.m4s\" startNumber=\"1\"/>\
         {}{}\
         </AdaptationSet><AdaptationSet contentType=\"audio\">\
         <SegmentTemplate timescale=\"1000\" duration=\"4000\" initialization=\"ainit.mp4\" media=\"a-$Number$.m4s\" startNumber=\"1\"/>\
         {}\
         </AdaptationSet></Period>\
         <Period duration=\"PT30S\"><AdaptationSet contentType=\"video\">\
         <SegmentTemplate timescale=\"1000\" duration=\"4000\" initialization=\"init.mp4\" media=\"v-$RepresentationID$-$Number$.m4s\" startNumber=\"1\"/>\
         {}{}\
         </AdaptationSet><AdaptationSet contentType=\"audio\">\
         <SegmentTemplate timescale=\"1000\" duration=\"4000\" initialization=\"ainit.mp4\" media=\"a-$Number$.m4s\" startNumber=\"1\"/>\
         {}\
         </AdaptationSet></Period></MPD>",
        rep("1080p", video_bw, "width=\"1920\" height=\"1080\" codecs=\"avc1.640028\" frameRate=\"30\""),
        rep("360p", video_bw, "width=\"640\" height=\"360\" codecs=\"avc1.42c01e\""),
        rep("a1", audio_bw, "codecs=\"mp4a.40.2\""),
        rep("1080p", video_bw, "width=\"1920\" height=\"1080\" codecs=\"avc1.640028\" frameRate=\"30\""),
        rep("360p", video_bw, "width=\"640\" height=\"360\" codecs=\"avc1.42c01e\""),
        rep("a1", audio_bw, "codecs=\"mp4a.40.2\""),
    )
}

const DRM_MPD: &str = "<MPD type=\"static\"><Period><AdaptationSet contentType=\"video\">\
<ContentProtection schemeIdUri=\"urn:uuid:edef8ba9-79d6-4ace-a3c8-27dcd51d21ed\"/>\
<SegmentTemplate timescale=\"1000\" duration=\"4000\" media=\"v-$Number$.m4s\"/>\
<Representation id=\"v\" bandwidth=\"1000\" width=\"640\" height=\"360\"/>\
</AdaptationSet></Period></MPD>";

fn seg_size(path: &str) -> Option<u64> {
    match path {
        p if p.starts_with("/seg-h") => Some(100_000),
        p if p.starts_with("/seg-a") => Some(20_000),
        p if p.starts_with("/seg-l") => Some(50_000),
        p if p.starts_with("/seg-m") => Some(40_000),
        p if p.starts_with("/v-") || p.starts_with("/a-") => Some(60_000),
        _ => None,
    }
}

fn handle(mut stream: std::net::TcpStream) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() || request_line.is_empty() {
        return;
    }
    let mut headers = String::new();
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {
                if line == "\r\n" || line == "\n" {
                    break;
                }
                headers.push_str(&line);
            }
        }
    }
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    let (method, path) = (
        parts.first().copied().unwrap_or(""),
        parts.get(1).copied().unwrap_or(""),
    );
    let lower = headers.to_ascii_lowercase();
    let has_cookie = lower.contains("cookie: sess=abc");
    let has_range = lower.contains("range: bytes=0-0");

    // (status, extra headers, body)
    let (status, extra, body): (&str, Vec<String>, Vec<u8>) = match (method, path) {
        ("HEAD", "/file.bin") | ("GET", "/file.bin") => {
            if !has_cookie {
                ("403 Forbidden", vec![], vec![])
            } else if has_range {
                (
                    "206 Partial Content",
                    vec!["Content-Range: bytes 0-0/12345".into()],
                    vec![b'x'],
                )
            } else {
                (
                    "200 OK",
                    vec![
                        "Content-Length: 12345".into(),
                        "Accept-Ranges: bytes".into(),
                        "Content-Type: application/octet-stream".into(),
                        "Content-Disposition: attachment; filename=\"report.pdf\"".into(),
                    ],
                    vec![b'x'; 12345],
                )
            }
        }
        ("HEAD", "/no-length.bin") => (
            "200 OK",
            vec!["Content-Type: application/octet-stream".into()],
            vec![],
        ),
        ("GET", "/no-length.bin") if has_range => (
            "206 Partial Content",
            vec!["Content-Range: bytes 0-0/777".into()],
            vec![b'x'],
        ),
        ("GET", "/no-length.bin") => (
            "200 OK",
            vec!["Content-Length: 777".into()],
            vec![b'x'; 777],
        ),
        ("HEAD", "/head403.bin") => ("403 Forbidden", vec![], vec![]),
        ("HEAD", "/head405.bin") => ("405 Method Not Allowed", vec![], vec![]),
        ("GET", "/head403.bin") if has_range => (
            "206 Partial Content",
            vec!["Content-Range: bytes 0-0/999".into()],
            vec![b'x'],
        ),
        ("GET", "/head405.bin") if has_range => (
            "206 Partial Content",
            vec!["Content-Range: bytes 0-0/888".into()],
            vec![b'x'],
        ),
        ("GET", "/head403.bin") | ("GET", "/head405.bin") => (
            "200 OK",
            vec!["Content-Length: 999".into()],
            vec![b'x'; 999],
        ),
        ("HEAD", "/false-length.bin") | ("HEAD", "/false-length-norange.bin") => {
            ("200 OK", vec!["Content-Length: 999999".into()], vec![])
        }
        ("GET", "/false-length.bin") if has_range => (
            "206 Partial Content",
            vec!["Content-Range: bytes 0-0/4321".into()],
            vec![b'x'],
        ),
        ("GET", "/false-length-norange.bin") => {
            // Ignores ranges entirely: 200 with the false length.
            (
                "200 OK",
                vec!["Content-Length: 999999".into()],
                vec![b'x'; 64],
            )
        }
        ("GET", "/false-length.bin") => (
            "200 OK",
            vec!["Content-Length: 999999".into()],
            vec![b'x'; 64],
        ),
        ("GET", "/master.m3u8") => {
            if !has_cookie {
                ("403 Forbidden", vec![], vec![])
            } else {
                (
                    "200 OK",
                    vec!["Content-Type: application/vnd.apple.mpegurl".into()],
                    MASTER.as_bytes().to_vec(),
                )
            }
        }
        ("GET", "/high.m3u8") => ("200 OK", vec![], HIGH.as_bytes().to_vec()),
        ("GET", "/low.m3u8") => ("200 OK", vec![], LOW.as_bytes().to_vec()),
        ("GET", "/audio.m3u8") => ("200 OK", vec![], AUDIO.as_bytes().to_vec()),
        ("GET", "/media-only.m3u8") => ("200 OK", vec![], MEDIA_ONLY.as_bytes().to_vec()),
        ("GET", "/drm.m3u8") => ("200 OK", vec![], DRM_HLS.as_bytes().to_vec()),
        ("GET", "/garbage.m3u8") => ("200 OK", vec![], b"hello, this is not a playlist".to_vec()),
        ("GET", "/multi.mpd") => (
            "200 OK",
            vec!["Content-Type: application/dash+xml".into()],
            dash_manifest("3000000", "128000").as_bytes().to_vec(),
        ),
        ("GET", "/dash-nosize.mpd") => {
            // Same shape, but the video/audio reps carry no bandwidth tag
            // for the 1080p/audio reps: sampling must fill the gap. The
            // manifest builder uses one bandwidth for all reps, so build
            // a dedicated document instead.
            let doc = "<MPD type=\"static\"><Period duration=\"PT30S\">\
                <AdaptationSet contentType=\"video\">\
                <SegmentTemplate timescale=\"1000\" duration=\"4000\" initialization=\"init.mp4\" media=\"v-$RepresentationID$-$Number$.m4s\" startNumber=\"1\"/>\
                <Representation id=\"v1\" width=\"1280\" height=\"720\" codecs=\"avc1.64001f\"/>\
                </AdaptationSet><AdaptationSet contentType=\"audio\">\
                <SegmentTemplate timescale=\"1000\" duration=\"4000\" initialization=\"ainit.mp4\" media=\"a-$Number$.m4s\" startNumber=\"1\"/>\
                <Representation id=\"a1\" codecs=\"mp4a.40.2\"/>\
                </AdaptationSet></Period>\
                <Period duration=\"PT30S\">\
                <AdaptationSet contentType=\"video\">\
                <SegmentTemplate timescale=\"1000\" duration=\"4000\" initialization=\"init.mp4\" media=\"v-$RepresentationID$-$Number$.m4s\" startNumber=\"1\"/>\
                <Representation id=\"v1\" width=\"1280\" height=\"720\" codecs=\"avc1.64001f\"/>\
                </AdaptationSet></Period></MPD>";
            (
                "200 OK",
                vec!["Content-Type: application/dash+xml".into()],
                doc.as_bytes().to_vec(),
            )
        }
        ("GET", "/drm.mpd") => (
            "200 OK",
            vec!["Content-Type: application/dash+xml".into()],
            DRM_MPD.as_bytes().to_vec(),
        ),
        ("GET", "/truncated.mpd") => ("200 OK", vec![], b"<MPD><Period>".to_vec()),
        _ if method == "GET" => {
            if let Some(size) = seg_size(path) {
                if has_range {
                    (
                        "206 Partial Content",
                        vec![format!("Content-Range: bytes 0-0/{size}")],
                        vec![b'x'],
                    )
                } else {
                    (
                        "200 OK",
                        vec![format!("Content-Length: {size}")],
                        vec![b'x'; size as usize],
                    )
                }
            } else {
                ("404 Not Found", vec![], vec![])
            }
        }
        _ => ("404 Not Found", vec![], vec![]),
    };
    let is_head = method == "HEAD";
    let mut head = format!("HTTP/1.1 {status}\r\nConnection: close\r\n");
    for h in &extra {
        head.push_str(h);
        head.push_str("\r\n");
    }
    if !extra
        .iter()
        .any(|h| h.to_ascii_lowercase().starts_with("content-length"))
        && !is_head
    {
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    if !is_head {
        let _ = stream.write_all(&body);
    }
    let _ = stream.flush();
}

fn serve() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            thread::spawn(move || handle(conn));
        }
    });
    format!("http://{addr}")
}

fn client() -> Client {
    Client::builder().no_proxy().build().unwrap()
}

fn captured_headers() -> RequestHeaders {
    RequestHeaders {
        referer: Some("https://example.test/page".into()),
        cookies: Some("sess=abc".into()),
        user_agent: Some("TestUA/1".into()),
        ..Default::default()
    }
}

#[tokio::test]
async fn direct_file_with_content_length() {
    let base = serve();
    let p = probe(
        &client(),
        &format!("{base}/file.bin"),
        &captured_headers(),
        "UA",
    )
    .await
    .unwrap();
    assert_eq!(p.total_size, Some(12345));
    assert!(!p.total_size_unverified, "206-confirmed size is verified");
    assert!(p.resumable);
    assert_eq!(p.file_name, "report.pdf");
}

#[tokio::test]
async fn direct_file_without_content_length_uses_ranged_get() {
    let base = serve();
    let p = probe(
        &client(),
        &format!("{base}/no-length.bin"),
        &RequestHeaders::default(),
        "UA",
    )
    .await
    .unwrap();
    assert_eq!(p.total_size, Some(777));
    assert!(!p.total_size_unverified);
}

#[tokio::test]
async fn head_rejected_falls_back_to_ranged_get() {
    let base = serve();
    let c = client();
    let p403 = probe(
        &c,
        &format!("{base}/head403.bin"),
        &RequestHeaders::default(),
        "UA",
    )
    .await
    .unwrap();
    assert_eq!(p403.total_size, Some(999), "HEAD 403 -> ranged GET");
    let p405 = probe(
        &c,
        &format!("{base}/head405.bin"),
        &RequestHeaders::default(),
        "UA",
    )
    .await
    .unwrap();
    assert_eq!(p405.total_size, Some(888), "HEAD 405 -> ranged GET");
}

#[tokio::test]
async fn false_content_length_corrected_by_ranged_check() {
    let base = serve();
    let p = probe(
        &client(),
        &format!("{base}/false-length.bin"),
        &RequestHeaders::default(),
        "UA",
    )
    .await
    .unwrap();
    // Defined behavior: the Content-Range total wins over a lying HEAD.
    assert_eq!(p.total_size, Some(4321));
    assert!(!p.total_size_unverified);
}

#[tokio::test]
async fn false_content_length_without_range_support_flagged_unverified() {
    let base = serve();
    let p = probe(
        &client(),
        &format!("{base}/false-length-norange.bin"),
        &RequestHeaders::default(),
        "UA",
    )
    .await
    .unwrap();
    // Defined behavior: no ranged confirmation exists, so the HEAD
    // length is kept but flagged untrusted (UI shows it as an estimate).
    assert_eq!(p.total_size, Some(999999));
    assert!(p.total_size_unverified);
    assert!(!p.resumable);
}

#[tokio::test]
async fn hls_master_and_media_metadata() {
    let base = serve();
    let meta = probe_stream(
        &client(),
        &format!("{base}/master.m3u8"),
        &captured_headers(),
        "UA",
    )
    .await
    .unwrap();
    assert!(meta.drm.is_none());
    assert_eq!(meta.duration_secs, Some(10.0));
    assert_eq!(meta.variants.len(), 2);
    let high = &meta.variants[0];
    assert_eq!((high.width, high.height), (1280, 720));
    assert_eq!(high.frame_rate, Some(30.0));
    assert_eq!(high.video_bps, 200_000, "AVERAGE-BANDWIDTH preferred");
    assert_eq!(
        high.audio_bps, 32_000,
        "separate audio group sampled and added"
    );
    assert_eq!(high.size_source, SizeSource::Bitrate);
    assert_eq!(high.estimated_size, Some(290_000));
    let low = &meta.variants[1];
    assert_eq!(low.video_bps, 80_000, "BANDWIDTH fallback");
    assert_eq!(low.audio_bps, 0);
    assert_eq!(low.estimated_size, Some(100_000));
}

#[tokio::test]
async fn hls_media_only_samples_segments() {
    let base = serve();
    let meta = probe_stream(
        &client(),
        &format!("{base}/media-only.m3u8"),
        &RequestHeaders::default(),
        "UA",
    )
    .await
    .unwrap();
    assert_eq!(meta.variants.len(), 1);
    let v = &meta.variants[0];
    assert_eq!(meta.duration_secs, Some(12.0));
    assert_eq!(v.video_bps, 80_000, "3 x 40000-byte segments over 12 s");
    assert_eq!(v.size_source, SizeSource::Sampled);
    assert_eq!(v.estimated_size, Some(120_000));
}

#[tokio::test]
async fn hls_captured_headers_are_used() {
    let base = serve();
    // The master playlist requires the captured cookie: without it the
    // probe fails, with it the metadata loads.
    let err = probe_stream(
        &client(),
        &format!("{base}/master.m3u8"),
        &RequestHeaders::default(),
        "UA",
    )
    .await;
    assert!(err.is_err(), "no cookie: server refuses the playlist");
    let ok = probe_stream(
        &client(),
        &format!("{base}/master.m3u8"),
        &captured_headers(),
        "UA",
    )
    .await;
    assert!(ok.is_ok());
}

#[tokio::test]
async fn dash_multi_period_metadata() {
    let base = serve();
    let meta = probe_stream(
        &client(),
        &format!("{base}/multi.mpd"),
        &RequestHeaders::default(),
        "UA",
    )
    .await
    .unwrap();
    assert!(meta.drm.is_none());
    assert_eq!(meta.duration_secs, Some(60.0), "two 30 s Periods summed");
    assert_eq!(
        meta.variants.len(),
        2,
        "video reps only; audio kept separate"
    );
    let v1080 = &meta.variants[0];
    assert_eq!((v1080.width, v1080.height), (1920, 1080));
    assert_eq!(v1080.frame_rate, Some(30.0));
    assert_eq!(v1080.video_bps, 3_000_000);
    assert_eq!(
        v1080.audio_bps, 128_000,
        "audio bitrate kept separate from video"
    );
    assert_eq!(v1080.estimated_size, Some(23_460_000));
    assert_eq!(meta.variants[1].audio_bps, 128_000);
}

#[tokio::test]
async fn dash_missing_bandwidth_samples_segments() {
    let base = serve();
    let meta = probe_stream(
        &client(),
        &format!("{base}/dash-nosize.mpd"),
        &RequestHeaders::default(),
        "UA",
    )
    .await
    .unwrap();
    assert_eq!(meta.duration_secs, Some(60.0));
    assert_eq!(meta.variants.len(), 1);
    let v = &meta.variants[0];
    assert_eq!(v.video_bps, 64_000, "60000-byte segments over 7.5 s each");
    assert_eq!(v.size_source, SizeSource::Sampled);
    assert_eq!(v.estimated_size, Some(480_000));
}

#[tokio::test]
async fn malformed_playlist_and_manifest_rejected() {
    let base = serve();
    let c = client();
    assert!(probe_stream(
        &c,
        &format!("{base}/garbage.m3u8"),
        &RequestHeaders::default(),
        "UA"
    )
    .await
    .is_err());
    assert!(probe_stream(
        &c,
        &format!("{base}/truncated.mpd"),
        &RequestHeaders::default(),
        "UA"
    )
    .await
    .is_err());
}

#[tokio::test]
async fn drm_streams_refused_with_message() {
    let base = serve();
    let c = client();
    let dash = probe_stream(
        &c,
        &format!("{base}/drm.mpd"),
        &RequestHeaders::default(),
        "UA",
    )
    .await
    .unwrap();
    assert!(dash.drm.is_some(), "DASH Widevine marker must be reported");
    assert!(dash.variants.is_empty(), "no variants offered for DRM");
    assert!(dash.drm.as_ref().unwrap().contains("DRM"));
    let hls = probe_stream(
        &c,
        &format!("{base}/drm.m3u8"),
        &RequestHeaders::default(),
        "UA",
    )
    .await
    .unwrap();
    assert!(hls.drm.is_some(), "HLS FairPlay KEYFORMAT must be reported");
    assert!(hls.variants.is_empty());
}
