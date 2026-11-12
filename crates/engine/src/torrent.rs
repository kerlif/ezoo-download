//! Torrents (.torrent files and magnet links).
//!
//! The sharing itself is done by aria2 (`aria2c.exe`, a small, well known
//! open-source download program that ships with Ezoo Download Manager in the tools
//! folder). It runs hidden, only while Ezoo Download Manager runs, and only answers
//! on this computer (127.0.0.1, with a random password per start). Ezoo
//! Download adds, watches, stops and removes the torrents through its
//! JSON-RPC interface and shows them like every other download.
//!
//! By default nothing is shared (seeded) after a torrent is complete; the
//! user can choose minutes of sharing in Options > Torrents.

use anyhow::{anyhow, bail, Context};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

// ------------------------------------------------------------------ bencode

/// A value of a .torrent file (bencode).
#[derive(Debug, Clone, PartialEq)]
enum Ben {
    Int(i64),
    Bytes(Vec<u8>),
    List(Vec<Ben>),
    Dict(Vec<(Vec<u8>, Ben)>),
}

impl Ben {
    fn get(&self, key: &str) -> Option<&Ben> {
        match self {
            Ben::Dict(d) => d.iter().find(|(k, _)| k == key.as_bytes()).map(|(_, v)| v),
            _ => None,
        }
    }
    fn int(&self) -> Option<i64> {
        match self {
            Ben::Int(i) => Some(*i),
            _ => None,
        }
    }
    fn text(&self) -> Option<String> {
        match self {
            Ben::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        }
    }
    fn list(&self) -> &[Ben] {
        match self {
            Ben::List(l) => l,
            _ => &[],
        }
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    depth: u32,
}

impl Reader<'_> {
    fn peek(&self) -> anyhow::Result<u8> {
        self.data
            .get(self.pos)
            .copied()
            .ok_or_else(|| anyhow!("the torrent file is cut short"))
    }
    fn number_until(&mut self, end: u8) -> anyhow::Result<i64> {
        let start = self.pos;
        while self.peek()? != end {
            self.pos += 1;
            if self.pos - start > 20 {
                bail!("not a torrent file");
            }
        }
        let s = std::str::from_utf8(&self.data[start..self.pos])
            .map_err(|_| anyhow!("not a torrent file"))?;
        self.pos += 1;
        s.parse().map_err(|_| anyhow!("not a torrent file"))
    }
    fn value(&mut self) -> anyhow::Result<Ben> {
        self.depth += 1;
        if self.depth > 64 {
            bail!("not a torrent file");
        }
        let v = match self.peek()? {
            b'i' => {
                self.pos += 1;
                Ben::Int(self.number_until(b'e')?)
            }
            b'l' => {
                self.pos += 1;
                let mut list = Vec::new();
                while self.peek()? != b'e' {
                    list.push(self.value()?);
                }
                self.pos += 1;
                Ben::List(list)
            }
            b'd' => {
                self.pos += 1;
                let mut dict = Vec::new();
                while self.peek()? != b'e' {
                    let Ben::Bytes(k) = self.value()? else {
                        bail!("not a torrent file")
                    };
                    let v = self.value()?;
                    dict.push((k, v));
                }
                self.pos += 1;
                Ben::Dict(dict)
            }
            b'0'..=b'9' => {
                let len = usize::try_from(self.number_until(b':')?)
                    .map_err(|_| anyhow!("not a torrent file"))?;
                let end = self
                    .pos
                    .checked_add(len)
                    .filter(|e| *e <= self.data.len())
                    .ok_or_else(|| anyhow!("the torrent file is cut short"))?;
                let b = self.data[self.pos..end].to_vec();
                self.pos = end;
                Ben::Bytes(b)
            }
            _ => bail!("not a torrent file"),
        };
        self.depth -= 1;
        Ok(v)
    }
}

/// One file inside a torrent.
#[derive(Debug, Clone, PartialEq)]
pub struct TorrentFile {
    /// path inside the torrent's folder, with "/" between the parts
    pub path: String,
    pub size: u64,
    /// its number in the torrent, 1-based, counting the padding files the
    /// list leaves out (what "select-file" wants)
    pub index: usize,
}

/// What a .torrent file shares.
#[derive(Debug, Clone, PartialEq)]
pub struct TorrentInfo {
    /// the folder (several files) or file name
    pub name: String,
    pub files: Vec<TorrentFile>,
    pub total: u64,
    /// the torrent says it is private (BEP 27): only its own trackers
    /// may be told about it — never the public ones, never DHT/PEX
    pub private: bool,
}

/// Read a .torrent file.
pub fn parse_torrent(data: &[u8]) -> anyhow::Result<TorrentInfo> {
    let root = Reader {
        data,
        pos: 0,
        depth: 0,
    }
    .value()?;
    let info = root
        .get("info")
        .ok_or_else(|| anyhow!("not a torrent file"))?;
    let name = info
        .get("name.utf-8")
        .or_else(|| info.get("name"))
        .and_then(Ben::text)
        .unwrap_or_else(|| "torrent".into());
    let mut files = Vec::new();
    if let Some(list) = info.get("files") {
        for (n, f) in list.list().iter().enumerate() {
            let parts = f
                .get("path.utf-8")
                .or_else(|| f.get("path"))
                .map(Ben::list)
                .unwrap_or(&[]);
            let path: Vec<String> = parts.iter().filter_map(Ben::text).collect();
            // padding files (BEP 47) are not real files
            let pad = f
                .get("attr")
                .and_then(Ben::text)
                .map(|a| a.contains('p'))
                .unwrap_or(false)
                || path.first().map(|p| p.starts_with(".pad")).unwrap_or(false);
            if pad {
                continue;
            }
            files.push(TorrentFile {
                path: path.join("/"),
                size: f.get("length").and_then(Ben::int).unwrap_or(0).max(0) as u64,
                index: n + 1,
            });
        }
    } else {
        files.push(TorrentFile {
            path: name.clone(),
            size: info.get("length").and_then(Ben::int).unwrap_or(0).max(0) as u64,
            index: 1,
        });
    }
    if files.is_empty() {
        bail!("this torrent has no files");
    }
    let total = files.iter().map(|f| f.size).sum();
    let private = info.get("private").and_then(Ben::int) == Some(1);
    Ok(TorrentInfo {
        name,
        files,
        total,
        private,
    })
}

/// A magnet link of a torrent ("magnet:?xt=urn:btih:...").
pub fn is_magnet(s: &str) -> bool {
    let s = s.trim().to_ascii_lowercase();
    // a real one is a few hundred characters
    s.len() <= 8192
        && !s.contains(char::is_whitespace)
        && s.starts_with("magnet:?")
        && (s.contains("xt=urn:btih:") || s.contains("xt=urn:btmh:"))
}

/// Is this download a torrent: a magnet link or a .torrent file on the disk.
pub fn is_torrent_source(s: &str) -> bool {
    is_magnet(s) || (!s.contains("://") && s.to_ascii_lowercase().ends_with(".torrent"))
}

/// The name a magnet link gives ("dn="), if any.
pub fn magnet_name(uri: &str) -> Option<String> {
    let query = uri.trim().split_once('?')?.1;
    query
        .split('&')
        .find_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            (k == "dn").then(|| {
                percent_encoding::percent_decode_str(&v.replace('+', " "))
                    .decode_utf8_lossy()
                    .trim()
                    .to_string()
            })
        })
        .filter(|n| !n.is_empty())
}

/// A magnet link, parsed: the torrent's hash, its name, and the trackers
/// it names. `parse_magnet` is strict — a link it rejects is not a
/// magnet link the program can use (see the tests for what is refused).
#[derive(Debug, Clone, PartialEq)]
pub struct Magnet {
    /// btih infohash, lowercase hex (40 chars); `None` for btmh-only links
    pub infohash: Option<String>,
    /// the btmh value (multihash), if the link uses v2 hashes
    pub btmh: Option<String>,
    /// the name from `dn=`, if any
    pub name: Option<String>,
    /// the trackers from `tr=`, percent-decoded, in order, no repeats
    pub trackers: Vec<String>,
}

fn decode_query(v: &str) -> String {
    percent_encoding::percent_decode_str(&v.replace('+', " "))
        .decode_utf8_lossy()
        .into_owned()
}

/// Base32 (RFC 4648, case-insensitive, padding optional) to bytes.
fn base32_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut bits: u32 = 0;
    let mut nbits = 0;
    for c in s.chars() {
        if c == '=' {
            break;
        }
        let u = c.to_ascii_uppercase();
        let v = match u {
            'A'..='Z' => u as u32 - 'A' as u32,
            '2'..='7' => u as u32 - '2' as u32 + 26,
            _ => return None,
        };
        bits = (bits << 5) | v;
        nbits += 5;
        if nbits >= 8 {
            nbits -= 8;
            out.push((bits >> nbits) as u8);
            // keep only the bits not used yet
            bits &= (1u32 << nbits) - 1;
        }
    }
    // what is left over (fewer than 5 bits) must be zero
    if nbits > 0 && bits & ((1u32 << nbits) - 1) != 0 {
        return None;
    }
    Some(out)
}

/// Parse a magnet link strictly: `xt=urn:btih:` with a 40-hex or
/// 32-base32 hash, or `xt=urn:btmh:` with a multihash. Anything else —
/// no hash, a short or mistyped hash, a hash of another kind, a space
/// in the link — is refused with a plain reason.
pub fn parse_magnet(uri: &str) -> anyhow::Result<Magnet> {
    let uri = uri.trim();
    if uri.contains(char::is_whitespace) || uri.len() > 8192 {
        bail!("not a magnet link");
    }
    // the scheme is case-insensitive, but names and tracker addresses
    // are not: match keys on a lowercased copy, decode values from the
    // original
    let Some(query) = uri
        .split_once('?')
        .map(|(_, q)| q)
        .filter(|_| uri[..uri.find('?').unwrap_or(0)].eq_ignore_ascii_case("magnet:"))
    else {
        bail!("not a magnet link");
    };
    let mut m = Magnet {
        infohash: None,
        btmh: None,
        name: None,
        trackers: Vec::new(),
    };
    let mut saw_xt = false;
    for kv in query.split('&') {
        let Some((k, v)) = kv.split_once('=') else {
            continue;
        };
        match k.to_ascii_lowercase().as_str() {
            "xt" => {
                if saw_xt {
                    continue; // the first hash wins
                }
                saw_xt = true;
                // the hash itself is case-insensitive
                let v = v.to_ascii_lowercase();
                if let Some(hash) = v.strip_prefix("urn:btih:") {
                    if hash.len() == 40 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
                        m.infohash = Some(hash.to_string());
                    } else if hash.len() == 32 {
                        let bytes =
                            base32_decode(hash)
                                .filter(|b| b.len() == 20)
                                .ok_or_else(|| {
                                    anyhow!("the magnet link's hash is not a valid infohash")
                                })?;
                        m.infohash = Some(bytes.iter().map(|b| format!("{b:02x}")).collect());
                    } else {
                        bail!("the magnet link's hash is not a valid infohash");
                    }
                } else if let Some(hash) = v.strip_prefix("urn:btmh:") {
                    if hash.is_empty()
                        || !hash.chars().all(|c| c.is_ascii_alphanumeric() || c == '=')
                    {
                        bail!("the magnet link's hash is not a valid infohash");
                    }
                    m.btmh = Some(hash.to_string());
                } else {
                    bail!("the magnet link's hash is not a btih or btmh hash");
                }
            }
            "dn" if m.name.is_none() => {
                let name = decode_query(v);
                let name = name.trim().to_string();
                if !name.is_empty() {
                    m.name = Some(name);
                }
            }
            "tr" => {
                let tr = decode_query(v);
                let tr = tr.trim().to_string();
                if !tr.is_empty() && !m.trackers.contains(&tr) && m.trackers.len() < 64 {
                    m.trackers.push(tr);
                }
            }
            _ => {}
        }
    }
    if !saw_xt || (m.infohash.is_none() && m.btmh.is_none()) {
        bail!("the magnet link has no torrent hash");
    }
    Ok(m)
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

// --------------------------------------------------------------------- aria2

struct Daemon {
    port: u16,
    secret: String,
    child: std::process::Child,
    /// the 0600 config file that carries the RPC secret; deleted on drop
    conf: PathBuf,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        // Best-effort: the secret must not outlive the daemon it gates.
        let _ = std::fs::remove_file(&self.conf);
    }
}

/// The hidden aria2 that shares the torrents (started when first needed).
pub struct Aria2 {
    client: reqwest::Client,
    daemon: tokio::sync::Mutex<Option<Daemon>>,
    /// folder for aria2's own files (the DHT list)
    data_dir: PathBuf,
    /// public trackers for known-public torrents (see `extra_trackers`)
    trackers: Vec<String>,
}

/// The state of one torrent, as aria2 reports it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TorrentStatus {
    /// "active", "waiting", "paused", "error", "complete", "removed"
    pub status: String,
    pub total: u64,
    pub done: u64,
    pub speed: u64,
    pub upload_speed: u64,
    pub connections: u32,
    pub seeders: u32,
    /// all chosen files are here (it may still share them)
    pub seeding: bool,
    /// the torrent's name, once known (magnet links learn it first)
    pub name: Option<String>,
    /// a magnet link got its file list: the torrent itself goes on here
    pub followed_by: Option<String>,
    pub error: Option<String>,
}

fn num(v: &Value) -> u64 {
    v.as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| v.as_u64())
        .unwrap_or(0)
}

/// Public trackers added to a torrent that is not private (the
/// torrent's own come first). Long-running, open ones that answer most
/// torrents. Never announced to for a private torrent — see Q56: a
/// live test (2026-11-05, aria2 1.37.0) proved aria2 announces to
/// `--bt-tracker` trackers for a private torrent too, so the fork
/// passes them per download, only when the torrent is known public,
/// and never for a magnet link whose file list has not come yet.
const PUBLIC_TRACKERS: &[&str] = &[
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://open.stealth.si:80/announce",
    "udp://tracker.torrent.eu.org:451/announce",
    "udp://exodus.desync.com:6969/announce",
    "udp://explodie.org:6969/announce",
    "udp://open.demonii.com:1337/announce",
    "udp://tracker.dler.org:6969/announce",
    "udp://tracker.tiny-vps.com:6969/announce",
    "udp://tracker.openbittorrent.com:6969/announce",
    "https://tracker.tamersunion.org:443/announce",
    "http://tracker.opentrackr.org:1337/announce",
];

/// The extra trackers for one download, from `trackers`: the public
/// ones, but only when the torrent is known public (`Some(false)`).
/// A private torrent (`Some(true)`) and an unknown one (`None` — a
/// magnet link before its file list came) get none: announcing a
/// private torrent to public trackers would tell strangers about it.
fn extra_trackers(private: Option<bool>, trackers: &[String]) -> Option<String> {
    if private == Some(false) && !trackers.is_empty() {
        Some(trackers.join(","))
    } else {
        None
    }
}

/// Options for one torrent.
#[derive(Debug, Clone, Default)]
pub struct AddOptions {
    pub dir: PathBuf,
    /// files to get, 1-based ("1,3-5"); `None` = all
    pub select: Option<String>,
    /// minutes of sharing after it is complete (0 = none)
    pub seed_minutes: u32,
    /// KB/s, 0 = no limit
    pub download_limit_kb: u64,
    pub upload_limit_kb: u64,
    /// check the pieces already on the disk first
    pub check: bool,
    /// the torrent says it is private (`Some(true)`), it is public
    /// (`Some(false)`), or nobody knows yet (`None` — a magnet link).
    /// Only a known-public torrent gets the public trackers.
    pub private: Option<bool>,
}

/// Write the RPC secret to a 0600 config file the daemon reads at start
/// (`--conf-path`). The file lives in the app's data dir next to the DHT
/// list and is deleted when the daemon stops (Drop for Daemon).
/// Owner-only on Unix: the mode is set explicitly after the write,
/// because OpenOptions::mode applies only at creation and a
/// pre-existing file would keep its old permissions.
fn write_rpc_conf(dir: &Path, secret: &str) -> std::io::Result<PathBuf> {
    use std::io::Write;
    let path = dir.join("aria2-rpc.conf");
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&path)?;
    writeln!(f, "rpc-secret={secret}")?;
    drop(f);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(path)
}

/// The aria2c command line for one daemon. Pure (no process is spawned),
/// so the argument vector can be unit-tested: RPC is on, but loopback-only
/// (`--rpc-listen-all=false`, `--rpc-allow-origin-all=false`) and gated by
/// the random per-run secret, on an ephemeral port, and the daemon dies
/// with the app (`--stop-with-process`). The secret is passed through a
/// 0600 config file (`--conf-path`, see write_rpc_conf), never on the
/// command line: argv is visible to every local process (ps), so a
/// secret in argv is a secret shared with the whole machine.
fn aria2_args(port: u16, conf: &Path, dht: &Path, pid: u32) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--enable-rpc=true".into(),
        "--rpc-listen-all=false".into(),
        "--rpc-allow-origin-all=false".into(),
        "--rpc-max-request-size=32M".into(),
        "--continue=true".into(),
        "--file-allocation=none".into(),
        "--seed-time=0".into(),
        "--bt-save-metadata=false".into(),
        "--follow-torrent=mem".into(),
        "--follow-metalink=false".into(),
        "--auto-file-renaming=false".into(),
        "--allow-overwrite=false".into(),
        "--max-concurrent-downloads=64".into(),
        "--enable-dht=true".into(),
        "--bt-enable-lpd=true".into(),
        "--enable-peer-exchange=true".into(),
        "--bt-max-peers=150".into(),
        // below this speed aria2 keeps looking for more people (its own
        // 50K made a torrent stay slow with the first few it found)
        "--bt-request-peer-speed-limit=20M".into(),
        "--console-log-level=error".into(),
        "--summary-interval=0".into(),
        "--quiet=true".into(),
        "--check-certificate=true".into(),
        "--disk-cache=32M".into(),
    ];
    args.push(format!("--rpc-listen-port={port}"));
    args.push(format!("--conf-path={}", conf.display()));
    args.push(format!("--stop-with-process={pid}"));
    args.push(format!("--dht-file-path={}", dht.display()));
    args
}

impl Aria2 {
    pub fn new(data_dir: &Path) -> Aria2 {
        Self::with_trackers(
            data_dir,
            PUBLIC_TRACKERS.iter().map(|s| s.to_string()).collect(),
        )
    }

    /// Same, with an explicit tracker list (the live tests point it at
    /// a loopback tracker instead of the public ones).
    pub fn with_trackers(data_dir: &Path, trackers: Vec<String>) -> Aria2 {
        // straight to 127.0.0.1, never through a proxy; title-case
        // headers like the HTTP client (same reason: servers that
        // fingerprint lowercase header names)
        let client = reqwest::Client::builder()
            .no_proxy()
            .http1_title_case_headers()
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap_or_default();
        Aria2 {
            client,
            daemon: tokio::sync::Mutex::new(None),
            data_dir: data_dir.to_path_buf(),
            trackers,
        }
    }

    async fn ensure(&self) -> anyhow::Result<(u16, String)> {
        let mut guard = self.daemon.lock().await;
        if let Some(d) = guard.as_mut() {
            if matches!(d.child.try_wait(), Ok(None)) {
                return Ok((d.port, d.secret.clone()));
            }
        }
        let tool = crate::mux::find_tool("aria2c").ok_or_else(|| {
            // Absent by construction (`bundled = false`): the selector
            // always yields a message for this case.
            let msg = crate::tools::aria2_missing_message(
                cfg!(windows),
                crate::tools::in_flatpak(),
                false,
            )
            .unwrap_or("aria2c is missing");
            anyhow!("{msg}")
        })?;
        let port = std::net::TcpListener::bind("127.0.0.1:0")?
            .local_addr()?
            .port();
        let secret = random_secret();
        let _ = std::fs::create_dir_all(&self.data_dir);
        let dht = self.data_dir.join("dht.dat");
        let conf = write_rpc_conf(&self.data_dir, &secret)?;
        let mut cmd = std::process::Command::new(&tool);
        cmd.args(aria2_args(port, &conf, &dht, std::process::id()))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        crate::mux::hide_window(&mut cmd);
        let child = cmd.spawn().context("could not start aria2c")?;
        *guard = Some(Daemon {
            port,
            secret: secret.clone(),
            child,
            conf,
        });
        drop(guard);
        // ready when it answers
        for _ in 0..50 {
            if self
                .raw_call(port, &secret, "aria2.getVersion", vec![])
                .await
                .is_ok()
            {
                return Ok((port, secret));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        bail!("aria2c did not start")
    }

    async fn raw_call(
        &self,
        port: u16,
        secret: &str,
        method: &str,
        mut params: Vec<Value>,
    ) -> anyhow::Result<Value> {
        params.insert(0, json!(format!("token:{secret}")));
        let body = json!({ "jsonrpc": "2.0", "id": "ezoo", "method": method, "params": params });
        let bytes = self
            .client
            .post(format!("http://127.0.0.1:{port}/jsonrpc"))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(serde_json::to_vec(&body)?)
            .send()
            .await?
            .bytes()
            .await?;
        let r: Value =
            serde_json::from_slice(&bytes).context("aria2 answered something unexpected")?;
        if let Some(e) = r.get("error") {
            bail!("{}", e["message"].as_str().unwrap_or("aria2 error"));
        }
        Ok(r["result"].clone())
    }

    async fn call(&self, method: &str, params: Vec<Value>) -> anyhow::Result<Value> {
        let (port, secret) = self.ensure().await?;
        self.raw_call(port, &secret, method, params).await
    }

    /// Start a torrent; gives aria2's id for it.
    pub async fn add(&self, source: &str, o: &AddOptions) -> anyhow::Result<String> {
        // a magnet link nobody can read is refused at once, plainly —
        // not left as a download that never starts
        let mut private = o.private;
        let mut file_data: Option<Vec<u8>> = None;
        if is_magnet(source) {
            parse_magnet(source)?;
        } else {
            let data = std::fs::read(source).context("the .torrent file can't be read")?;
            let info = parse_torrent(&data)?;
            // the file itself says whether it is private, unless the
            // caller already said so
            private = private.or(Some(info.private));
            file_data = Some(data);
        }
        let mut opts = json!({
            "dir": o.dir.to_string_lossy(),
            "seed-time": o.seed_minutes.to_string(),
            "max-download-limit": format!("{}K", o.download_limit_kb),
            "max-upload-limit": format!("{}K", o.upload_limit_kb),
            "bt-remove-unselected-file": "true",
            "pause-metadata": "false",
        });
        if o.check {
            opts["check-integrity"] = json!("true");
        }
        if let Some(sel) = o.select.as_deref().filter(|s| !s.is_empty()) {
            opts["select-file"] = json!(sel);
        }
        // the public trackers only for a torrent known to be public
        // (Q56: aria2 announces to them for a private torrent too)
        if let Some(trackers) = extra_trackers(private, &self.trackers) {
            opts["bt-tracker"] = json!(trackers);
            opts["bt-tracker-connect-timeout"] = json!("10");
            opts["bt-tracker-timeout"] = json!("20");
        }
        let gid = if is_magnet(source) {
            self.call("aria2.addUri", vec![json!([source.trim()]), opts])
                .await?
        } else {
            let data = file_data.expect("a .torrent file was read above");
            self.call(
                "aria2.addTorrent",
                vec![json!(base64(&data)), json!([]), opts],
            )
            .await?
        };
        gid.as_str()
            .map(String::from)
            .ok_or_else(|| anyhow!("aria2 did not take the torrent"))
    }

    pub async fn status(&self, gid: &str) -> anyhow::Result<TorrentStatus> {
        let keys = json!([
            "status",
            "totalLength",
            "completedLength",
            "downloadSpeed",
            "uploadSpeed",
            "connections",
            "numSeeders",
            "seeder",
            "bittorrent",
            "followedBy",
            "errorMessage",
            "errorCode"
        ]);
        let r = self
            .call("aria2.tellStatus", vec![json!(gid), keys])
            .await?;
        let name = r["bittorrent"]["info"]["name"]
            .as_str()
            .map(String::from)
            .filter(|n| !n.is_empty());
        Ok(TorrentStatus {
            status: r["status"].as_str().unwrap_or("").to_string(),
            total: num(&r["totalLength"]),
            done: num(&r["completedLength"]),
            speed: num(&r["downloadSpeed"]),
            upload_speed: num(&r["uploadSpeed"]),
            connections: num(&r["connections"]) as u32,
            seeders: num(&r["numSeeders"]) as u32,
            seeding: r["seeder"].as_str() == Some("true"),
            name,
            followed_by: r["followedBy"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|g| g.as_str())
                .map(String::from),
            error: r["errorMessage"]
                .as_str()
                .filter(|m| !m.is_empty())
                .map(String::from),
        })
    }

    /// The file list of a magnet link before it is downloaded: only the
    /// torrent's own description is fetched from the people who share
    /// it, into `dir` (a .torrent file, given back). Gives up after
    /// `wait` — a link nobody shares fails plainly instead of hanging
    /// the download. No public trackers are passed here: nobody knows
    /// yet whether the torrent is private (Q56).
    pub async fn metadata(
        &self,
        magnet: &str,
        dir: &Path,
        wait: Duration,
    ) -> anyhow::Result<PathBuf> {
        parse_magnet(magnet)?;
        std::fs::create_dir_all(dir)?;
        let opts = json!({
            "dir": dir.to_string_lossy(),
            "bt-metadata-only": "true",
            "bt-save-metadata": "true",
            "follow-torrent": "false",
            "seed-time": "0",
        });
        let gid = self
            .call("aria2.addUri", vec![json!([magnet.trim()]), opts])
            .await?;
        let gid = gid
            .as_str()
            .map(String::from)
            .ok_or_else(|| anyhow!("aria2 did not take the link"))?;
        let started = std::time::Instant::now();
        let result = loop {
            if started.elapsed() > wait {
                break Err(anyhow!("nobody sent the file list in time"));
            }
            tokio::time::sleep(Duration::from_millis(700)).await;
            let st = match self.status(&gid).await {
                Ok(s) => s,
                Err(e) => break Err(e),
            };
            match st.status.as_str() {
                "complete" => {
                    let found = std::fs::read_dir(dir)?
                        .filter_map(|e| e.ok())
                        .map(|e| e.path())
                        .find(|p| {
                            p.extension()
                                .is_some_and(|x| x.eq_ignore_ascii_case("torrent"))
                        });
                    break found.ok_or_else(|| anyhow!("the file list was not saved"));
                }
                "error" | "removed" => {
                    break Err(anyhow!(
                        "{}",
                        st.error
                            .unwrap_or_else(|| "the file list could not be fetched".into())
                    ));
                }
                _ => {}
            }
        };
        self.remove(&gid).await;
        result
    }

    /// Stop it now (what is here stays, it goes on from there next time).
    pub async fn remove(&self, gid: &str) {
        if self.daemon.lock().await.is_none() {
            return;
        }
        let _ = self.call("aria2.forceRemove", vec![json!(gid)]).await;
        // forget it, so the same torrent can be added again
        for _ in 0..20 {
            if self
                .call("aria2.removeDownloadResult", vec![json!(gid)])
                .await
                .is_ok()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// The files of a torrent once aria2 knows them, with how much of each
    /// is here.
    pub async fn files(&self, gid: &str) -> anyhow::Result<Vec<FileState>> {
        let r = self.call("aria2.getFiles", vec![json!(gid)]).await?;
        Ok(r.as_array()
            .map(|a| {
                a.iter()
                    .map(|f| FileState {
                        path: PathBuf::from(f["path"].as_str().unwrap_or("")),
                        size: num(&f["length"]),
                        done: num(&f["completedLength"]),
                        selected: f["selected"].as_str() == Some("true"),
                    })
                    .collect()
            })
            .unwrap_or_default())
    }
}

/// One file of a running torrent.
#[derive(Debug, Clone, PartialEq)]
pub struct FileState {
    pub path: PathBuf,
    pub size: u64,
    pub done: u64,
    /// one of the files the user wants (the others only get the bits of
    /// pieces they share with those)
    pub selected: bool,
}

impl Drop for Aria2 {
    fn drop(&mut self) {
        if let Some(d) = self.daemon.get_mut().as_mut() {
            let _ = d.child.kill();
        }
    }
}

fn random_secret() -> String {
    use ring::rand::SecureRandom;
    let mut b = [0u8; 16];
    if ring::rand::SystemRandom::new().fill(&mut b).is_err() {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        b = (t ^ u128::from(std::process::id())).to_le_bytes();
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ben_str(s: &str) -> String {
        format!("{}:{s}", s.len())
    }

    #[test]
    fn reads_a_multi_file_torrent() {
        let t = format!(
            "d8:announce{}4:infod5:filesld6:lengthi100e4:pathl{}{}eed6:lengthi5e4:pathl{}eed4:attr1:p6:lengthi7e4:pathl{}eee4:name{}12:piece lengthi16384e6:pieces0:ee",
            ben_str("http://t/a"),
            ben_str("Season 1"),
            ben_str("ep1.mkv"),
            ben_str("readme.txt"),
            ben_str(".pad"),
            ben_str("My Show")
        );
        let i = parse_torrent(t.as_bytes()).unwrap();
        assert_eq!(i.name, "My Show");
        assert_eq!(
            i.files,
            vec![
                TorrentFile {
                    path: "Season 1/ep1.mkv".into(),
                    size: 100,
                    index: 1
                },
                TorrentFile {
                    path: "readme.txt".into(),
                    size: 5,
                    index: 2
                },
            ]
        );
        assert_eq!(i.total, 105);
    }

    #[test]
    fn reads_a_single_file_torrent_and_refuses_junk() {
        let t = format!("d4:infod6:lengthi42e4:name{}ee", ben_str("film.mp4"));
        let i = parse_torrent(t.as_bytes()).unwrap();
        assert_eq!(
            i.files,
            vec![TorrentFile {
                path: "film.mp4".into(),
                size: 42,
                index: 1
            }]
        );
        assert!(parse_torrent(b"<html>").is_err());
        assert!(parse_torrent(b"d4:infod6:lengthi42e4:name99:x").is_err());
        assert!(parse_torrent(&b"l".repeat(200)).is_err());
    }

    #[test]
    fn magnet_links() {
        let m = "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=Big+Film%20(2024)&tr=udp%3A%2F%2Ft";
        assert!(is_magnet(m));
        assert!(is_torrent_source(m));
        assert!(is_torrent_source(r"C:\Users\a\Downloads\x.torrent"));
        assert!(!is_torrent_source("https://site/x.torrent"));
        assert!(!is_magnet("magnet:?xt=urn:sha1:abc"));
        assert_eq!(magnet_name(m).as_deref(), Some("Big Film (2024)"));
        assert_eq!(magnet_name("magnet:?xt=urn:btih:00"), None);
    }

    #[test]
    fn magnet_parsing() {
        let hex = "0123456789abcdef0123456789abcdef01234567";
        let m = parse_magnet(&format!(
            "magnet:?xt=urn:btih:{hex}&dn=Big+Film%20(2024)&tr=udp%3A%2F%2Ftracker.example%3A1337%2Fannounce&tr=udp%3A%2F%2Ftracker.example%3A1337%2Fannounce"
        ))
        .unwrap();
        assert_eq!(m.infohash.as_deref(), Some(hex));
        assert_eq!(m.btmh, None);
        assert_eq!(m.name.as_deref(), Some("Big Film (2024)"));
        assert_eq!(
            m.trackers,
            vec!["udp://tracker.example:1337/announce".to_string()]
        );
        // uppercase hash and scheme: the same torrent
        let up = parse_magnet(&format!(
            "MAGNET:?xt=urn:btih:{}&dn=UPPER+Name",
            hex.to_ascii_uppercase()
        ))
        .unwrap();
        assert_eq!(up.infohash.as_deref(), Some(hex));
        assert_eq!(up.name.as_deref(), Some("UPPER Name"));
        // base32 hash (32 chars) is the same 20 bytes, hex-encoded
        let bytes: Vec<u8> = (0..20).collect();
        let b32 = {
            const T: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
            let mut s = String::new();
            let mut bits: u32 = 0;
            let mut n = 0;
            for b in &bytes {
                bits = (bits << 8) | u32::from(*b);
                n += 8;
                while n >= 5 {
                    n -= 5;
                    s.push(T[(bits >> n) as usize & 31] as char);
                    bits &= (1u32 << n) - 1;
                }
            }
            if n > 0 {
                s.push(T[(bits << (5 - n)) as usize & 31] as char);
            }
            s
        };
        let m32 = parse_magnet(&format!("magnet:?xt=urn:btih:{b32}")).unwrap();
        assert_eq!(
            m32.infohash.as_deref(),
            Some("000102030405060708090a0b0c0d0e0f10111213")
        );
        // the same hash in lowercase decodes to the same bytes
        let m32low =
            parse_magnet(&format!("magnet:?xt=urn:btih:{}", b32.to_ascii_lowercase())).unwrap();
        assert_eq!(m32low.infohash, m32.infohash);
        // a btmh link parses, with no btih hash
        let mh = parse_magnet(
            "magnet:?xt=urn:btmh:1220e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855&dn=Film",
        )
        .unwrap();
        assert_eq!(mh.infohash, None);
        assert!(mh.btmh.is_some());
        assert_eq!(mh.name.as_deref(), Some("Film"));
    }

    #[test]
    fn magnet_parsing_refuses_malformed_links() {
        // no hash at all
        assert!(parse_magnet("magnet:?dn=Film").is_err());
        assert!(parse_magnet("magnet:?").is_err());
        // not a magnet link
        assert!(parse_magnet("https://example.com/x.torrent").is_err());
        assert!(parse_magnet("").is_err());
        // a space in the link
        assert!(parse_magnet(
            "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=Big Film"
        )
        .is_err());
        // a hash of another kind
        assert!(
            parse_magnet("magnet:?xt=urn:sha1:0123456789abcdef0123456789abcdef01234567").is_err()
        );
        // too short / mistyped / wrong-length hash
        assert!(parse_magnet("magnet:?xt=urn:btih:00").is_err());
        assert!(
            parse_magnet("magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef0123456").is_err()
        );
        assert!(
            parse_magnet("magnet:?xt=urn:btih:zz23456789abcdef0123456789abcdef01234567").is_err()
        );
        assert!(
            parse_magnet("magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef0123456g").is_err()
        );
        // base32 of the wrong length
        assert!(parse_magnet("magnet:?xt=urn:btih:MFRGGZDFMZTWQ2LK").is_err());
        // an empty btmh hash
        assert!(parse_magnet("magnet:?xt=urn:btmh:").is_err());
    }

    #[test]
    fn private_flag_is_read_from_the_torrent() {
        // private=1, keys in canonical order
        let t = format!(
            "d8:announce{}4:infod6:lengthi42e4:name{}12:piece lengthi16384e6:pieces0:7:privatei1eee",
            ben_str("http://t/a"),
            ben_str("film.mp4")
        );
        let i = parse_torrent(t.as_bytes()).unwrap();
        assert!(i.private);
        let t2 = format!(
            "d8:announce{}4:infod6:lengthi42e4:name{}12:piece lengthi16384e6:pieces0:ee",
            ben_str("http://t/a"),
            ben_str("film.mp4")
        );
        assert!(!parse_torrent(t2.as_bytes()).unwrap().private);
    }

    #[test]
    fn public_trackers_only_for_known_public_torrents() {
        let list = vec!["udp://tracker.example:1337/announce".to_string()];
        // public: the trackers go
        assert_eq!(
            extra_trackers(Some(false), &list).as_deref(),
            Some("udp://tracker.example:1337/announce")
        );
        // private: nothing goes — announcing it would tell strangers
        assert_eq!(extra_trackers(Some(true), &list), None);
        // unknown (a magnet link before its file list came): nothing goes
        assert_eq!(extra_trackers(None, &list), None);
        // no trackers configured: nothing to pass even for a public torrent
        assert_eq!(extra_trackers(Some(false), &[]), None);
    }

    #[test]
    fn base64_like_everyone() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn aria2_rpc_is_loopback_only_with_a_secret() {
        let args = aria2_args(
            45678,
            Path::new("/tmp/x/aria2-rpc.conf"),
            Path::new("/tmp/x/dht.dat"),
            1234,
        );
        // RPC is on — the app drives aria2 over it — but loopback-only
        // and origin-locked, so no other machine or web page can reach it
        assert!(args.iter().any(|a| a == "--enable-rpc=true"));
        assert!(args.iter().any(|a| a == "--rpc-listen-all=false"));
        assert!(args.iter().any(|a| a == "--rpc-allow-origin-all=false"));
        assert!(args.iter().any(|a| a == "--rpc-listen-port=45678"));
        // the secret gates every call and reaches the daemon through a
        // 0600 config file, never on the command line (argv is visible
        // to every local process)
        assert!(args
            .iter()
            .any(|a| a == "--conf-path=/tmp/x/aria2-rpc.conf"));
        assert!(!args.iter().any(|a| a.starts_with("--rpc-secret")));
        // the daemon dies with the app
        assert!(args.iter().any(|a| a == "--stop-with-process=1234"));
        assert!(args.iter().any(|a| a == "--dht-file-path=/tmp/x/dht.dat"));
        // looks harder for people (P5), but never announces to public
        // trackers from here: those go per download, only for a torrent
        // known to be public (Q56 — aria2 leaks them for private ones)
        assert!(args.iter().any(|a| a == "--bt-max-peers=150"));
        assert!(args
            .iter()
            .any(|a| a == "--bt-request-peer-speed-limit=20M"));
        assert!(!args.iter().any(|a| a.starts_with("--bt-tracker=")));
        // never the open variants
        assert!(!args.iter().any(|a| a == "--rpc-listen-all=true"));
        assert!(!args.iter().any(|a| a == "--rpc-allow-origin-all=true"));
        assert!(!args.iter().any(|a| a == "--enable-rpc=false"));
    }

    #[test]
    fn rpc_secret_is_random_per_run() {
        let a = random_secret();
        let b = random_secret();
        assert_ne!(a, b);
        // 16 random bytes, hex-encoded
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn rpc_conf_file_holds_the_secret_owner_only() {
        let dir = std::env::temp_dir().join(format!("ezoo-rpc-conf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_rpc_conf(&dir, "deadbeefdeadbeefdeadbeefdeadbeef").unwrap();
        assert_eq!(path, dir.join("aria2-rpc.conf"));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "rpc-secret=deadbeefdeadbeefdeadbeefdeadbeef\n"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
