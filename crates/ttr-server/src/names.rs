//! Validazione e normalizzazione dei nomi di giocatori e partite.
//!
//! I nomi sono normalizzati in NFC; i caratteri di controllo e di formato
//! (zero-width, bidi) sono rifiutati. Il confronto tra nomi usa `name_key`.

use unicode_normalization::UnicodeNormalization;
use unicode_properties::{GeneralCategory, UnicodeGeneralCategory};

/// Motivo per cui un nome è stato rifiutato.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum NameError {
    #[error("Il nome non può essere vuoto.")]
    Empty,
    #[error("Il nome può avere al massimo {max} caratteri.")]
    TooLong { max: usize },
    #[error("Il nome contiene caratteri non ammessi (di controllo o invisibili).")]
    ForbiddenChar,
}

/// Byte massimi esaminati per carattere ammesso: scarta subito input enormi
/// senza normalizzarli.
const MAX_BYTES_PER_CHAR: usize = 16;

/// Rimuove gli spazi esterni, normalizza in NFC e verifica lunghezza (in
/// caratteri) e categorie Unicode `Cc`/`Cf`.
pub fn normalize_name(raw: &str, max_chars: usize) -> Result<String, NameError> {
    let raw = raw.trim();
    if raw.len() > max_chars.saturating_mul(MAX_BYTES_PER_CHAR) {
        return Err(NameError::TooLong { max: max_chars });
    }
    let name: String = raw.nfc().collect();
    if name.chars().any(is_forbidden) {
        return Err(NameError::ForbiddenChar);
    }
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name.chars().count() > max_chars {
        return Err(NameError::TooLong { max: max_chars });
    }
    Ok(name)
}

fn is_forbidden(c: char) -> bool {
    matches!(
        c.general_category(),
        GeneralCategory::Control | GeneralCategory::Format
    )
}

/// Chiave di confronto: NFC, spazi esterni rimossi, minuscolo.
pub fn name_key(name: &str) -> String {
    name.trim()
        .nfc()
        .collect::<String>()
        .to_lowercase()
        .nfc()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_to_nfc_and_trims() {
        assert_eq!(normalize_name("  e\u{301} ", 24).unwrap(), "\u{e9}");
    }

    #[test]
    fn rejects_control_and_format_characters() {
        for bad in [
            "Anna\u{200b}",
            "An\u{202e}na",
            "An\nna",
            "\u{feff}Anna",
            "Anna\u{0}",
        ] {
            assert_eq!(
                normalize_name(bad, 24),
                Err(NameError::ForbiddenChar),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn rejects_empty_and_too_long() {
        assert_eq!(normalize_name("   ", 24), Err(NameError::Empty));
        assert_eq!(
            normalize_name(&"x".repeat(25), 24),
            Err(NameError::TooLong { max: 24 })
        );
        assert_eq!(
            normalize_name(&"x".repeat(1_000_000), 24),
            Err(NameError::TooLong { max: 24 })
        );
        assert!(normalize_name(&"è".repeat(24), 24).is_ok());
    }

    #[test]
    fn keys_ignore_case_and_normalization_form() {
        assert_eq!(name_key("\u{c9}mile"), name_key("e\u{301}mile"));
        assert_eq!(name_key(" ANNA "), name_key("anna"));
    }
}
