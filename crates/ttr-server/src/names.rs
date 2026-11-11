//! Validazione e normalizzazione dei nomi di giocatori e partite.
//!
//! I nomi sono normalizzati in NFKC con un solo spazio ASCII tra le parole; i
//! caratteri invisibili, di controllo e i mix di alfabeti nella stessa parola
//! sono rifiutati; `U+FE0F` e `U+200D` sono ammessi solo dentro sequenze di
//! emoji. Il confronto tra nomi usa `name_key`, lo skeleton
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
    if has_forbidden(&nfkc) {
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

/// Vero se il nome ha un carattere non ammesso. `U+FE0F` segue un carattere
/// Extended_Pictographic; `U+200D` unisce due caratteri Extended_Pictographic
/// (il primo eventualmente seguito da `U+FE0F`).
fn has_forbidden(s: &str) -> bool {
    let chars: Vec<char> = s.chars().collect();
    chars.iter().enumerate().any(|(i, &c)| {
        let prev = i.checked_sub(1).map(|j| chars[j]);
        let next = chars.get(i + 1).copied();
        match c {
            '\u{FE0F}' => !prev.is_some_and(is_extended_pictographic),
            '\u{200D}' => {
                let joins_after = match prev {
                    Some('\u{FE0F}') => i >= 2 && is_extended_pictographic(chars[i - 2]),
                    Some(p) => is_extended_pictographic(p),
                    None => false,
                };
                !(joins_after && next.is_some_and(is_extended_pictographic))
            }
            _ => is_forbidden(c),
        }
    })
}

/// Approssimazione dell'Extended_Pictographic di Unicode (UTS #51): i blocchi
/// emoji e i simboli storici che ne fanno parte.
fn is_extended_pictographic(c: char) -> bool {
    matches!(
        c,
        '\u{A9}'
            | '\u{AE}'
            | '\u{203C}'
            | '\u{2049}'
            | '\u{2122}'
            | '\u{2139}'
            | '\u{2194}'..='\u{2199}'
            | '\u{21A9}'..='\u{21AA}'
            | '\u{231A}'..='\u{231B}'
            | '\u{2328}'
            | '\u{2388}'
            | '\u{23CF}'
            | '\u{23E9}'..='\u{23F3}'
            | '\u{23F8}'..='\u{23FA}'
            | '\u{24C2}'
            | '\u{25AA}'..='\u{25AB}'
            | '\u{25B6}'
            | '\u{25C0}'
            | '\u{25FB}'..='\u{25FE}'
            | '\u{2600}'..='\u{2605}'
            | '\u{2607}'..='\u{2612}'
            | '\u{2614}'..='\u{2685}'
            | '\u{2690}'..='\u{2705}'
            | '\u{2708}'..='\u{2712}'
            | '\u{2714}'
            | '\u{2716}'
            | '\u{271D}'
            | '\u{2721}'
            | '\u{2728}'
            | '\u{2733}'..='\u{2734}'
            | '\u{2744}'
            | '\u{2747}'
            | '\u{274C}'
            | '\u{274E}'
            | '\u{2753}'..='\u{2755}'
            | '\u{2757}'
            | '\u{2763}'..='\u{2767}'
            | '\u{2795}'..='\u{2797}'
            | '\u{27A1}'
            | '\u{27B0}'
            | '\u{27BF}'
            | '\u{2934}'..='\u{2935}'
            | '\u{2B05}'..='\u{2B07}'
            | '\u{2B1B}'..='\u{2B1C}'
            | '\u{2B50}'
            | '\u{2B55}'
            | '\u{3030}'
            | '\u{303D}'
            | '\u{3297}'
            | '\u{3299}'
            | '\u{1F000}'..='\u{1F0FF}'
            | '\u{1F10D}'..='\u{1F10F}'
            | '\u{1F12F}'
            | '\u{1F16C}'..='\u{1F171}'
            | '\u{1F17E}'..='\u{1F17F}'
            | '\u{1F18E}'
            | '\u{1F191}'..='\u{1F19A}'
            | '\u{1F1AD}'..='\u{1F1E5}'
            | '\u{1F201}'..='\u{1F20F}'
            | '\u{1F21A}'
            | '\u{1F22F}'
            | '\u{1F232}'..='\u{1F23A}'
            | '\u{1F23C}'..='\u{1F23F}'
            | '\u{1F249}'..='\u{1F3FA}'
            | '\u{1F400}'..='\u{1F53D}'
            | '\u{1F546}'..='\u{1F64F}'
            | '\u{1F680}'..='\u{1F6FF}'
            | '\u{1F774}'..='\u{1F77F}'
            | '\u{1F7D5}'..='\u{1F7FF}'
            | '\u{1F80C}'..='\u{1F80F}'
            | '\u{1F848}'..='\u{1F84F}'
            | '\u{1F85A}'..='\u{1F86F}'
            | '\u{1F888}'..='\u{1F88F}'
            | '\u{1F8AE}'..='\u{1F8FF}'
            | '\u{1F90C}'..='\u{1F93A}'
            | '\u{1F93C}'..='\u{1F945}'
            | '\u{1F947}'..='\u{1FAFF}'
            | '\u{1FC00}'..='\u{1FFFD}'
    )
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
    fn accepts_emoji_sequences() {
        assert_eq!(
            normalize_name("Anna \u{2764}\u{FE0F}", 24).unwrap(),
            "Anna \u{2764}\u{FE0F}"
        );
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
        assert_eq!(normalize_name(family, 24).unwrap(), family);
        let eye = "\u{1F441}\u{FE0F}\u{200D}\u{1F5E8}\u{FE0F}";
        assert_eq!(normalize_name(eye, 24).unwrap(), eye);
    }

    #[test]
    fn rejects_variation_selector_and_zwj_outside_emoji() {
        for bad in [
            "Anna\u{FE0F}",
            "Anna\u{200D}",
            "Anna\u{200D}Bob",
            "\u{2764}\u{200D}Anna",
            "Anna\u{200D}\u{2764}",
            "\u{2764}\u{200D}",
            "\u{FE0F}\u{2764}",
            "\u{2764}\u{FE0F}\u{FE0F}",
            "\u{2764}\u{FE00}",
        ] {
            assert_eq!(
                normalize_name(bad, 24),
                Err(NameError::ForbiddenChar),
                "{bad:?}"
            );
        }
    }

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
