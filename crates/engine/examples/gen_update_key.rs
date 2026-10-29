//! Local key-derivation utility ONLY. It generates no keys by itself:
//! it takes a 64-hex Ed25519 seed on the command line and prints the
//! matching public half, derived exactly the way the updater does
//! (`ring::signature::Ed25519KeyPair::from_seed_unchecked` over the
//! seed decoded by `selfupdate::from_hex`, public half as 64 hex
//! characters) — the same construction `sign_update` uses to check
//! `UPDATE_SIGNING_KEY` against the built-in public key.
//!
//!     cargo run -p swiftget-engine --example gen_update_key -- <64 hex seed>
//!
//! The seed itself NEVER enters the repository. The fork's public half
//! is committed inline as `UPDATE_PUBLIC_KEY` in
//! `crates/engine/src/selfupdate.rs` — that constant is the single
//! source of the public key; there is no key file under
//! `packaging/keys/`. The private half lives only as the repository
//! secret `UPDATE_SIGNING_KEY`.
//!
//! NEVER use this utility to overwrite the existing fork keypair: a
//! new keypair would make every installed copy refuse signed updates,
//! because their built-in public key would no longer match. This
//! example exists only to derive the public half from a seed that is
//! already held, e.g. to verify it against the committed constant.

use ring::signature::{Ed25519KeyPair, KeyPair};
use swiftget_engine::selfupdate::from_hex;

fn main() {
    let arg = std::env::args().nth(1);
    let seed = match arg.as_deref().map(from_hex) {
        Some(Some(s)) if s.len() == 32 => s,
        _ => {
            eprintln!("usage: gen_update_key <64 hex characters of randomness>");
            eprintln!("make some with: openssl rand -hex 32");
            std::process::exit(2);
        }
    };
    let kp = Ed25519KeyPair::from_seed_unchecked(&seed).expect("bad seed");
    let public: String = kp
        .public_key()
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    println!("public: {public}");
    println!(
        "(the seed you passed is the private key; store it as the CI secret UPDATE_SIGNING_KEY)"
    );
}
