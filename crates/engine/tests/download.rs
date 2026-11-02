//! End-to-end: a local HTTP server with Range support, a slow region to force
//! dynamic splitting, then pause + resume. The result must be byte-identical.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use swiftget_engine::http::{build_client, probe, Progress, Shared};
use swiftget_engine::http_download::{self, HttpJob};
use swiftget_engine::limiter::SpeedLimiter;
use swiftget_engine::RequestHeaders;
use tokio_util::sync::CancellationToken;

mod testserver;

fn test_data(len: usize) -> Arc<Vec<u8>> {
    Arc::new((0..len).map(|i| (i * 31 % 251) as u8).collect())
}

async fn serve(data: Arc<Vec<u8>>) -> u16 {
    let (port, _) = testserver::spawn({
        let data = data.clone();
        move |req: &testserver::Request| {
            let range = req
                .range()
                .map(|(a, b)| (a, b.unwrap_or(data.len() - 1).min(data.len() - 1)));
            let (a, b) = range.unwrap_or((0, data.len() - 1));
            let resp = if range.is_some() {
                testserver::StreamResponse::new(206, data[a..=b].to_vec())
                    .header("Content-Range", &format!("bytes {a}-{b}/{}", data.len()))
                    .header(
                        "Content-Disposition",
                        "attachment; filename=\"test file.bin\"",
                    )
            } else {
                testserver::StreamResponse::new(200, data.clone().to_vec())
            };
            // the first part of the file is slow -> other connections split it
            let delay = if a < data.len() / 8 {
                Duration::from_millis(5)
            } else {
                Duration::from_millis(1)
            };
            testserver::Action::Stream(resp.chunk_delay(delay))
        }
    });
    port
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn segmented_download_with_pause_and_resume() {
    let data = test_data(4 * 1024 * 1024 + 123);
    let port = serve(data.clone()).await;
    let url = format!("http://127.0.0.1:{port}/files/test.bin");
    let shared = Arc::new(Shared::new(build_client().unwrap(), SpeedLimiter::new(0)));

    let p = probe(&shared.client(), &url, &RequestHeaders::default(), "test")
        .await
        .unwrap();
    assert_eq!(p.total_size, Some(data.len() as u64));
    assert!(p.resumable);
    assert_eq!(p.file_name, "test file.bin");

    let dir = std::env::temp_dir().join(format!("swiftget-test-{port}"));
    std::fs::create_dir_all(&dir).unwrap();
    let part = dir.join("out.sgpart");
    let _ = std::fs::remove_file(&part);
    let job = HttpJob {
        url: url.clone(),
        headers: RequestHeaders::default(),
        user_agent: "test".into(),
        total_size: p.total_size,
        resumable: true,
        part_path: part.clone(),
        connections: 8,
        min_split: 64 * 1024,
        retries: 3,
        progress_base: 0,
        max_request: None,
        limiter: Arc::new(SpeedLimiter::new(0)),
        adaptive: Default::default(),
        learned_start: None,
        report: None,
    };
    let segs = Arc::new(Mutex::new(Vec::new()));
    let progress = Arc::new(Progress::default());

    // start, then pause once about a quarter is there
    let cancel = CancellationToken::new();
    let c2 = cancel.clone();
    let p2 = progress.clone();
    let quarter = data.len() as u64 / 4;
    tokio::spawn(async move {
        while p2.bytes.load(Ordering::Relaxed) < quarter {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        c2.cancel();
    });
    let first = http_download::run(
        shared.clone(),
        job.clone(),
        segs.clone(),
        progress.clone(),
        cancel,
    )
    .await;
    assert!(first.is_err(), "should have been paused");
    let after_pause = progress.bytes.load(Ordering::Relaxed);
    assert!(after_pause > 0 && after_pause < data.len() as u64);

    // resume with the saved segment table
    let saved = segs.lock().unwrap().clone();
    let segs = Arc::new(Mutex::new(saved));
    http_download::run(
        shared,
        job,
        segs.clone(),
        progress.clone(),
        CancellationToken::new(),
    )
    .await
    .unwrap();

    let out = std::fs::read(&part).unwrap();
    assert_eq!(out.len(), data.len());
    assert!(out == *data, "downloaded bytes differ from the source");
    // how far the file gets split depends on how fast the machine is
    assert!(!segs.lock().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// YouTube style: each connection may only ask for a small piece per request.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chunked_requests_with_32_connections() {
    let data = test_data(3 * 1024 * 1024 + 7);
    let port = serve(data.clone()).await;
    let url = format!("http://127.0.0.1:{port}/v.bin");
    let shared = Arc::new(Shared::new(build_client().unwrap(), SpeedLimiter::new(0)));
    let dir = std::env::temp_dir().join(format!("swiftget-chunk-{port}"));
    std::fs::create_dir_all(&dir).unwrap();
    let part = dir.join("v.sgpart");
    let job = HttpJob {
        url,
        headers: RequestHeaders::default(),
        user_agent: "test".into(),
        total_size: Some(data.len() as u64),
        resumable: true,
        part_path: part.clone(),
        connections: 32,
        min_split: 32 * 1024,
        retries: 3,
        progress_base: 0,
        max_request: Some(40 * 1024),
        limiter: Arc::new(SpeedLimiter::new(0)),
        adaptive: Default::default(),
        learned_start: None,
        report: None,
    };
    let segs = Arc::new(Mutex::new(Vec::new()));
    let progress = Arc::new(Progress::default());
    http_download::run(
        shared,
        job,
        segs,
        progress.clone(),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    let out = std::fs::read(&part).unwrap();
    assert!(out == *data, "chunked download differs from the source");
    assert_eq!(progress.bytes.load(Ordering::Relaxed), data.len() as u64);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Like `serve`, but while `down` is set every connection is dropped at once,
/// and the first answers break off half way: a flaky internet line.
async fn serve_flaky(data: Arc<Vec<u8>>, down: Arc<std::sync::atomic::AtomicBool>) -> u16 {
    let served = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let (port, _) = testserver::spawn({
        let data = data.clone();
        let served = served.clone();
        let down = down.clone();
        move |req: &testserver::Request| {
            // while `down` is set every connection is dropped at once,
            // before any answer (the old server dropped at accept time)
            if down.load(Ordering::Relaxed) {
                return testserver::Action::Drop;
            }
            let n = served.fetch_add(1, Ordering::Relaxed);
            let range = req
                .range()
                .map(|(a, b)| (a, b.unwrap_or(data.len() - 1).min(data.len() - 1)));
            let (a, b) = range.unwrap_or((0, data.len() - 1));
            let body = &data[a..=b];
            // the first requests lose the line after half of their part
            let cut = if n < 4 { body.len() / 2 } else { body.len() };
            testserver::Action::Stream(
                testserver::StreamResponse::new(206, body[..cut].to_vec())
                    .header("Content-Range", &format!("bytes {a}-{b}/{}", data.len()))
                    .content_length(b - a + 1)
                    .chunk_delay(Duration::from_millis(1)),
            )
        }
    });
    port
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn waits_through_a_long_outage_and_resumes() {
    let data = test_data(2 * 1024 * 1024 + 99);
    let down = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port = serve_flaky(data.clone(), down.clone()).await;
    let url = format!("http://127.0.0.1:{port}/f.bin");
    let shared = Arc::new(Shared::new(build_client().unwrap(), SpeedLimiter::new(0)));
    let dir = std::env::temp_dir().join(format!("swiftget-flaky-{port}"));
    std::fs::create_dir_all(&dir).unwrap();
    let part = dir.join("out.sgpart");
    let job = HttpJob {
        url,
        headers: RequestHeaders::default(),
        user_agent: "test".into(),
        total_size: Some(data.len() as u64),
        resumable: true,
        part_path: part.clone(),
        connections: 4,
        min_split: 64 * 1024,
        // far fewer than the failures below: a dropped line must not use them up
        retries: 1,
        progress_base: 0,
        max_request: None,
        limiter: Arc::new(SpeedLimiter::new(0)),
        adaptive: Default::default(),
        learned_start: None,
        report: None,
    };
    let segs = Arc::new(Mutex::new(Vec::new()));
    let progress = Arc::new(Progress::default());

    // the line goes down once a bit is there, for longer than the old retries
    let (d2, p2) = (down.clone(), progress.clone());
    let watcher = tokio::spawn(async move {
        while p2.bytes.load(Ordering::Relaxed) < 200_000 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        d2.store(true, Ordering::Relaxed);
        let mut saw_waiting = false;
        for _ in 0..60 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            saw_waiting |= p2.waiting.load(Ordering::Relaxed);
        }
        d2.store(false, Ordering::Relaxed);
        saw_waiting
    });
    http_download::run(
        shared,
        job,
        segs,
        progress.clone(),
        CancellationToken::new(),
    )
    .await
    .expect("should wait for the connection and finish");
    assert!(
        watcher.await.unwrap(),
        "should have shown 'waiting for connection'"
    );
    assert!(!progress.waiting.load(Ordering::Relaxed));
    let out = std::fs::read(&part).unwrap();
    assert!(out == *data, "downloaded bytes differ from the source");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn network_errors_are_told_apart() {
    use swiftget_engine::http::{http_status_of, is_network_error, HttpStatus, NetDrop};
    let drop: anyhow::Error = NetDrop("connection closed early".into()).into();
    assert!(is_network_error(&drop));
    assert!(is_network_error(&drop.context("part 3")));
    let reset: anyhow::Error = std::io::Error::from(std::io::ErrorKind::ConnectionReset).into();
    assert!(is_network_error(&reset));
    let full: anyhow::Error = std::io::Error::from(std::io::ErrorKind::StorageFull).into();
    assert!(!is_network_error(&full));
    let gone: anyhow::Error = HttpStatus(410).into();
    assert!(!is_network_error(&gone));
    assert_eq!(http_status_of(&gone.context("x")), Some(410));
}

/// The folder changed in "Download File Info" while it was already
/// downloading (in the first folder), then stopped and resumed: the file in
/// the new folder must be the whole file, not a nearly empty one.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn folder_changed_while_downloading() {
    let data = test_data(3 * 1024 * 1024 + 7);
    let port = serve(data.clone()).await;
    let url = format!("http://127.0.0.1:{port}/files/test.bin");
    let root = std::env::temp_dir().join(format!("ezoo-retarget-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (first, second) = (root.join("first"), root.join("second"));
    std::fs::create_dir_all(&first).unwrap();
    let mgr =
        swiftget_engine::Manager::new(tokio::runtime::Handle::current(), root.join("app")).unwrap();
    let id = mgr.add(swiftget_engine::NewDownload {
        url,
        save_dir: Some(first.clone()),
        start: true,
        ..Default::default()
    });
    // part of it is there
    for _ in 0..400 {
        if mgr
            .get(id)
            .map(|v| v.downloaded > 256 * 1024)
            .unwrap_or(false)
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    mgr.pause(id);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let name = mgr.get(id).unwrap().file_name;
    mgr.retarget(id, &second, &name, None, None).unwrap();
    mgr.start(id);
    let mut status = swiftget_engine::Status::Queued;
    for _ in 0..1500 {
        status = mgr.get(id).unwrap().status;
        if matches!(
            status,
            swiftget_engine::Status::Completed | swiftget_engine::Status::Failed
        ) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        status,
        swiftget_engine::Status::Completed,
        "{:?}",
        mgr.get(id).unwrap().error
    );
    let v = mgr.get(id).unwrap();
    assert!(v.save_path.starts_with(&second), "{:?}", v.save_path);
    let got = std::fs::read(&v.save_path).unwrap();
    assert_eq!(got.len(), data.len());
    assert!(
        got == *data,
        "the file in the new folder is not the downloaded file"
    );
    mgr.remove(id, false);
    let _ = std::fs::remove_dir_all(&root);
}

/// The folder changed in "Download File Info" while it was already
/// downloading (in the first folder), then stopped and resumed: the file in
/// the new folder must be the whole file, not a nearly empty one.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn folder_changed_while_running() {
    let data = test_data(3 * 1024 * 1024 + 7);
    let port = serve(data.clone()).await;
    let url = format!("http://127.0.0.1:{port}/files/test.bin");
    let root = std::env::temp_dir().join(format!("ezoo-retarget2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (first, second) = (root.join("first"), root.join("second"));
    std::fs::create_dir_all(&first).unwrap();
    let mgr =
        swiftget_engine::Manager::new(tokio::runtime::Handle::current(), root.join("app")).unwrap();
    let id = mgr.add(swiftget_engine::NewDownload {
        url,
        save_dir: Some(first.clone()),
        start: true,
        ..Default::default()
    });
    // part of it is there
    for _ in 0..400 {
        if mgr
            .get(id)
            .map(|v| v.downloaded > 256 * 1024)
            .unwrap_or(false)
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let name = mgr.get(id).unwrap().file_name;
    mgr.retarget(id, &second, &name, None, None).unwrap();
    let mut status = swiftget_engine::Status::Queued;
    for _ in 0..1500 {
        status = mgr.get(id).unwrap().status;
        if matches!(
            status,
            swiftget_engine::Status::Completed | swiftget_engine::Status::Failed
        ) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        status,
        swiftget_engine::Status::Completed,
        "{:?}",
        mgr.get(id).unwrap().error
    );
    let v = mgr.get(id).unwrap();
    assert!(v.save_path.starts_with(&second), "{:?}", v.save_path);
    let got = std::fs::read(&v.save_path).unwrap();
    assert_eq!(got.len(), data.len());
    assert!(
        got == *data,
        "the file in the new folder is not the downloaded file"
    );
    mgr.remove(id, false);
    let _ = std::fs::remove_dir_all(&root);
}

/// A web server with a few files by path (Range supported, a bit slow).
async fn serve_files(files: Vec<(&'static str, Arc<Vec<u8>>)>) -> u16 {
    let files = Arc::new(files);
    let (port, _) = testserver::spawn({
        let files = files.clone();
        move |req: &testserver::Request| {
            let Some((_, data)) = files.iter().find(|(p, _)| *p == req.path) else {
                return testserver::Action::Respond(testserver::Response::new(404, Vec::new()));
            };
            let range = req
                .range()
                .map(|(a, b)| (a, b.unwrap_or(data.len() - 1).min(data.len() - 1)));
            let (a, b) = range.unwrap_or((0, data.len() - 1));
            let resp = if range.is_some() {
                testserver::StreamResponse::new(206, data[a..=b].to_vec())
                    .header("Content-Range", &format!("bytes {a}-{b}/{}", data.len()))
            } else {
                testserver::StreamResponse::new(200, data.clone().to_vec())
            };
            testserver::Action::Stream(
                resp.chunk_size(16384)
                    .chunk_delay(Duration::from_millis(15)),
            )
        }
    });
    port
}

/// Picture and sound in two files (a video site): the folder changed while
/// it downloads. The joined file in the new folder must have both.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn video_and_sound_folder_changed() {
    let (Ok(v), Ok(a)) = (
        std::fs::read(std::env::var("EZOO_TEST_VIDEO").unwrap_or_default()),
        std::fs::read(std::env::var("EZOO_TEST_AUDIO").unwrap_or_default()),
    ) else {
        return; // test media not given
    };
    let (vlen, alen) = (v.len(), a.len());
    let port = serve_files(vec![("/v.mp4", Arc::new(v)), ("/a.m4a", Arc::new(a))]).await;
    let root = std::env::temp_dir().join(format!("ezoo-retarget-av-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (first, second) = (root.join("first"), root.join("second"));
    std::fs::create_dir_all(&first).unwrap();
    let mgr =
        swiftget_engine::Manager::new(tokio::runtime::Handle::current(), root.join("app")).unwrap();
    let id = mgr.add(swiftget_engine::NewDownload {
        url: format!("http://127.0.0.1:{port}/v.mp4"),
        audio_url: Some(format!("http://127.0.0.1:{port}/a.m4a")),
        file_name: Some("clip.mp4".into()),
        save_dir: Some(first.clone()),
        early: true,
        start: true,
        ..Default::default()
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_ne!(
        mgr.get(id).unwrap().status,
        swiftget_engine::Status::Completed,
        "too fast for the test"
    );
    mgr.pause(id);
    tokio::time::sleep(Duration::from_millis(300)).await;
    mgr.retarget(id, &second, "my clip.mp4", None, None)
        .unwrap();
    mgr.start(id);
    let mut status = swiftget_engine::Status::Queued;
    for _ in 0..1500 {
        status = mgr.get(id).unwrap().status;
        if matches!(
            status,
            swiftget_engine::Status::Completed | swiftget_engine::Status::Failed
        ) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        status,
        swiftget_engine::Status::Completed,
        "{:?}",
        mgr.get(id).unwrap().error
    );
    let saved = mgr.get(id).unwrap().save_path;
    assert!(saved.starts_with(&second), "{saved:?}");
    let size = std::fs::metadata(&saved).unwrap().len() as usize;
    assert!(
        size > vlen && size > alen,
        "joined file is {size} bytes (video {vlen}, sound {alen})"
    );
    mgr.remove(id, false);
    let _ = std::fs::remove_dir_all(&root);
}

/// Options > connections changed while a file downloads: fewer, then more,
/// at once and without a restart; the file is still byte for byte right.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn connections_changed_while_downloading() {
    let data = test_data(6 * 1024 * 1024 + 77);
    let port = serve(data.clone()).await;
    let url = format!("http://127.0.0.1:{port}/files/conns.bin");
    let shared = Arc::new(Shared::new(build_client().unwrap(), SpeedLimiter::new(0)));
    let p = probe(&shared.client(), &url, &RequestHeaders::default(), "test")
        .await
        .unwrap();
    let dir = std::env::temp_dir().join(format!("swiftget-conns-{port}"));
    std::fs::create_dir_all(&dir).unwrap();
    let part = dir.join("out.sgpart");
    let _ = std::fs::remove_file(&part);
    let job = HttpJob {
        url,
        headers: RequestHeaders::default(),
        user_agent: "test".into(),
        total_size: p.total_size,
        resumable: true,
        part_path: part.clone(),
        connections: 8,
        min_split: 64 * 1024,
        retries: 3,
        progress_base: 0,
        max_request: None,
        limiter: Arc::new(SpeedLimiter::new(0)),
        adaptive: Default::default(),
        learned_start: None,
        report: None,
    };
    let segs = Arc::new(Mutex::new(Vec::new()));
    let progress = Arc::new(Progress::default());
    let p2 = progress.clone();
    let total = data.len() as u64;
    let changer = tokio::spawn(async move {
        let mut fewest = u32::MAX;
        let mut most = 0;
        // 8 -> 2 at a fifth of the file
        while p2.bytes.load(Ordering::Relaxed) < total / 5 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        p2.want_connections.store(2, Ordering::Relaxed);
        for _ in 0..300 {
            tokio::time::sleep(Duration::from_millis(5)).await;
            fewest = fewest.min(p2.connections.load(Ordering::Relaxed));
            if p2.bytes.load(Ordering::Relaxed) > total / 2 {
                break;
            }
        }
        // then 16
        p2.want_connections.store(16, Ordering::Relaxed);
        for _ in 0..600 {
            tokio::time::sleep(Duration::from_millis(5)).await;
            most = most.max(p2.connections.load(Ordering::Relaxed));
        }
        (fewest, most)
    });
    http_download::run(
        shared,
        job,
        segs,
        progress.clone(),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    let (fewest, most) = changer.await.unwrap();
    assert!(fewest <= 2, "fewer connections at once: {fewest}");
    assert!(most > 8, "more connections at once: {most}");
    assert_eq!(std::fs::read(&part).unwrap(), *data, "the file is right");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A link that expires: "/get" sends the browser on to "/file?t=N"; after
/// about a third of the file every old token answers 403. The download asks
/// "/get" again, checks that the new link is the same file and goes on.
async fn serve_expiring(data: Arc<Vec<u8>>, wrong_size_once: bool) -> u16 {
    use std::sync::atomic::{AtomicU64, AtomicUsize};
    let token = Arc::new(AtomicU64::new(1));
    let sent = Arc::new(AtomicUsize::new(0));
    let wrong = Arc::new(std::sync::atomic::AtomicBool::new(wrong_size_once));
    let (port, _) = testserver::spawn({
        let data = data.clone();
        let token = token.clone();
        let sent = sent.clone();
        let wrong = wrong.clone();
        move |req: &testserver::Request| {
            if req.path == "/get" {
                // a fresh link each time it is asked
                let t = token.load(Ordering::SeqCst);
                return testserver::Action::Respond(
                    testserver::Response::new(302, Vec::new())
                        .header("Location", &format!("/file?t={t}")),
                );
            }
            let t: u64 = req
                .path
                .rsplit('=')
                .next()
                .and_then(|x| x.parse().ok())
                .unwrap_or(0);
            if t != token.load(Ordering::SeqCst) {
                return testserver::Action::Respond(testserver::Response::new(403, Vec::new()));
            }
            // the first new link once gives another (smaller) file
            let len = if t == 2 && wrong.swap(false, Ordering::SeqCst) {
                data.len() / 2
            } else {
                data.len()
            };
            let range = req
                .range()
                .map(|(a, b)| (a, b.unwrap_or(len - 1).min(len - 1)));
            let (a, b) = range.unwrap_or((0, len - 1));
            let b = b.min(len - 1);
            let token2 = token.clone();
            let sent2 = sent.clone();
            let data_len = data.len();
            testserver::Action::Stream(
                testserver::StreamResponse::new(206, data[a..=b].to_vec())
                    .header("Content-Range", &format!("bytes {a}-{b}/{len}"))
                    .header("Content-Disposition", "attachment; filename=\"exp.bin\"")
                    .chunk_delay(Duration::from_millis(2))
                    .on_chunk(move |chunk_len| {
                        // expire once, after a third of the file
                        if sent2.fetch_add(chunk_len, Ordering::SeqCst) + chunk_len > data_len / 3
                            && token2.load(Ordering::SeqCst) == 1
                        {
                            token2.store(2, Ordering::SeqCst);
                        }
                        // expired while sending: the connection drops
                        token2.load(Ordering::SeqCst) == t
                    }),
            )
        }
    });
    port
}

async fn run_expiring(wrong_size_once: bool) {
    let data = test_data(3 * 1024 * 1024 + 11);
    let port = serve_expiring(data.clone(), wrong_size_once).await;
    let root = std::env::temp_dir().join(format!("ezoo-expire-{}-{port}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("out");
    std::fs::create_dir_all(&dir).unwrap();
    let mgr =
        swiftget_engine::Manager::new(tokio::runtime::Handle::current(), root.join("app")).unwrap();
    let id = mgr.add(swiftget_engine::NewDownload {
        url: format!("http://127.0.0.1:{port}/get"),
        save_dir: Some(dir.clone()),
        start: true,
        ..Default::default()
    });
    let mut status = swiftget_engine::Status::Queued;
    for _ in 0..3000 {
        status = mgr.get(id).unwrap().status;
        if matches!(
            status,
            swiftget_engine::Status::Completed | swiftget_engine::Status::Failed
        ) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        status,
        swiftget_engine::Status::Completed,
        "{:?}",
        mgr.get(id).unwrap().error
    );
    let v = mgr.get(id).unwrap();
    assert!(
        std::fs::read(&v.save_path).unwrap() == *data,
        "the file is right after the new link"
    );
    mgr.remove(id, false);
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn expired_link_is_renewed() {
    run_expiring(false).await;
}

/// The first new link gives another file: nothing of it is written, the
/// next one (the right file) is used.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wrong_new_link_is_not_used() {
    run_expiring(true).await;
}

/// A busy server: after a third of the file every request gets "503" for a
/// while, and many answers break off half way. The download must finish by
/// itself (no "Resume" click), and the file must be right.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn busy_server_needs_no_resume_click() {
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize};
    let data = test_data(4 * 1024 * 1024 + 5);
    let sent = Arc::new(AtomicUsize::new(0));
    // when the busy time ends (0 = not busy yet)
    let busy_until = Arc::new(AtomicU64::new(0));
    let was_busy = Arc::new(AtomicBool::new(false));
    let served = Arc::new(AtomicUsize::new(0));
    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
    let (port, _) = testserver::spawn({
        let data = data.clone();
        let sent = sent.clone();
        let busy_until = busy_until.clone();
        let was_busy = was_busy.clone();
        let served = served.clone();
        move |req: &testserver::Request| {
            let until = busy_until.load(Ordering::SeqCst);
            if until != 0 && now_ms() < until {
                return testserver::Action::Respond(testserver::Response::new(503, Vec::new()));
            }
            let n = served.fetch_add(1, Ordering::SeqCst);
            let range = req
                .range()
                .map(|(a, b)| (a, b.unwrap_or(data.len() - 1).min(data.len() - 1)));
            let (a, b) = range.unwrap_or((0, data.len() - 1));
            let body = &data[a..=b];
            // every third answer breaks off half way
            let cut = if n % 3 == 2 {
                body.len() / 2
            } else {
                body.len()
            };
            let sent2 = sent.clone();
            let busy_until2 = busy_until.clone();
            let was_busy2 = was_busy.clone();
            let data_len = data.len();
            testserver::Action::Stream(
                testserver::StreamResponse::new(206, body[..cut].to_vec())
                    .header("Content-Range", &format!("bytes {a}-{b}/{}", data.len()))
                    .header("Content-Disposition", "attachment; filename=\"busy.bin\"")
                    .content_length(b - a + 1)
                    .chunk_delay(Duration::from_millis(1))
                    .on_chunk(move |chunk_len| {
                        if sent2.fetch_add(chunk_len, Ordering::SeqCst) + chunk_len > data_len / 3
                            && !was_busy2.swap(true, Ordering::SeqCst)
                        {
                            busy_until2.store(now_ms() + 20_000, Ordering::SeqCst);
                        }
                        // busy now: the line drops
                        !(busy_until2.load(Ordering::SeqCst) > now_ms())
                    }),
            )
        }
    });
    let root = std::env::temp_dir().join(format!("ezoo-busy-{}-{port}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("out");
    std::fs::create_dir_all(&dir).unwrap();
    let mgr =
        swiftget_engine::Manager::new(tokio::runtime::Handle::current(), root.join("app")).unwrap();
    let id = mgr.add(swiftget_engine::NewDownload {
        url: format!("http://127.0.0.1:{port}/busy.bin"),
        save_dir: Some(dir.clone()),
        start: true,
        ..Default::default()
    });
    let mut status = swiftget_engine::Status::Queued;
    let mut saw_failed = false;
    for _ in 0..6000 {
        status = mgr.get(id).unwrap().status;
        if status == swiftget_engine::Status::Failed {
            saw_failed = true;
        }
        if status == swiftget_engine::Status::Completed {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        was_busy.load(Ordering::SeqCst),
        "the server was busy for a while"
    );
    assert!(
        !saw_failed,
        "it stopped and waited for a click: {:?}",
        mgr.get(id).unwrap().error
    );
    assert_eq!(
        status,
        swiftget_engine::Status::Completed,
        "{:?}",
        mgr.get(id).unwrap().error
    );
    let v = mgr.get(id).unwrap();
    assert!(
        std::fs::read(&v.save_path).unwrap() == *data,
        "the file is right"
    );
    mgr.remove(id, false);
    let _ = std::fs::remove_dir_all(&root);
}

/// A server that takes some requests and never answers them (no headers, or
/// it stops sending): YouTube does this now and then. The download must not
/// sit at 0 waiting for ever: it asks again by itself and finishes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn silent_server_does_not_hold_the_download() {
    use std::sync::atomic::AtomicUsize;
    let data = test_data(3 * 1024 * 1024 + 3);
    let asked = Arc::new(AtomicUsize::new(0));
    let (port, _) = testserver::spawn({
        let data = data.clone();
        let asked = asked.clone();
        move |req: &testserver::Request| {
            let n = asked.fetch_add(1, Ordering::SeqCst);
            let range = req
                .range()
                .map(|(a, b)| (a, b.unwrap_or(data.len() - 1).min(data.len() - 1)));
            let (a, b) = range.unwrap_or((0, data.len() - 1));
            let body = &data[a..=b];
            // the very first request (the size check) and every fourth
            // one: the connection stays open, nothing is ever sent
            // (the answer would only arrive after an hour, long after
            // this test ends, so the client must retry on its own)
            if n == 0 || n % 4 == 3 {
                return testserver::Action::Stream(
                    testserver::StreamResponse::new(206, body.to_vec())
                        .header("Content-Range", &format!("bytes {a}-{b}/{}", data.len()))
                        .header("Content-Disposition", "attachment; filename=\"quiet.bin\"")
                        .content_length(b - a + 1)
                        .hold_before(Duration::from_secs(3600)),
                );
            }
            // every fifth answer stops half way and then keeps quiet
            let quiet_after = if n % 5 == 4 {
                body.len() / 2
            } else {
                body.len()
            };
            testserver::Action::Stream(
                testserver::StreamResponse::new(206, body[..quiet_after].to_vec())
                    .header("Content-Range", &format!("bytes {a}-{b}/{}", data.len()))
                    .header("Content-Disposition", "attachment; filename=\"quiet.bin\"")
                    .content_length(b - a + 1)
                    .chunk_delay(Duration::from_millis(1))
                    .hold_after(if quiet_after < body.len() {
                        Duration::from_secs(3600)
                    } else {
                        Duration::ZERO
                    }),
            )
        }
    });
    let root = std::env::temp_dir().join(format!("ezoo-quiet-{}-{port}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("out");
    std::fs::create_dir_all(&dir).unwrap();
    let mgr =
        swiftget_engine::Manager::new(tokio::runtime::Handle::current(), root.join("app")).unwrap();
    let mut s = mgr.settings();
    s.connections = 8;
    mgr.set_settings(s).unwrap();
    let id = mgr.add(swiftget_engine::NewDownload {
        url: format!("http://127.0.0.1:{port}/quiet.bin"),
        save_dir: Some(dir.clone()),
        start: true,
        ..Default::default()
    });
    let mut status = swiftget_engine::Status::Queued;
    let mut saw_failed = false;
    // a little over two minutes at most
    for _ in 0..7000 {
        status = mgr.get(id).unwrap().status;
        saw_failed |= status == swiftget_engine::Status::Failed;
        if status == swiftget_engine::Status::Completed {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!saw_failed, "it stopped: {:?}", mgr.get(id).unwrap().error);
    assert_eq!(
        status,
        swiftget_engine::Status::Completed,
        "{:?}",
        mgr.get(id).unwrap().error
    );
    let v = mgr.get(id).unwrap();
    assert!(
        std::fs::read(&v.save_path).unwrap() == *data,
        "the file is right"
    );
    mgr.remove(id, false);
    let _ = std::fs::remove_dir_all(&root);
}

/// HLS: the list address sends the player on to another folder (relative
/// addresses are relative to that one), and the parts are byte ranges of one
/// file with an init part (#EXT-X-MAP / #EXT-X-BYTERANGE).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn hls_byte_ranges_after_a_redirect() {
    let media = test_data(300_000);
    let (port, _) = testserver::spawn({
        let media = media.clone();
        move |req: &testserver::Request| {
            match req.path.as_str() {
                "/watch/list.m3u8" => testserver::Action::Respond(
                    testserver::Response::new(302, Vec::new())
                        .header("Location", "/cdn/v1/master.m3u8"),
                ),
                "/cdn/v1/master.m3u8" => testserver::Action::Respond(
                    testserver::Response::new(
                        200,
                        b"#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=900000\nhd/index.m3u8\n".to_vec(),
                    ),
                ),
                "/cdn/v1/hd/index.m3u8" => testserver::Action::Respond(
                    testserver::Response::new(
                        200,
                        b"#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXT-X-MAP:URI=\"all.mp4\",BYTERANGE=\"1000@0\"\n#EXTINF:6,\n#EXT-X-BYTERANGE:99000@1000\nall.mp4\n#EXTINF:6,\n#EXT-X-BYTERANGE:100000\nall.mp4\n#EXTINF:6,\n#EXT-X-BYTERANGE:100000\nall.mp4\n#EXT-X-ENDLIST\n".to_vec(),
                    ),
                ),
                "/cdn/v1/hd/all.mp4" => match req.range() {
                    Some((a, b)) => {
                        let b = b.unwrap_or(media.len() - 1).min(media.len() - 1);
                        testserver::Action::Respond(
                            testserver::Response::new(206, media[a..=b].to_vec()).header(
                                "Content-Range",
                                &format!("bytes {a}-{b}/{}", media.len()),
                            ),
                        )
                    }
                    None => testserver::Action::Respond(
                        testserver::Response::new(200, media.clone().to_vec()),
                    ),
                },
                _ => testserver::Action::Respond(testserver::Response::new(404, Vec::new())),
            }
        }
    });
    let dir = std::env::temp_dir().join(format!("ezoo-hls-range-{port}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let job = swiftget_engine::hls::HlsJob {
        url: format!("http://127.0.0.1:{port}/watch/list.m3u8"),
        headers: RequestHeaders::default(),
        user_agent: "test".into(),
        output: dir.join("out.mp4"),
        temp_dir: dir.join("parts"),
        connections: 3,
        retries: 2,
        limiter: Arc::new(SpeedLimiter::new(0)),
    };
    let shared = Arc::new(Shared::new(build_client().unwrap(), SpeedLimiter::new(0)));
    swiftget_engine::hls::run(
        shared,
        job,
        Arc::new(Progress::default()),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    let got = std::fs::read(dir.join("out.mp4")).unwrap();
    assert!(
        got == *media,
        "the stream put together is the file ({} of {} bytes)",
        got.len(),
        media.len()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Like `serve`, but the first half of the file is slow (100 ms per 8 KB
/// chunk: about 80 KB/s) and the second half fast.
async fn serve_slow_half(data: Arc<Vec<u8>>) -> u16 {
    let (port, _) = testserver::spawn({
        let data = data.clone();
        move |req: &testserver::Request| {
            let range = req
                .range()
                .map(|(a, b)| (a, b.unwrap_or(data.len() - 1).min(data.len() - 1)));
            let (a, b) = range.unwrap_or((0, data.len() - 1));
            let slow_until = data.len() / 2;
            let resp = if range.is_some() {
                testserver::StreamResponse::new(206, data[a..=b].to_vec())
                    .header("Content-Range", &format!("bytes {a}-{b}/{}", data.len()))
            } else {
                testserver::StreamResponse::new(200, data.clone().to_vec())
            };
            // the chunk's offset decides: first half slow, rest fast
            testserver::Action::Stream(resp.chunk_delay_fn(move |offset, _| {
                if a + offset < slow_until {
                    Duration::from_millis(100)
                } else {
                    Duration::from_millis(1)
                }
            }))
        }
    });
    port
}

/// Q32: one half of the file is slow (a throttled server), the other fast.
/// The fast connection must not sit idle while the slow half crawls alone:
/// it takes half of the slow remainder, so the download ends in well under
/// the time the slow half would take by itself (~25 s here).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn slow_segment_is_shared_not_left_alone() {
    let data = test_data(4 * 1024 * 1024);
    let port = serve_slow_half(data.clone()).await;
    let url = format!("http://127.0.0.1:{port}/slow.bin");
    let shared = Arc::new(Shared::new(build_client().unwrap(), SpeedLimiter::new(0)));
    let dir = std::env::temp_dir().join(format!("swiftget-slow-{port}"));
    std::fs::create_dir_all(&dir).unwrap();
    let part = dir.join("out.sgpart");
    let job = HttpJob {
        url,
        headers: RequestHeaders::default(),
        user_agent: "test".into(),
        total_size: Some(data.len() as u64),
        resumable: true,
        part_path: part.clone(),
        connections: 2,
        min_split: 64 * 1024,
        retries: 3,
        progress_base: 0,
        max_request: None,
        limiter: Arc::new(SpeedLimiter::new(0)),
        adaptive: Default::default(),
        learned_start: None,
        report: None,
    };
    let segs = Arc::new(Mutex::new(Vec::new()));
    let progress = Arc::new(Progress::default());
    let started = std::time::Instant::now();
    let run = http_download::run(
        shared,
        job,
        segs,
        progress.clone(),
        CancellationToken::new(),
    );
    tokio::time::timeout(Duration::from_secs(30), run)
        .await
        .expect("the download did not finish in 30 s")
        .unwrap();
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(18),
        "took {elapsed:?}: the slow half was left alone"
    );
    assert_eq!(std::fs::read(&part).unwrap(), *data, "the file is right");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Q30: one download per site at a time. Two files from the same host with
/// the limit at 1: the second waits in the queue while the first runs,
/// then runs after it — both end up whole.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn per_site_limit_runs_one_download_at_a_time() {
    let data = test_data(4 * 1024 * 1024 + 5);
    let port = serve(data.clone()).await;
    let root = std::env::temp_dir().join(format!("ezoo-site-limit-{port}"));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("out");
    std::fs::create_dir_all(&dir).unwrap();
    let mgr =
        swiftget_engine::Manager::new(tokio::runtime::Handle::current(), root.join("app")).unwrap();
    let mut s = mgr.settings();
    s.server_limits = vec![swiftget_engine::config::ServerLimit {
        host: "127.0.0.1".into(),
        connections: 32,
        max_downloads: Some(1),
    }];
    mgr.set_settings(s).unwrap();
    let id1 = mgr.add(swiftget_engine::NewDownload {
        url: format!("http://127.0.0.1:{port}/one.bin"),
        save_dir: Some(dir.clone()),
        start: false,
        ..Default::default()
    });
    let id2 = mgr.add(swiftget_engine::NewDownload {
        url: format!("http://127.0.0.1:{port}/two.bin"),
        save_dir: Some(dir.clone()),
        start: false,
        ..Default::default()
    });
    mgr.start(id1);
    mgr.start(id2);
    // the first runs, the second waits in the queue
    let mut saw_gating = false;
    for _ in 0..400 {
        let v1 = mgr.get(id1).unwrap();
        let v2 = mgr.get(id2).unwrap();
        if matches!(
            v1.status,
            swiftget_engine::Status::Downloading | swiftget_engine::Status::Connecting
        ) {
            assert_eq!(
                v2.status,
                swiftget_engine::Status::Queued,
                "the second download started too"
            );
            saw_gating = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        saw_gating,
        "the first download finished before the gate could be seen"
    );
    // both finish, one after the other
    for _ in 0..6000 {
        let s1 = mgr.get(id1).unwrap().status;
        let s2 = mgr.get(id2).unwrap().status;
        if s1 == swiftget_engine::Status::Completed && s2 == swiftget_engine::Status::Completed {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        mgr.get(id1).unwrap().status,
        swiftget_engine::Status::Completed,
        "{:?}",
        mgr.get(id1).unwrap().error
    );
    assert_eq!(
        mgr.get(id2).unwrap().status,
        swiftget_engine::Status::Completed,
        "{:?}",
        mgr.get(id2).unwrap().error
    );
    for id in [id1, id2] {
        let v = mgr.get(id).unwrap();
        assert!(
            std::fs::read(&v.save_path).unwrap() == *data,
            "the file is right"
        );
        mgr.remove(id, false);
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// A web server with a few files by path (Range supported), each with an
/// ETag when given one, and a fixed Last-Modified date for the tagged ones.
type TaggedFile = (&'static str, Arc<Vec<u8>>, Option<&'static str>);
async fn serve_tagged(files: Vec<TaggedFile>) -> u16 {
    let files = Arc::new(files);
    let (port, _) = testserver::spawn({
        let files = files.clone();
        move |req: &testserver::Request| {
            let Some((_, data, etag)) = files.iter().find(|(p, _, _)| *p == req.path) else {
                return testserver::Action::Respond(testserver::Response::new(404, Vec::new()));
            };
            let range = req
                .range()
                .map(|(a, b)| (a, b.unwrap_or(data.len() - 1).min(data.len() - 1)));
            let (a, b) = range.unwrap_or((0, data.len() - 1));
            let mut resp = if range.is_some() {
                testserver::StreamResponse::new(206, data[a..=b].to_vec())
                    .header("Content-Range", &format!("bytes {a}-{b}/{}", data.len()))
            } else {
                testserver::StreamResponse::new(200, data.clone().to_vec())
            };
            if let Some(e) = etag {
                resp = resp
                    .header("ETag", e)
                    .header("Last-Modified", "Thu, 01 Oct 2026 00:00:00 GMT");
            }
            resp = resp.header("Content-Disposition", "attachment; filename=\"tagged.bin\"");
            testserver::Action::Stream(resp.chunk_size(16384).chunk_delay(Duration::from_millis(2)))
        }
    });
    port
}

/// Q31: refresh-link only takes the same file. Same size + same strong
/// ETag: yes. Same size but another ETag: no. Another size: no. A new
/// link without any validator when the old one had one: no. And when the
/// download never had a validator, size + resumable is enough.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn refresh_link_accepts_same_file_rejects_other() {
    let data_a = test_data(3 * 1024 * 1024 + 11);
    let data_c: Arc<Vec<u8>> = Arc::new(
        (0..3 * 1024 * 1024 + 11)
            .map(|i| (i * 17 % 251) as u8)
            .collect(),
    );
    let data_b = test_data(1024 * 1024 + 5);
    let port = serve_tagged(vec![
        ("/a.bin", data_a.clone(), Some("\"aaa\"")),
        ("/b.bin", data_b.clone(), Some("\"bbb\"")),
        ("/c.bin", data_c.clone(), Some("\"ccc\"")),
        ("/e.bin", data_a.clone(), None),
    ])
    .await;
    let base = format!("http://127.0.0.1:{port}");
    let root = std::env::temp_dir().join(format!("ezoo-refresh-{port}"));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("out");
    std::fs::create_dir_all(&dir).unwrap();
    let mgr =
        swiftget_engine::Manager::new(tokio::runtime::Handle::current(), root.join("app")).unwrap();
    let id = mgr.add(swiftget_engine::NewDownload {
        url: format!("{base}/a.bin"),
        save_dir: Some(dir.clone()),
        start: true,
        ..Default::default()
    });
    // part of it is there
    for _ in 0..400 {
        if mgr
            .get(id)
            .map(|v| v.downloaded > 256 * 1024)
            .unwrap_or(false)
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    mgr.pause(id);
    tokio::time::sleep(Duration::from_millis(300)).await;
    // same size, another ETag: no
    assert!(mgr
        .refresh_link(id, &format!("{base}/c.bin"))
        .await
        .is_err());
    assert_eq!(mgr.get(id).unwrap().url, format!("{base}/a.bin"));
    // another size: no
    assert!(mgr
        .refresh_link(id, &format!("{base}/b.bin"))
        .await
        .is_err());
    assert_eq!(mgr.get(id).unwrap().url, format!("{base}/a.bin"));
    // same file but the new link lost its validator: no
    assert!(mgr
        .refresh_link(id, &format!("{base}/e.bin"))
        .await
        .is_err());
    assert_eq!(mgr.get(id).unwrap().url, format!("{base}/a.bin"));
    // the same file: yes, and it goes on from where it was
    mgr.refresh_link(id, &format!("{base}/a.bin"))
        .await
        .unwrap();
    mgr.start(id);
    let mut status = swiftget_engine::Status::Queued;
    for _ in 0..3000 {
        status = mgr.get(id).unwrap().status;
        if matches!(
            status,
            swiftget_engine::Status::Completed | swiftget_engine::Status::Failed
        ) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        status,
        swiftget_engine::Status::Completed,
        "{:?}",
        mgr.get(id).unwrap().error
    );
    let v = mgr.get(id).unwrap();
    assert!(
        std::fs::read(&v.save_path).unwrap() == *data_a,
        "the file is right"
    );
    mgr.remove(id, false);

    // no validator anywhere: size + resumable is enough
    let port2 = serve(data_a.clone()).await;
    let port3 = serve(data_b.clone()).await;
    let id2 = mgr.add(swiftget_engine::NewDownload {
        url: format!("http://127.0.0.1:{port2}/plain.bin"),
        save_dir: Some(dir.clone()),
        start: true,
        ..Default::default()
    });
    for _ in 0..400 {
        if mgr
            .get(id2)
            .map(|v| v.downloaded > 256 * 1024)
            .unwrap_or(false)
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    mgr.pause(id2);
    tokio::time::sleep(Duration::from_millis(300)).await;
    mgr.refresh_link(id2, &format!("http://127.0.0.1:{port2}/plain.bin"))
        .await
        .unwrap();
    assert!(mgr
        .refresh_link(id2, &format!("http://127.0.0.1:{port3}/plain.bin"))
        .await
        .is_err());
    mgr.start(id2);
    let mut status = swiftget_engine::Status::Queued;
    for _ in 0..3000 {
        status = mgr.get(id2).unwrap().status;
        if matches!(
            status,
            swiftget_engine::Status::Completed | swiftget_engine::Status::Failed
        ) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        status,
        swiftget_engine::Status::Completed,
        "{:?}",
        mgr.get(id2).unwrap().error
    );
    let v = mgr.get(id2).unwrap();
    assert!(
        std::fs::read(&v.save_path).unwrap() == *data_a,
        "the file is right"
    );
    mgr.remove(id2, false);
    let _ = std::fs::remove_dir_all(&root);
}

/// The UI path: `refresh_link_with_headers` probes with the fresh
/// headers (as the browser capture hands them over), swaps the link
/// and keeps those headers on the record; a different file is refused
/// and the old link stays.
#[tokio::test]
async fn refresh_link_with_headers_uses_given_headers() {
    let data = test_data(512 * 1024 + 7);
    let other = test_data(256 * 1024 + 3);
    let port = serve_tagged(vec![
        ("/a.bin", data.clone(), Some("\"aaa\"")),
        ("/b.bin", other.clone(), Some("\"bbb\"")),
    ])
    .await;
    let base = format!("http://127.0.0.1:{port}");
    let root = std::env::temp_dir().join(format!("ezoo-refresh-hdr-{port}"));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("out");
    std::fs::create_dir_all(&dir).unwrap();
    let mgr =
        swiftget_engine::Manager::new(tokio::runtime::Handle::current(), root.join("app")).unwrap();
    let id = mgr.add(swiftget_engine::NewDownload {
        url: format!("{base}/a.bin"),
        save_dir: Some(dir.clone()),
        start: true,
        ..Default::default()
    });
    // let part of it arrive first, so the record knows the size
    for _ in 0..400 {
        if mgr
            .get(id)
            .map(|v| v.downloaded > 64 * 1024)
            .unwrap_or(false)
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    mgr.pause(id);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let headers = swiftget_engine::RequestHeaders {
        referer: Some(format!("{base}/page")),
        ..Default::default()
    };
    // same file, fresh headers: yes, and the headers are kept
    mgr.refresh_link_with_headers(id, &format!("{base}/a.bin"), headers.clone())
        .await
        .unwrap();
    let v = mgr.get(id).unwrap();
    assert_eq!(v.url, format!("{base}/a.bin"));
    assert_eq!(v.headers.referer, Some(format!("{base}/page")));
    // another file: no, the link and the headers stay as they were
    assert!(mgr
        .refresh_link_with_headers(id, &format!("{base}/b.bin"), headers)
        .await
        .is_err());
    let v = mgr.get(id).unwrap();
    assert_eq!(v.url, format!("{base}/a.bin"));
    mgr.remove(id, false);
    let _ = std::fs::remove_dir_all(&root);
}

/// Q26 plumbing (Phase 5): the quality the overlay offered for a download
/// travels with the record — AddRequest -> NewDownload -> stored record —
/// so a later quality window can offer the same choice again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quality_key_is_stored_with_the_record() {
    let root = std::env::temp_dir().join(format!("ezoo-quality-key-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let mgr =
        swiftget_engine::Manager::new(tokio::runtime::Handle::current(), root.join("app")).unwrap();
    let id = mgr.add(swiftget_engine::NewDownload {
        url: "http://127.0.0.1:1/video.mp4".to_string(),
        file_name: Some("video.mp4".to_string()),
        source_page: Some("https://www.youtube.com/watch?v=AAAAAAAAAAA".to_string()),
        source_key: Some("1080p|MP4".to_string()),
        quality_key: Some("1080p".to_string()),
        ..Default::default()
    });
    // no download is started: the record just has to exist and persist
    assert!(mgr.get(id).is_some());
    mgr.shutdown();
    let saved = std::fs::read_to_string(root.join("app").join("downloads.json")).unwrap();
    assert!(
        saved.contains("\"quality_key\":\"1080p\""),
        "quality key persisted: {saved}"
    );
    assert!(
        saved.contains("\"source_key\":\"1080p|MP4\""),
        "source key persisted: {saved}"
    );
    assert!(
        saved.contains("watch?v=AAAAAAAAAAA"),
        "source page persisted: {saved}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
