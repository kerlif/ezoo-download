//! Verify a published SHA256SUMS file's Ed25519 signature with the exact
//! logic the updater uses (`selfupdate::sums_signature_ok`).
//!
//!     cargo run -p swiftget-engine --example verify_sums -- <version> <sums-file> <sig-file>
//!
//! Exit 0 when the signature verifies against the public key built into
//! the program (the primary key or the offline emergency key); exit 1
//! when it does not, when a file is missing or unreadable, or when the
//! signature file is malformed. Usage errors exit 2.
//!
//! The release workflow's publish-updater-manifest job runs this on the
//! SHA256SUMS-windows.txt + .sig downloaded from the release BEFORE it
//! signs anything (P8-sexies): a tampered sums file can never bless a
//! tampered Setup.exe, because the signature is checked first and the
//! Setup.exe hash is then compared against the verified sums file.
//!
//! For tests ONLY, `UPDATE_VERIFY_KEY=<64 hex public key>` overrides
//! the built-in keys, so fixtures can be signed with a throwaway
//! keypair. The workflow never sets it.

use std::fs;
use swiftget_engine::selfupdate::{sums_signature_ok, sums_signature_ok_with};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(version), Some(sums_path), Some(sig_path)) = (args.get(1), args.get(2), args.get(3))
    else {
        eprintln!("usage: verify_sums <version> <sums-file> <sig-file>");
        std::process::exit(2);
    };
    let sums = match fs::read_to_string(sums_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read sums file {sums_path}: {e}");
            std::process::exit(1);
        }
    };
    let sig = match fs::read_to_string(sig_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read signature file {sig_path}: {e}");
            std::process::exit(1);
        }
    };
    let ok = match std::env::var("UPDATE_VERIFY_KEY") {
        Ok(k) if !k.trim().is_empty() => {
            sums_signature_ok_with(version, &sums, sig.trim(), &[k.trim()])
        }
        _ => sums_signature_ok(version, &sums, sig.trim()),
    };
    if ok {
        println!("sums signature OK (version {version})");
    } else {
        eprintln!("sums signature FAILED (version {version}): refusing to trust this sums file");
        std::process::exit(1);
    }
}
