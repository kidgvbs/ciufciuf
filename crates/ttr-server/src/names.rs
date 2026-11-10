//! Validazione e normalizzazione dei nomi di giocatori e partite.
//!
//! I nomi sono normalizzati in NFKC con un solo spazio ASCII tra le parole; i
//! caratteri invisibili, di controllo e i mix di alfabeti nella stessa parola
//! sono rifiutati. Il confronto tra nomi usa `name_key`, lo skeleton
//! confusabile UTS #39 in minuscolo.

use unicode_normalization::UnicodeNormalization;
use unicode_properties::{GeneralCategory, UnicodeGeneralCategory};
use unicode_security::MixedScript;

/// Motivo per cui un nome è stato rifiutato.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum NameError {
    #[error("Il nome non può essere vuoto.")]
    Empty,
    #[error("Il nome può avere al massimo {max} caratteri.")]
    TooLong { max: usize },
    #[error("Il nome contiene caratteri non ammessi (di controllo, invisibili o combinanti).")]
    ForbiddenChar,
    #[error("Ogni parola del nome deve usare un solo alfabeto.")]
    MixedScript,
}

/// Byte massimi esaminati per carattere ammesso: scarta subito input enormi
/// senza normalizzarli.
const MAX_BYTES_PER_CHAR: usize = 16;

/// Segni combinanti consecutivi ammessi.
const MAX_CONSECUTIVE_COMBINING: usize = 2;

/// Rimuove gli spazi esterni, normalizza in NFKC, riduce ogni sequenza di
/// spazi Unicode a un solo `' '` e verifica lunghezza (in caratteri),
/// caratteri invisibili o combinanti e alfabeti delle parole.
pub fn normalize_name(raw: &str, max_chars: usize) -> Result<String, NameError> {
    let raw = raw.trim();
    if raw.len() > max_chars.saturating_mul(MAX_BYTES_PER_CHAR) {
        return Err(NameError::TooLong { max: max_chars });
    }
    let nfkc: String = raw.nfkc().collect();
    if nfkc.chars().any(is_forbidden) {
        return Err(NameError::ForbiddenChar);
    }
    let name = nfkc.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name.chars().count() > max_chars {
        return Err(NameError::TooLong { max: max_chars });
    }
    for word in name.split(' ') {
        check_combining(word)?;
        if !word.is_single_script() {
            return Err(NameError::MixedScript);
        }
    }
    Ok(name)
}

/// Caratteri che non possono comparire in un nome: controllo, formato, uso
/// privato, non assegnati, separatori di riga e paragrafo, e i caratteri
/// "vuoti" che Unicode classifica come lettere o simboli.
fn is_forbidden(c: char) -> bool {
    if matches!(
        c.general_category(),
        GeneralCategory::Control
            | GeneralCategory::Format
            | GeneralCategory::PrivateUse
            | GeneralCategory::Unassigned
            | GeneralCategory::LineSeparator
            | GeneralCategory::ParagraphSeparator
    ) {
        return true;
    }
    matches!(
        c,
        '\u{115F}'
            | '\u{1160}'
            | '\u{3164}'
            | '\u{FFA0}'
            | '\u{2800}'
            | '\u{034F}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{E0100}'..='\u{E01EF}'
    )
}

fn is_combining(c: char) -> bool {
    matches!(
        c.general_category(),
        GeneralCategory::NonspacingMark | GeneralCategory::EnclosingMark
    )
}

/// Una parola non inizia con un segno combinante e ne ha al massimo
/// `MAX_CONSECUTIVE_COMBINING` di fila.
fn check_combining(word: &str) -> Result<(), NameError> {
    let mut run = 0;
    for (i, c) in word.chars().enumerate() {
        if is_combining(c) {
            run += 1;
            if i == 0 || run > MAX_CONSECUTIVE_COMBINING {
                return Err(NameError::ForbiddenChar);
            }
        } else {
            run = 0;
        }
    }
    Ok(())
}

/// Chiave di confronto: skeleton confusabile (UTS #39) del nome NFKC, con
/// spazi ridotti a uno e minuscole.
pub fn name_key(name: &str) -> String {
    let nfkc: String = name.nfkc().collect();
    let spaced = nfkc.split_whitespace().collect::<Vec<_>>().join(" ");
    let skeleton: String = unicode_security::skeleton(&spaced.to_lowercase()).collect();
    skeleton.to_lowercase()
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

    #[test]
    fn invisible_and_blank_characters_are_rejected() {
        for bad in [
            "Anna\u{3164}",
            "Anna\u{2800}",
            "Anna\u{34f}",
            "Anna\u{FE0F}",
            "\u{3164}",
            "Anna\u{115F}",
            "Anna\u{FFA0}",
            "Anna\u{E0100}",
            "Anna\u{E000}",
            "An\u{2028}na",
            "Anna\u{2029}x",
        ] {
            assert_eq!(
                normalize_name(bad, 24),
                Err(NameError::ForbiddenChar),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn unicode_whitespace_collapses_to_one_space() {
        assert_eq!(
            normalize_name("Anna\u{00A0}\u{00A0}Bianchi", 24).unwrap(),
            "Anna Bianchi"
        );
        assert_eq!(
            normalize_name("  Anna \u{3000}  \u{2003}Bianchi  ", 24).unwrap(),
            "Anna Bianchi"
        );
    }

    #[test]
    fn fullwidth_and_confusable_names_share_a_key() {
        assert_eq!(
            normalize_name("\u{FF21}\u{FF4E}\u{FF4E}\u{FF41}", 24).unwrap(),
            "Anna"
        );
        assert_eq!(
            name_key("\u{FF21}\u{FF4E}\u{FF4E}\u{FF41}"),
            name_key("Anna")
        );
        // Cirillico: С а р contro latino C a p.
        assert_eq!(name_key("\u{421}\u{430}\u{440}"), name_key("Cap"));
        assert_eq!(name_key("Anna  Bianchi"), name_key("anna bianchi"));
    }

    #[test]
    fn mixed_script_words_are_rejected() {
        assert_eq!(
            normalize_name("\u{410}nna", 24),
            Err(NameError::MixedScript)
        );
        assert!(normalize_name("\u{421}\u{430}\u{440}", 24).is_ok());
        assert!(
            normalize_name("Anna \u{41F}\u{440}\u{438}\u{432}\u{435}\u{442}", 24).is_ok(),
            "alfabeti diversi in parole diverse"
        );
        assert!(normalize_name("Anna_2000", 24).is_ok());
    }

    #[test]
    fn accented_names_stay_valid_in_both_forms() {
        for name in ["Jos\u{e9}", "Jose\u{301}", "Zo\u{eb}", "Zoe\u{308}"] {
            assert!(normalize_name(name, 24).is_ok(), "{name:?}");
        }
        assert_eq!(
            normalize_name("Jose\u{301}", 24).unwrap(),
            normalize_name("Jos\u{e9}", 24).unwrap()
        );
    }

    #[test]
    fn stray_combining_marks_are_rejected() {
        assert_eq!(
            normalize_name("\u{301}Anna", 24),
            Err(NameError::ForbiddenChar)
        );
        assert_eq!(
            normalize_name("Anna \u{301}Bob", 24),
            Err(NameError::ForbiddenChar)
        );
        assert_eq!(
            normalize_name("a\u{300}\u{301}\u{302}\u{303}", 24),
            Err(NameError::ForbiddenChar)
        );
    }
}
