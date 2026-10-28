//! Update checker against a LOCAL test server only (127.0.0.1, ephemeral
//! port, std TcpListener): no live api.github.com call is ever made.
//! Covers the Phase 7 gate fixtures: (a) offline, (b) older version,
//! (c) newer version with a valid signature, (d) bad hash, (e) bad
//! signature, (f) malformed JSON, (g) rate limit, (h) cross-host
//! redirect — plus ETag/304 caching, the checksum-file requirement,
//! and reuse of an already-downloaded verified copy. Signing fixtures
//! use throwaway keys generated at test runtime, never a real key.

use reqwest::{redirect::Policy, Client};
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::json;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use swiftget_engine::selfupdate::{
    check_latest_with_key, current, download_artifact, sha256_hex, signed_message, CheckOutcome,
    UPDATE_PUBLIC_KEY,
};

// ---------------------------------------------------------------------------
// Canned local server.

struct Req {
    path: String,
    headers: Vec<(String, String)>,
}

struct Resp {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

#[derive(Default)]
struct Stats {
    hits: HashMap<String, usize>,
    last_headers: HashMap<String, Vec<(String, String)>>,
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        302 => "Found",
        304 => "Not Modified",
        403 => "Forbidden",
        404 => "Not Found",
        429 => "Too Many Requests",
        _ => "Status",
    }
}

fn handle_conn(
    mut stream: std::net::TcpStream,
    handler: &Arc<dyn Fn(&Req) -> Resp + Send + Sync>,
    stats: &Arc<Mutex<Stats>>,
) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() || request_line.is_empty() {
        return;
    }
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_string();
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {
                if line == "\r\n" || line == "\n" {
                    break;
                }
                if let Some((k, v)) = line.trim_end().split_once(':') {
                    headers.push((k.trim().to_string(), v.trim().to_string()));
                }
            }
        }
    }
    let req = Req { path, headers };
    let resp = handler(&req);
    {
        let mut s = stats.lock().unwrap();
        *s.hits.entry(req.path.clone()).or_insert(0) += 1;
        s.last_headers.insert(req.path.clone(), req.headers.clone());
    }
    let mut out = format!("HTTP/1.1 {} {}\r\n", resp.status, reason(resp.status));
    for (k, v) in &resp.headers {
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    out.push_str(&format!("Content-Length: {}\r\n", resp.body.len()));
    out.push_str("Connection: close\r\n\r\n");
    let _ = stream.write_all(out.as_bytes());
    let _ = stream.write_all(&resp.body);
}

fn serve<F>(handler: F) -> (u16, Arc<Mutex<Stats>>)
where
    F: Fn(&Req) -> Resp + Send + Sync + 'static,
{
    let handler: Arc<dyn Fn(&Req) -> Resp + Send + Sync> = Arc::new(handler);
    let stats = Arc::new(Mutex::new(Stats::default()));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (h2, s2) = (handler.clone(), stats.clone());
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            match conn {
                Ok(stream) => {
                    let (h3, s3) = (h2.clone(), s2.clone());
                    std::thread::spawn(move || handle_conn(stream, &h3, &s3));
                }
                Err(_) => break,
            }
        }
    });
    (port, stats)
}

fn checker_client() -> Client {
    Client::builder()
        .redirect(Policy::none())
        .no_proxy()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
}

fn header<'a>(req: &'a Req, name: &str) -> Option<&'a str> {
    req.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

// ---------------------------------------------------------------------------
// Fixtures.

fn test_key(seed: u8) -> (String, Ed25519KeyPair) {
    let kp = Ed25519KeyPair::from_seed_unchecked(&[seed; 32]).unwrap();
    let pubhex: String = kp
        .public_key()
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    (pubhex, kp)
}

fn sign(kp: &Ed25519KeyPair, version: &str, sha: &str) -> String {
    kp.sign(signed_message(version, sha).as_bytes())
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn payload(base: &str, tag: &str, assets: &[(&str, &str)]) -> String {
    let assets: Vec<_> = assets
        .iter()
        .map(|(name, path)| json!({"name": name, "browser_download_url": format!("{base}{path}")}))
        .collect();
    json!({
        "tag_name": tag,
        "html_url": format!("{base}/releases"),
        "assets": assets,
    })
    .to_string()
}

fn manifest(version: &str, sha: &str, sig: &str) -> String {
    json!({"version": version, "sha256": sha, "sig": sig}).to_string()
}

fn sums(entries: &[(&str, &str)]) -> String {
    entries
        .iter()
        .map(|(hash, name)| format!("{hash}  {name}\n"))
        .collect()
}

struct Fixture {
    sha: String,
    sig: String,
    body: Vec<u8>,
}

fn good_fixture() -> Fixture {
    let (_pubkey, kp) = test_key(11);
    let body = b"fake-setup-bytes".to_vec();
    let sha = sha256_hex(&body);
    let sig = sign(&kp, "99.0.0", &sha);
    Fixture { sha, sig, body }
}

fn tmpdir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ezoo-uc-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

async fn run_check(
    client: &Client,
    base: &str,
    dir: &std::path::Path,
    pubkey: &str,
) -> CheckOutcome {
    check_latest_with_key(
        client,
        &format!("{base}/latest"),
        &dir.join("checker-cache.json"),
        current(),
        pubkey,
    )
    .await
}

// ---------------------------------------------------------------------------
// (a) offline.

#[tokio::test]
async fn offline_connection_refused_is_a_silent_fail() {
    let port = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let dir = tmpdir("offline");
    let outcome = run_check(
        &checker_client(),
        &format!("http://127.0.0.1:{port}"),
        &dir,
        UPDATE_PUBLIC_KEY,
    )
    .await;
    assert!(matches!(
        outcome,
        CheckOutcome::SilentFail {
            rate_limited: false,
            retry_after_secs: None
        }
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// (b) older version: up to date, manifest never fetched.

#[tokio::test]
async fn older_release_is_up_to_date_and_manifest_is_not_fetched() {
    let (port, stats) = {
        let base_holder = Arc::new(Mutex::new(String::new()));
        let (p, s) = serve({
            let base_holder = base_holder.clone();
            move |req| match req.path.as_str() {
                "/latest" => Resp {
                    status: 200,
                    headers: vec![],
                    body: payload(
                        &base_holder.lock().unwrap(),
                        "v0.0.1",
                        &[("version.json", "/version.json")],
                    )
                    .into_bytes(),
                },
                _ => Resp {
                    status: 404,
                    headers: vec![],
                    body: vec![],
                },
            }
        });
        *base_holder.lock().unwrap() = format!("http://127.0.0.1:{p}");
        (p, s)
    };
    let dir = tmpdir("older");
    let outcome = run_check(
        &checker_client(),
        &format!("http://127.0.0.1:{port}"),
        &dir,
        UPDATE_PUBLIC_KEY,
    )
    .await;
    assert!(matches!(outcome, CheckOutcome::UpToDate));
    assert_eq!(stats.lock().unwrap().hits.get("/version.json"), None);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// (c) newer version, valid signature: offered, with the app User-Agent.

#[tokio::test]
async fn newer_release_with_valid_signature_is_offered() {
    let fx = good_fixture();
    let (port, stats) = {
        let base_holder = Arc::new(Mutex::new(String::new()));
        let fx = Arc::new(fx);
        let (p, s) = serve({
            let base_holder = base_holder.clone();
            let fx = fx.clone();
            move |req| {
                let base = base_holder.lock().unwrap().clone();
                match req.path.as_str() {
                    "/latest" => Resp {
                        status: 200,
                        headers: vec![("ETag".into(), "\"v99\"".into())],
                        body: payload(
                            &base,
                            "v99.0.0",
                            &[
                                ("version.json", "/version.json"),
                                ("SHA256SUMS.txt", "/sums.txt"),
                                ("EzooDownload-Setup.exe", "/setup.exe"),
                            ],
                        )
                        .into_bytes(),
                    },
                    "/version.json" => Resp {
                        status: 200,
                        headers: vec![],
                        body: manifest("99.0.0", &fx.sha, &fx.sig).into_bytes(),
                    },
                    "/sums.txt" => Resp {
                        status: 200,
                        headers: vec![],
                        body: sums(&[(&fx.sha, "EzooDownload-Setup.exe")]).into_bytes(),
                    },
                    _ => Resp {
                        status: 404,
                        headers: vec![],
                        body: vec![],
                    },
                }
            }
        });
        *base_holder.lock().unwrap() = format!("http://127.0.0.1:{p}");
        (p, s)
    };
    let dir = tmpdir("newer");
    let outcome = run_check(
        &checker_client(),
        &format!("http://127.0.0.1:{port}"),
        &dir,
        &good_fixture_pubkey(),
    )
    .await;
    // the fixture keypair is regenerated identically (seed 11), so the
    // public key matches the manifest signature
    let rel = match outcome {
        CheckOutcome::Available(rel) => rel,
        other => panic!("expected Available, got {other:?}"),
    };
    assert_eq!(rel.version, "99.0.0");
    assert_eq!(rel.tag, "v99.0.0");
    assert_eq!(rel.manifest.sha256, rel_sums_sha(&rel.sums_text));
    assert!(rel.setup_asset_url().ends_with("/setup.exe"));
    let headers = stats
        .lock()
        .unwrap()
        .last_headers
        .get("/latest")
        .cloned()
        .unwrap();
    let ua = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("user-agent"))
        .map(|(_, v)| v.clone())
        .unwrap();
    assert_eq!(ua, format!("EzooDownload/{}", current()));
    let _ = std::fs::remove_dir_all(&dir);
}

fn good_fixture_pubkey() -> String {
    test_key(11).0
}

fn rel_sums_sha(sums_text: &str) -> String {
    swiftget_engine::selfupdate::sums_hash(sums_text, "EzooDownload-Setup.exe").unwrap()
}

// ---------------------------------------------------------------------------
// (d) bad hash: refused at download, and refused by the sums cross-check.

#[tokio::test]
async fn tampered_artifact_bytes_are_refused() {
    let fx = good_fixture();
    let (port, _stats) = {
        let base_holder = Arc::new(Mutex::new(String::new()));
        let fx = Arc::new(fx);
        let (p, s) = serve({
            let base_holder = base_holder.clone();
            let fx = fx.clone();
            move |req| {
                let base = base_holder.lock().unwrap().clone();
                match req.path.as_str() {
                    "/latest" => Resp {
                        status: 200,
                        headers: vec![],
                        body: payload(
                            &base,
                            "v99.0.0",
                            &[
                                ("version.json", "/version.json"),
                                ("SHA256SUMS.txt", "/sums.txt"),
                                ("EzooDownload-Setup.exe", "/setup.exe"),
                            ],
                        )
                        .into_bytes(),
                    },
                    "/version.json" => Resp {
                        status: 200,
                        headers: vec![],
                        body: manifest("99.0.0", &fx.sha, &fx.sig).into_bytes(),
                    },
                    "/sums.txt" => Resp {
                        status: 200,
                        headers: vec![],
                        body: sums(&[(&fx.sha, "EzooDownload-Setup.exe")]).into_bytes(),
                    },
                    "/setup.exe" => Resp {
                        status: 200,
                        headers: vec![],
                        body: b"tampered-bytes".to_vec(),
                    },
                    _ => Resp {
                        status: 404,
                        headers: vec![],
                        body: vec![],
                    },
                }
            }
        });
        *base_holder.lock().unwrap() = format!("http://127.0.0.1:{p}");
        (p, s)
    };
    let dir = tmpdir("badhash");
    let client = checker_client();
    let outcome = run_check(
        &client,
        &format!("http://127.0.0.1:{port}"),
        &dir,
        &good_fixture_pubkey(),
    )
    .await;
    let rel = match outcome {
        CheckOutcome::Available(rel) => rel,
        other => panic!("expected Available, got {other:?}"),
    };
    let err = download_artifact(
        &client,
        &rel.setup_asset_url(),
        "EzooDownload-Setup.exe",
        &rel.manifest.sha256,
        &rel.sums_text,
        &dir,
        &|_, _| {},
    )
    .await
    .unwrap_err();
    assert!(err.is_verification(), "{err}");
    assert!(!dir.join("EzooDownload-Setup.exe").exists());
    assert!(!dir.join("EzooDownload-Setup.download").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn sums_entry_that_disagrees_with_the_manifest_is_refused() {
    let fx = good_fixture();
    let other_sha = sha256_hex(b"something-else");
    let (port, stats) = {
        let base_holder = Arc::new(Mutex::new(String::new()));
        let fx = Arc::new(fx);
        let (p, s) = serve({
            let base_holder = base_holder.clone();
            let fx = fx.clone();
            let other_sha = other_sha.clone();
            move |req| {
                let base = base_holder.lock().unwrap().clone();
                match req.path.as_str() {
                    "/latest" => Resp {
                        status: 200,
                        headers: vec![],
                        body: payload(
                            &base,
                            "v99.0.0",
                            &[
                                ("version.json", "/version.json"),
                                ("SHA256SUMS.txt", "/sums.txt"),
                                ("EzooDownload-Setup.exe", "/setup.exe"),
                            ],
                        )
                        .into_bytes(),
                    },
                    "/version.json" => Resp {
                        status: 200,
                        headers: vec![],
                        body: manifest("99.0.0", &fx.sha, &fx.sig).into_bytes(),
                    },
                    "/sums.txt" => Resp {
                        status: 200,
                        headers: vec![],
                        body: sums(&[(&other_sha, "EzooDownload-Setup.exe")]).into_bytes(),
                    },
                    "/setup.exe" => Resp {
                        status: 200,
                        headers: vec![],
                        body: fx.body.clone(),
                    },
                    _ => Resp {
                        status: 404,
                        headers: vec![],
                        body: vec![],
                    },
                }
            }
        });
        *base_holder.lock().unwrap() = format!("http://127.0.0.1:{p}");
        (p, s)
    };
    let dir = tmpdir("badsum");
    let client = checker_client();
    let outcome = run_check(
        &client,
        &format!("http://127.0.0.1:{port}"),
        &dir,
        &good_fixture_pubkey(),
    )
    .await;
    let rel = match outcome {
        CheckOutcome::Available(rel) => rel,
        other => panic!("expected Available, got {other:?}"),
    };
    let err = download_artifact(
        &client,
        &rel.setup_asset_url(),
        "EzooDownload-Setup.exe",
        &rel.manifest.sha256,
        &rel.sums_text,
        &dir,
        &|_, _| {},
    )
    .await
    .unwrap_err();
    assert!(err.is_verification(), "{err}");
    // refused before any fetch: the artifact was never requested
    assert_eq!(stats.lock().unwrap().hits.get("/setup.exe"), None);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// (e) bad signature: never offered.

#[tokio::test]
async fn wrong_key_signature_and_missing_signature_are_never_offered() {
    for (name, make_sig) in [
        ("wrongkey", {
            let (_p, other) = test_key(77);
            let fx = good_fixture();
            sign(&other, "99.0.0", &fx.sha)
        }),
        ("emptysig", String::new()),
    ] {
        let fx = good_fixture();
        let (port, _stats) = {
            let base_holder = Arc::new(Mutex::new(String::new()));
            let fx = Arc::new(fx);
            let sig = make_sig.clone();
            let (p, s) = serve({
                let base_holder = base_holder.clone();
                let fx = fx.clone();
                move |req| {
                    let base = base_holder.lock().unwrap().clone();
                    match req.path.as_str() {
                        "/latest" => Resp {
                            status: 200,
                            headers: vec![],
                            body: payload(
                                &base,
                                "v99.0.0",
                                &[
                                    ("version.json", "/version.json"),
                                    ("SHA256SUMS.txt", "/sums.txt"),
                                ],
                            )
                            .into_bytes(),
                        },
                        "/version.json" => Resp {
                            status: 200,
                            headers: vec![],
                            body: manifest("99.0.0", &fx.sha, &sig).into_bytes(),
                        },
                        "/sums.txt" => Resp {
                            status: 200,
                            headers: vec![],
                            body: sums(&[(&fx.sha, "EzooDownload-Setup.exe")]).into_bytes(),
                        },
                        _ => Resp {
                            status: 404,
                            headers: vec![],
                            body: vec![],
                        },
                    }
                }
            });
            *base_holder.lock().unwrap() = format!("http://127.0.0.1:{p}");
            (p, s)
        };
        let dir = tmpdir(&format!("badsig-{name}"));
        let outcome = run_check(
            &checker_client(),
            &format!("http://127.0.0.1:{port}"),
            &dir,
            &good_fixture_pubkey(),
        )
        .await;
        assert!(
            matches!(outcome, CheckOutcome::SilentFail { .. }),
            "{name}: expected SilentFail, got {outcome:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ---------------------------------------------------------------------------
// (f) malformed answers: silent fail.

#[tokio::test]
async fn malformed_answers_fail_silently() {
    // malformed API JSON
    let (port, _) = serve(|req| match req.path.as_str() {
        "/latest" => Resp {
            status: 200,
            headers: vec![],
            body: b"{not json".to_vec(),
        },
        _ => Resp {
            status: 404,
            headers: vec![],
            body: vec![],
        },
    });
    let dir = tmpdir("malformed-api");
    assert!(matches!(
        run_check(
            &checker_client(),
            &format!("http://127.0.0.1:{port}"),
            &dir,
            UPDATE_PUBLIC_KEY
        )
        .await,
        CheckOutcome::SilentFail { .. }
    ));
    let _ = std::fs::remove_dir_all(&dir);

    // malformed tag
    let (port, _) = {
        let base_holder = Arc::new(Mutex::new(String::new()));
        let (p, s) = serve({
            let base_holder = base_holder.clone();
            move |req| match req.path.as_str() {
                "/latest" => Resp {
                    status: 200,
                    headers: vec![],
                    body: payload(&base_holder.lock().unwrap(), "banana", &[]).into_bytes(),
                },
                _ => Resp {
                    status: 404,
                    headers: vec![],
                    body: vec![],
                },
            }
        });
        *base_holder.lock().unwrap() = format!("http://127.0.0.1:{p}");
        (p, s)
    };
    let dir = tmpdir("malformed-tag");
    assert!(matches!(
        run_check(
            &checker_client(),
            &format!("http://127.0.0.1:{port}"),
            &dir,
            UPDATE_PUBLIC_KEY
        )
        .await,
        CheckOutcome::SilentFail { .. }
    ));
    let _ = std::fs::remove_dir_all(&dir);

    // malformed version.json
    let (port, _) = {
        let base_holder = Arc::new(Mutex::new(String::new()));
        let (p, s) = serve({
            let base_holder = base_holder.clone();
            move |req| match req.path.as_str() {
                "/latest" => Resp {
                    status: 200,
                    headers: vec![],
                    body: payload(
                        &base_holder.lock().unwrap(),
                        "v99.0.0",
                        &[("version.json", "/version.json")],
                    )
                    .into_bytes(),
                },
                "/version.json" => Resp {
                    status: 200,
                    headers: vec![],
                    body: b"{\"version\":".to_vec(),
                },
                _ => Resp {
                    status: 404,
                    headers: vec![],
                    body: vec![],
                },
            }
        });
        *base_holder.lock().unwrap() = format!("http://127.0.0.1:{p}");
        (p, s)
    };
    let dir = tmpdir("malformed-manifest");
    assert!(matches!(
        run_check(
            &checker_client(),
            &format!("http://127.0.0.1:{port}"),
            &dir,
            UPDATE_PUBLIC_KEY
        )
        .await,
        CheckOutcome::SilentFail { .. }
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// (g) rate limit: silent fail with a backoff hint, exactly one request.

#[tokio::test]
async fn rate_limit_fails_silently_once_with_a_backoff_hint() {
    let (port, stats) = serve(|req| match req.path.as_str() {
        "/latest" => Resp {
            status: 403,
            headers: vec![
                ("X-RateLimit-Remaining".into(), "0".into()),
                ("Retry-After".into(), "120".into()),
            ],
            body: b"{\"message\":\"API rate limit exceeded\"}".to_vec(),
        },
        _ => Resp {
            status: 404,
            headers: vec![],
            body: vec![],
        },
    });
    let dir = tmpdir("ratelimit");
    let outcome = run_check(
        &checker_client(),
        &format!("http://127.0.0.1:{port}"),
        &dir,
        UPDATE_PUBLIC_KEY,
    )
    .await;
    assert!(matches!(
        outcome,
        CheckOutcome::SilentFail {
            rate_limited: true,
            retry_after_secs: Some(120)
        }
    ));
    assert_eq!(stats.lock().unwrap().hits.get("/latest"), Some(&1));
    let _ = std::fs::remove_dir_all(&dir);

    // 429 with a reset time instead of Retry-After
    let reset = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 300;
    let (port, stats) = serve(move |req| match req.path.as_str() {
        "/latest" => Resp {
            status: 429,
            headers: vec![("X-RateLimit-Reset".into(), reset.to_string())],
            body: vec![],
        },
        _ => Resp {
            status: 404,
            headers: vec![],
            body: vec![],
        },
    });
    let dir = tmpdir("ratelimit2");
    let outcome = run_check(
        &checker_client(),
        &format!("http://127.0.0.1:{port}"),
        &dir,
        UPDATE_PUBLIC_KEY,
    )
    .await;
    match outcome {
        CheckOutcome::SilentFail {
            rate_limited: true,
            retry_after_secs: Some(secs),
        } => assert!((240..=300).contains(&secs), "secs={secs}"),
        other => panic!("expected rate-limited SilentFail, got {other:?}"),
    }
    assert_eq!(stats.lock().unwrap().hits.get("/latest"), Some(&1));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// (h) redirects: cross-host refused, same-origin followed.

#[tokio::test]
async fn cross_host_redirect_is_rejected_and_never_followed() {
    // the "other host": a second local server that must never be hit
    let (other_port, other_stats) = serve(|_| Resp {
        status: 200,
        headers: vec![],
        body: b"{}".to_vec(),
    });
    let (port, _) = serve(move |req| match req.path.as_str() {
        "/latest" => Resp {
            status: 302,
            headers: vec![(
                "Location".into(),
                format!("http://127.0.0.1:{other_port}/latest"),
            )],
            body: vec![],
        },
        _ => Resp {
            status: 404,
            headers: vec![],
            body: vec![],
        },
    });
    let dir = tmpdir("redirect-cross");
    let outcome = run_check(
        &checker_client(),
        &format!("http://127.0.0.1:{port}"),
        &dir,
        UPDATE_PUBLIC_KEY,
    )
    .await;
    assert!(matches!(outcome, CheckOutcome::SilentFail { .. }));
    assert_eq!(other_stats.lock().unwrap().hits.len(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn same_origin_redirect_is_followed() {
    let (port, stats) = serve(|req| match req.path.as_str() {
        "/latest" => Resp {
            status: 302,
            headers: vec![("Location".into(), "/real".into())],
            body: vec![],
        },
        "/real" => Resp {
            status: 200,
            headers: vec![],
            body: payload("http://127.0.0.1:0", "v0.0.1", &[]).into_bytes(),
        },
        _ => Resp {
            status: 404,
            headers: vec![],
            body: vec![],
        },
    });
    let dir = tmpdir("redirect-same");
    let outcome = run_check(
        &checker_client(),
        &format!("http://127.0.0.1:{port}"),
        &dir,
        UPDATE_PUBLIC_KEY,
    )
    .await;
    assert!(matches!(outcome, CheckOutcome::UpToDate));
    assert_eq!(stats.lock().unwrap().hits.get("/real"), Some(&1));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// ETag caching: 304 reuses the cached payload; the header is only sent
// when a cached ETag exists.

#[tokio::test]
async fn etag_cache_is_used_on_304() {
    let fx = good_fixture();
    let (port, stats) = {
        let base_holder = Arc::new(Mutex::new(String::new()));
        let fx = Arc::new(fx);
        let (p, s) = serve({
            let base_holder = base_holder.clone();
            let fx = fx.clone();
            move |req| {
                let base = base_holder.lock().unwrap().clone();
                match req.path.as_str() {
                    "/latest" => {
                        if header(req, "if-none-match") == Some("\"v99\"") {
                            Resp {
                                status: 304,
                                headers: vec![],
                                body: vec![],
                            }
                        } else {
                            Resp {
                                status: 200,
                                headers: vec![("ETag".into(), "\"v99\"".into())],
                                body: payload(
                                    &base,
                                    "v99.0.0",
                                    &[
                                        ("version.json", "/version.json"),
                                        ("SHA256SUMS.txt", "/sums.txt"),
                                    ],
                                )
                                .into_bytes(),
                            }
                        }
                    }
                    "/version.json" => Resp {
                        status: 200,
                        headers: vec![],
                        body: manifest("99.0.0", &fx.sha, &fx.sig).into_bytes(),
                    },
                    "/sums.txt" => Resp {
                        status: 200,
                        headers: vec![],
                        body: sums(&[(&fx.sha, "EzooDownload-Setup.exe")]).into_bytes(),
                    },
                    _ => Resp {
                        status: 404,
                        headers: vec![],
                        body: vec![],
                    },
                }
            }
        });
        *base_holder.lock().unwrap() = format!("http://127.0.0.1:{p}");
        (p, s)
    };
    let dir = tmpdir("etag");
    let client = checker_client();
    let base = format!("http://127.0.0.1:{port}");
    let first = run_check(&client, &base, &dir, &good_fixture_pubkey()).await;
    assert!(matches!(first, CheckOutcome::Available(_)));
    // the first request carried no conditional header
    let first_headers = stats
        .lock()
        .unwrap()
        .last_headers
        .get("/latest")
        .cloned()
        .unwrap();
    assert!(
        first_headers
            .iter()
            .all(|(k, _)| !k.eq_ignore_ascii_case("if-none-match")),
        "first check must not send If-None-Match"
    );
    assert!(dir.join("checker-cache.json").exists());
    let second = run_check(&client, &base, &dir, &good_fixture_pubkey()).await;
    let rel = match second {
        CheckOutcome::Available(rel) => rel,
        other => panic!("expected cached Available, got {other:?}"),
    };
    assert_eq!(rel.version, "99.0.0");
    let second_headers = stats
        .lock()
        .unwrap()
        .last_headers
        .get("/latest")
        .cloned()
        .unwrap();
    assert_eq!(
        second_headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("if-none-match"))
            .map(|(_, v)| v.as_str()),
        Some("\"v99\"")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Missing manifest / missing checksum file: no offer.

#[tokio::test]
async fn missing_manifest_or_sums_is_a_silent_fail() {
    // no version.json asset at all
    let (port, _) = {
        let base_holder = Arc::new(Mutex::new(String::new()));
        let (p, s) = serve({
            let base_holder = base_holder.clone();
            move |req| match req.path.as_str() {
                "/latest" => Resp {
                    status: 200,
                    headers: vec![],
                    body: payload(
                        &base_holder.lock().unwrap(),
                        "v99.0.0",
                        &[("SHA256SUMS.txt", "/sums.txt")],
                    )
                    .into_bytes(),
                },
                _ => Resp {
                    status: 404,
                    headers: vec![],
                    body: vec![],
                },
            }
        });
        *base_holder.lock().unwrap() = format!("http://127.0.0.1:{p}");
        (p, s)
    };
    let dir = tmpdir("no-manifest");
    assert!(matches!(
        run_check(
            &checker_client(),
            &format!("http://127.0.0.1:{port}"),
            &dir,
            UPDATE_PUBLIC_KEY
        )
        .await,
        CheckOutcome::SilentFail { .. }
    ));
    let _ = std::fs::remove_dir_all(&dir);

    // manifest present and signed, but no checksum file asset
    let fx = good_fixture();
    let (port, _) = {
        let base_holder = Arc::new(Mutex::new(String::new()));
        let fx = Arc::new(fx);
        let (p, s) = serve({
            let base_holder = base_holder.clone();
            let fx = fx.clone();
            move |req| {
                let base = base_holder.lock().unwrap().clone();
                match req.path.as_str() {
                    "/latest" => Resp {
                        status: 200,
                        headers: vec![],
                        body: payload(&base, "v99.0.0", &[("version.json", "/version.json")])
                            .into_bytes(),
                    },
                    "/version.json" => Resp {
                        status: 200,
                        headers: vec![],
                        body: manifest("99.0.0", &fx.sha, &fx.sig).into_bytes(),
                    },
                    _ => Resp {
                        status: 404,
                        headers: vec![],
                        body: vec![],
                    },
                }
            }
        });
        *base_holder.lock().unwrap() = format!("http://127.0.0.1:{p}");
        (p, s)
    };
    let dir = tmpdir("no-sums");
    assert!(matches!(
        run_check(
            &checker_client(),
            &format!("http://127.0.0.1:{port}"),
            &dir,
            &good_fixture_pubkey()
        )
        .await,
        CheckOutcome::SilentFail { .. }
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// A verified download succeeds and is reused without re-fetching.

#[tokio::test]
async fn verified_download_succeeds_and_is_reused() {
    let fx = good_fixture();
    let (port, stats) = {
        let base_holder = Arc::new(Mutex::new(String::new()));
        let fx = Arc::new(fx);
        let (p, s) = serve({
            let base_holder = base_holder.clone();
            let fx = fx.clone();
            move |req| {
                let base = base_holder.lock().unwrap().clone();
                match req.path.as_str() {
                    "/latest" => Resp {
                        status: 200,
                        headers: vec![],
                        body: payload(
                            &base,
                            "v99.0.0",
                            &[
                                ("version.json", "/version.json"),
                                ("SHA256SUMS.txt", "/sums.txt"),
                                ("EzooDownload-Setup.exe", "/setup.exe"),
                            ],
                        )
                        .into_bytes(),
                    },
                    "/version.json" => Resp {
                        status: 200,
                        headers: vec![],
                        body: manifest("99.0.0", &fx.sha, &fx.sig).into_bytes(),
                    },
                    "/sums.txt" => Resp {
                        status: 200,
                        headers: vec![],
                        body: sums(&[(&fx.sha, "EzooDownload-Setup.exe")]).into_bytes(),
                    },
                    "/setup.exe" => Resp {
                        status: 200,
                        headers: vec![],
                        body: fx.body.clone(),
                    },
                    _ => Resp {
                        status: 404,
                        headers: vec![],
                        body: vec![],
                    },
                }
            }
        });
        *base_holder.lock().unwrap() = format!("http://127.0.0.1:{p}");
        (p, s)
    };
    let dir = tmpdir("reuse");
    let client = checker_client();
    let outcome = run_check(
        &client,
        &format!("http://127.0.0.1:{port}"),
        &dir,
        &good_fixture_pubkey(),
    )
    .await;
    let rel = match outcome {
        CheckOutcome::Available(rel) => rel,
        other => panic!("expected Available, got {other:?}"),
    };
    let url = rel.setup_asset_url();
    let sha = rel.manifest.sha256.clone();
    let sums = rel.sums_text.clone();
    let first = download_artifact(
        &client,
        &url,
        "EzooDownload-Setup.exe",
        &sha,
        &sums,
        &dir,
        &|_, _| {},
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(&first).unwrap(), b"fake-setup-bytes");
    let second = download_artifact(
        &client,
        &url,
        "EzooDownload-Setup.exe",
        &sha,
        &sums,
        &dir,
        &|_, _| {},
    )
    .await
    .unwrap();
    assert_eq!(first, second);
    assert_eq!(stats.lock().unwrap().hits.get("/setup.exe"), Some(&1));
    let _ = std::fs::remove_dir_all(&dir);
}
