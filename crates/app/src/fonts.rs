//! The fonts of the interface, embedded in the program and registered
//! in memory at startup, so the program never depends on system fonts:
//! Latin text in **Carlito**, Arabic text in **Noto Sans Arabic**, and
//! **Liberation Serif** for document-style text only. Sources and
//! licences: see `ui/fonts/FONTS-LICENSE.txt`.
//!
//! Registration uses Slint's shared fontique collection
//! (`slint::fontique_011`, feature `unstable-fontique-011` in
//! `crates/app/Cargo.toml`): the bundled fonts are registered from
//! their embedded bytes into the same collection Slint's text shaper
//! reads, and the Arabic and Latin scripts get explicit fallbacks, so
//! mixed Arabic/Latin text renders even on a system with no fonts
//! installed at all. Nothing is written to the user's font folder any
//! more; copies the old mechanism left behind are deleted instead
//! (see [`cleanup_legacy`]).

use std::path::PathBuf;

/// Latin text: Carlito, metric-compatible with Calibri (SIL OFL 1.1).
pub(crate) const CARLITO: &[u8] = include_bytes!("../ui/fonts/Carlito-Regular.ttf");
pub(crate) const CARLITO_BOLD: &[u8] = include_bytes!("../ui/fonts/Carlito-Bold.ttf");
/// Arabic text: Noto Sans Arabic, static instances (SIL OFL 1.1).
pub(crate) const NOTO_ARABIC: &[u8] = include_bytes!("../ui/fonts/NotoSansArabic-Regular.ttf");
pub(crate) const NOTO_ARABIC_BOLD: &[u8] = include_bytes!("../ui/fonts/NotoSansArabic-Bold.ttf");
/// Document-style text only: Liberation Serif, metric-compatible with
/// Times New Roman (SIL OFL 1.1). Never the UI default.
pub(crate) const LIBERATION_SERIF: &[u8] =
    include_bytes!("../ui/fonts/LiberationSerif-Regular.ttf");
pub(crate) const LIBERATION_SERIF_BOLD: &[u8] =
    include_bytes!("../ui/fonts/LiberationSerif-Bold.ttf");

/// Family names as the fonts themselves declare them (name-table ID 1).
pub(crate) const LATIN_FAMILY: &str = "Carlito";
pub(crate) const ARABIC_FAMILY: &str = "Noto Sans Arabic";
pub(crate) const SERIF_FAMILY: &str = "Liberation Serif";

/// A font choice saved under a name the program no longer ships moves to
/// its replacement; every other stored value passes through unchanged.
/// ("InstaArabi" shipped without a licence and was removed in v1.1.0;
/// "Times New Roman" was a renamed Liberation Serif and now keeps its
/// own name.)
pub(crate) fn migrate_font_family(stored: &str) -> &str {
    match stored {
        "InstaArabi" => ARABIC_FAMILY,
        "Times New Roman" => SERIF_FAMILY,
        other => other,
    }
}

/// What a stored font choice means on screen: legacy names map to their
/// replacements, and an empty or unknown choice means the bundled
/// default for the language (Arabic in Noto Sans Arabic, anything else
/// in Carlito) — never a system font.
pub(crate) fn resolve_font_family(stored: &str, lang: &str) -> String {
    let migrated = migrate_font_family(stored);
    if migrated.is_empty() || !crate::FONTS.contains(&migrated) {
        crate::default_family(lang).to_string()
    } else {
        migrated.to_string()
    }
}

/// Register the bundled fonts in memory and wire the script fallbacks.
/// Runs before any window opens. Registration failures are reported on
/// stderr and never panic: the UI then falls back to system fonts.
pub(crate) fn register() {
    use slint::fontique_011::fontique;

    let mut collection = slint::fontique_011::shared_collection();
    let mut families = 0;
    for data in [
        CARLITO,
        CARLITO_BOLD,
        NOTO_ARABIC,
        NOTO_ARABIC_BOLD,
        LIBERATION_SERIF,
        LIBERATION_SERIF_BOLD,
    ] {
        let blob = fontique::Blob::new(std::sync::Arc::new(data));
        families += collection.register_fonts(blob, None).len();
    }
    if families == 0 {
        eprintln!("ezoo: no bundled font registered; using system fonts");
        return;
    }
    wire_fallbacks(&mut collection);
}

/// Wire the script fallbacks and generic families on a collection the
/// bundled fonts were registered into. Split out of [`register`] so the
/// wiring can be tested on a fresh collection with no system fonts
/// (see `script_fallbacks_resolve_to_the_bundled_fonts`).
fn wire_fallbacks(collection: &mut slint::fontique_011::fontique::Collection) {
    use slint::fontique_011::fontique;

    // Noto Sans Arabic covers Arabic only (no Latin glyphs), and Carlito
    // covers Latin only: without explicit script fallbacks, mixed text
    // would lose one script wherever no system font covers it.
    let arabic = collection.family_id(ARABIC_FAMILY);
    let latin = collection.family_id(LATIN_FAMILY);
    if let Some(id) = arabic {
        let ok = collection.append_fallbacks(
            fontique::FallbackKey::new(fontique::Script::from_bytes(*b"Arab"), None),
            std::iter::once(id),
        );
        if !ok {
            eprintln!("ezoo: could not add the Arabic font fallback");
        }
    } else {
        eprintln!("ezoo: bundled Arabic font not found after registration");
    }
    if let Some(id) = latin {
        let ok = collection.append_fallbacks(
            fontique::FallbackKey::new(fontique::Script::from_bytes(*b"Latn"), None),
            std::iter::once(id),
        );
        if !ok {
            eprintln!("ezoo: could not add the Latin font fallback");
        }
    } else {
        eprintln!("ezoo: bundled Latin font not found after registration");
    }

    // Generic families back the "no family requested" stack Slint builds
    // (named family, SansSerif, SystemUi). fontique iterates the
    // registered families before the system ones for a generic family,
    // so (with no SLINT_DEFAULT_FONT / SLINT_FONT_PATH override) the
    // bundled fonts are consulted before any system font when the named
    // family has no glyph for a character. The script fallbacks above
    // are consulted after the whole stack: parley chains the stack
    // families, then the fallback families, and takes the first font
    // whose charmap covers the character.
    let mut bundled = Vec::new();
    if let Some(id) = latin {
        bundled.push(id);
    }
    if let Some(id) = arabic {
        bundled.push(id);
    }
    for generic in [
        fontique::GenericFamily::SansSerif,
        fontique::GenericFamily::SystemUi,
    ] {
        collection.append_generic_families(generic, bundled.iter().copied());
    }
}

/// Delete the font copies the old mechanism wrote to the per-user font
/// folder at first run. Only the three filenames this program used,
/// only in that folder; failures are ignored. The folder itself is
/// removed when it ends up empty. The OS font system drops the deleted
/// files on its next scan.
pub(crate) fn cleanup_legacy() {
    const LEGACY: [&str; 3] = [
        "InstaArabi-Regular.ttf",
        "TimesNewRoman-Regular.ttf",
        "TimesNewRoman-Bold.ttf",
    ];
    let Some(dir) = legacy_font_dir() else {
        return;
    };
    for name in LEGACY {
        let _ = std::fs::remove_file(dir.join(name));
    }
    let _ = std::fs::remove_dir(&dir);
    #[cfg(windows)]
    cleanup_legacy_registry();
}

#[cfg(target_os = "linux")]
fn legacy_font_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
        .map(|base| base.join("fonts").join("Ezoo"))
}

#[cfg(windows)]
fn legacy_font_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|base| base.join("Microsoft").join("Windows").join("Fonts"))
}

#[cfg(not(any(windows, target_os = "linux")))]
fn legacy_font_dir() -> Option<PathBuf> {
    None
}

#[cfg(windows)]
fn wide(s: String) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(&s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Delete the per-user registry values the old mechanism added for its
/// font copies ("InstaArabi (TrueType)", "TimesNewRoman (TrueType)").
/// Best-effort: a stale value pointing at a deleted file is harmless,
/// the font is simply unavailable.
#[cfg(windows)]
fn cleanup_legacy_registry() {
    let key = wide(r"Software\Microsoft\Windows NT\CurrentVersion\Fonts".to_string());
    unsafe {
        let mut hkey = std::ptr::null_mut();
        let open = windows_sys::Win32::System::Registry::RegOpenKeyExW(
            windows_sys::Win32::System::Registry::HKEY_CURRENT_USER,
            key.as_ptr(),
            0,
            windows_sys::Win32::System::Registry::KEY_SET_VALUE,
            &mut hkey,
        );
        if open != 0 {
            return;
        }
        for label in ["InstaArabi (TrueType)", "TimesNewRoman (TrueType)"] {
            let value = wide(label.to_string());
            windows_sys::Win32::System::Registry::RegDeleteValueW(hkey, value.as_ptr());
        }
        windows_sys::Win32::System::Registry::RegCloseKey(hkey);
    }
}

/// Register the bundled fonts and remove the old per-user copies.
pub(crate) fn setup() {
    register();
    cleanup_legacy();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_names_migrate() {
        assert_eq!(migrate_font_family("InstaArabi"), "Noto Sans Arabic");
        assert_eq!(migrate_font_family("Times New Roman"), "Liberation Serif");
        assert_eq!(migrate_font_family("Carlito"), "Carlito");
        assert_eq!(migrate_font_family("Tahoma"), "Tahoma");
        assert_eq!(migrate_font_family(""), "");
    }

    #[test]
    fn stored_choices_resolve() {
        assert_eq!(resolve_font_family("", "ar"), "Noto Sans Arabic");
        assert_eq!(resolve_font_family("", "en"), "Carlito");
        assert_eq!(resolve_font_family("InstaArabi", "fr"), "Noto Sans Arabic");
        assert_eq!(
            resolve_font_family("Times New Roman", "en"),
            "Liberation Serif"
        );
        assert_eq!(resolve_font_family("NoSuchFont", "ar"), "Noto Sans Arabic");
        assert_eq!(resolve_font_family("Tahoma", "ar"), "Tahoma");
        assert_eq!(
            resolve_font_family("Liberation Serif", "ar"),
            "Liberation Serif"
        );
    }

    #[test]
    fn defaults_never_return_a_system_only_family() {
        // An empty or unknown stored choice means the bundled default
        // for the language — never a font that only exists on the
        // system. An explicit dropdown choice is the user's own and
        // passes through unchanged (its Arabic glyphs still reach
        // Noto Sans Arabic through the Arab script fallback).
        for lang in ["ar", "en", "fr", "es", "de", "pt", "tr", "ru"] {
            let expected = if lang == "ar" {
                ARABIC_FAMILY
            } else {
                LATIN_FAMILY
            };
            assert_eq!(resolve_font_family("", lang), expected, "{lang}");
            assert_eq!(resolve_font_family("NoSuchFont", lang), expected, "{lang}");
            assert_eq!(crate::default_family(lang), expected, "{lang}");
        }
        assert_eq!(resolve_font_family("Tahoma", "ar"), "Tahoma");
    }

    /// The first font in a fontique query chain whose charmap covers
    /// `ch`, following exactly the selection parley performs: the
    /// requested families in order, then the script fallback families,
    /// first covering font wins.
    fn first_covering(
        collection: &mut slint::fontique_011::fontique::Collection,
        cache: &mut slint::fontique_011::fontique::SourceCache,
        families: &[slint::fontique_011::fontique::QueryFamily<'static>],
        script: [u8; 4],
        ch: char,
    ) -> Option<slint::fontique_011::fontique::FamilyId> {
        use slint::fontique_011::fontique;
        let mut query = collection.query(cache);
        query.set_families(families.iter().copied());
        query.set_fallbacks(fontique::FallbackKey::new(
            fontique::Script::from_bytes(script),
            None,
        ));
        let mut found = None;
        query.matches_with(|font| {
            let covers = font
                .charmap()
                .is_some_and(|cmap| cmap.map(ch).is_some_and(|glyph| glyph != 0));
            if covers {
                found = Some(font.family.0);
                fontique::QueryStatus::Stop
            } else {
                fontique::QueryStatus::Continue
            }
        });
        found
    }

    #[test]
    fn script_fallbacks_resolve_to_the_bundled_fonts() {
        use slint::fontique_011::fontique;

        // A fresh collection with no system fonts at all: fontconfig
        // is never consulted, so anything that resolves here resolves
        // from the bundled fonts alone. This is the API-level form of
        // the Phase 3 one-glyph substitute render test (see
        // docs/FONTS.md); it proves font selection, not shaping —
        // shaping is what the render test's screenshots show.
        let mut collection = fontique::Collection::new(fontique::CollectionOptions {
            shared: false,
            system_fonts: false,
        });
        for data in [
            CARLITO,
            CARLITO_BOLD,
            NOTO_ARABIC,
            NOTO_ARABIC_BOLD,
            LIBERATION_SERIF,
            LIBERATION_SERIF_BOLD,
        ] {
            let blob = fontique::Blob::new(std::sync::Arc::new(data));
            assert!(!collection.register_fonts(blob, None).is_empty());
        }
        wire_fallbacks(&mut collection);

        let arabic = collection
            .family_id(ARABIC_FAMILY)
            .expect("Noto Sans Arabic registered");
        let latin = collection
            .family_id(LATIN_FAMILY)
            .expect("Carlito registered");
        let serif = collection
            .family_id(SERIF_FAMILY)
            .expect("Liberation Serif registered");
        assert_ne!(arabic, latin);
        assert_ne!(arabic, serif);
        assert_ne!(latin, serif);

        // Fallback identity: the Arab script fallback is exactly the
        // Noto Sans Arabic family, the Latin fallback exactly Carlito.
        let arab_fallbacks: Vec<_> = collection
            .fallback_families(fontique::FallbackKey::new(
                fontique::Script::from_bytes(*b"Arab"),
                None,
            ))
            .collect();
        assert_eq!(arab_fallbacks, vec![arabic]);
        let latn_fallbacks: Vec<_> = collection
            .fallback_families(fontique::FallbackKey::new(
                fontique::Script::from_bytes(*b"Latn"),
                None,
            ))
            .collect();
        assert_eq!(latn_fallbacks, vec![latin]);

        // Generic order: the bundled families come before any system
        // family in the SansSerif / SystemUi lists, so a glyph missing
        // from the named family meets a bundled font first.
        let sans: Vec<_> = collection
            .generic_families(fontique::GenericFamily::SansSerif)
            .collect();
        assert_eq!(sans, vec![latin, arabic]);
        let system_ui: Vec<_> = collection
            .generic_families(fontique::GenericFamily::SystemUi)
            .collect();
        assert_eq!(system_ui, vec![latin, arabic]);

        let mut cache = fontique::SourceCache::new(fontique::SourceCacheOptions::default());
        let named = |id: fontique::FamilyId| vec![fontique::QueryFamily::Id(id)];
        // The named family alone, so only the script fallback can
        // supply the other script: Arabic under Carlito resolves to
        // Noto Sans Arabic, Latin under Noto Sans Arabic resolves to
        // Carlito, and Liberation Serif covers Latin itself.
        assert_eq!(
            first_covering(&mut collection, &mut cache, &named(latin), *b"Arab", 'ب'),
            Some(arabic)
        );
        assert_eq!(
            first_covering(&mut collection, &mut cache, &named(arabic), *b"Latn", 'A'),
            Some(latin)
        );
        assert_eq!(
            first_covering(&mut collection, &mut cache, &named(serif), *b"Latn", 'A'),
            Some(serif)
        );
        assert_eq!(
            first_covering(&mut collection, &mut cache, &named(serif), *b"Arab", 'ب'),
            Some(arabic)
        );
        // The full stack Slint builds — [named, SansSerif, SystemUi] —
        // with the same outcomes: no system font is ever reached.
        let stack = |id: fontique::FamilyId| {
            vec![
                fontique::QueryFamily::Id(id),
                fontique::QueryFamily::Generic(fontique::GenericFamily::SansSerif),
                fontique::QueryFamily::Generic(fontique::GenericFamily::SystemUi),
            ]
        };
        assert_eq!(
            first_covering(&mut collection, &mut cache, &stack(latin), *b"Arab", 'ب'),
            Some(arabic)
        );
        assert_eq!(
            first_covering(&mut collection, &mut cache, &stack(arabic), *b"Latn", 'A'),
            Some(latin)
        );
        assert_eq!(
            first_covering(&mut collection, &mut cache, &stack(arabic), *b"Arab", 'ب'),
            Some(arabic)
        );
        assert_eq!(
            first_covering(&mut collection, &mut cache, &stack(latin), *b"Latn", 'A'),
            Some(latin)
        );
    }

    /// A minimal TrueType reader, just enough to check the bundled
    /// fonts' name tables (family, licence) and weight class without
    /// any new dependency.
    struct FontInfo {
        names: Vec<(u16, String)>,
        weight: Option<u16>,
        has_fvar: bool,
    }

    fn be16(data: &[u8], at: usize) -> u16 {
        u16::from_be_bytes([data[at], data[at + 1]])
    }

    fn be32(data: &[u8], at: usize) -> u32 {
        u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
    }

    fn parse_font(data: &[u8]) -> FontInfo {
        let count = be16(data, 4) as usize;
        let mut tables: Vec<([u8; 4], usize, usize)> = Vec::new();
        for i in 0..count {
            let at = 12 + i * 16;
            let tag = [data[at], data[at + 1], data[at + 2], data[at + 3]];
            tables.push((
                tag,
                be32(data, at + 8) as usize,
                be32(data, at + 12) as usize,
            ));
        }
        let table = |tag: &[u8; 4]| {
            tables
                .iter()
                .find(|(t, _, _)| t == tag)
                .map(|(_, o, l)| (*o, *l))
        };
        let mut names = Vec::new();
        if let Some((off, _)) = table(b"name") {
            let records = be16(data, off + 2) as usize;
            let strings = off + be16(data, off + 4) as usize;
            for i in 0..records {
                let at = off + 6 + i * 12;
                let platform = be16(data, at);
                let name_id = be16(data, at + 6);
                let len = be16(data, at + 8) as usize;
                let start = strings + be16(data, at + 10) as usize;
                let raw = &data[start..start + len];
                let text = if platform == 3 {
                    char::decode_utf16(
                        raw.as_chunks::<2>()
                            .0
                            .iter()
                            .map(|c| u16::from_be_bytes([c[0], c[1]])),
                    )
                    .map(|r| r.unwrap_or('\u{fffd}'))
                    .collect()
                } else {
                    raw.iter().map(|b| *b as char).collect()
                };
                names.push((name_id, text));
            }
        }
        let weight = table(b"OS/2").map(|(off, _)| be16(data, off + 4));
        FontInfo {
            names,
            weight,
            has_fvar: table(b"fvar").is_some(),
        }
    }

    fn name(info: &FontInfo, id: u16) -> String {
        info.names
            .iter()
            .find(|(n, _)| *n == id)
            .map(|(_, s)| s.clone())
            .unwrap_or_default()
    }

    fn check_bundled(data: &[u8], family: &str, weight: u16) {
        let info = parse_font(data);
        assert_eq!(name(&info, 1), family);
        assert!(!info.has_fvar, "{family}: variable font, expected static");
        assert_eq!(info.weight, Some(weight), "{family}");
        let licence = name(&info, 13);
        assert!(
            licence.contains("SIL Open Font License"),
            "{family}: name ID 13 is not the OFL statement: {licence:?}"
        );
        let url = name(&info, 14);
        assert!(
            url.contains("OFL") || url.contains("openfontlicense"),
            "{family}: name ID 14 is not the OFL URL: {url:?}"
        );
    }

    #[test]
    fn bundled_fonts_carry_family_weight_and_ofl_notice() {
        check_bundled(CARLITO, "Carlito", 400);
        check_bundled(CARLITO_BOLD, "Carlito", 700);
        check_bundled(NOTO_ARABIC, "Noto Sans Arabic", 400);
        check_bundled(NOTO_ARABIC_BOLD, "Noto Sans Arabic", 700);
        check_bundled(LIBERATION_SERIF, "Liberation Serif", 400);
        check_bundled(LIBERATION_SERIF_BOLD, "Liberation Serif", 700);
    }
}
