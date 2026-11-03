//! Operazioni sul mazzo treni: pesca, rimescolamento, soglia locomotive.

use ttr_proto::CardColor;

use crate::rng::SeededRng;
use crate::types::*;

/// Pesca una carta dal mazzo coperto.
/// Se vuoto, rimescola gli scarti. Se ancora vuoto, restituisce None.
pub fn draw_from_deck(
    draw_pile: &mut Vec<CardColor>,
    discards: &mut Vec<CardColor>,
    rng: &mut SeededRng,
) -> Option<CardColor> {
    if let Some(card) = draw_pile.pop() {
        return Some(card);
    }
    // Mazzo esaurito: rimescola scarti
    if !discards.is_empty() {
        std::mem::swap(draw_pile, discards);
        rng.shuffle(draw_pile);
        return draw_pile.pop();
    }
    None
}

/// Pesca una carta dal face-up all'indice dato.
/// La sostituisce dal mazzo. Controlla la soglia locomotive dopo.
pub fn draw_from_face_up(
    index: usize,
    face_up: &mut Vec<Option<CardColor>>,
    draw_pile: &mut Vec<CardColor>,
    discards: &mut Vec<CardColor>,
    rng: &mut SeededRng,
) -> Option<CardColor> {
    if index >= face_up.len() {
        return None;
    }
    let card = face_up[index].take()?;

    // Rimpiazza dal mazzo
    if let Some(replacement) = draw_from_deck(draw_pile, discards, rng) {
        face_up[index] = Some(replacement);
    }
    // else: il mazzo è completamente esaurito, lo slot resta None

    // Controlla soglia ≥3 locomotive
    check_and_reset_loco_threshold(face_up, draw_pile, discards, rng);

    Some(card)
}

/// Conta le locomotive visibili nelle scoperte.
pub fn loco_count_face_up(face_up: &[Option<CardColor>]) -> usize {
    face_up
        .iter()
        .filter(|c| matches!(c, Some(CardColor::Locomotive)))
        .count()
}

/// Se ≥3 locomotive scoperte, scarta tutte e 5 e rivela 5 nuove.
/// Ritorna true se è avvenuto un reset.
pub fn check_and_reset_loco_threshold(
    face_up: &mut Vec<Option<CardColor>>,
    draw_pile: &mut Vec<CardColor>,
    discards: &mut Vec<CardColor>,
    rng: &mut SeededRng,
) -> bool {
    let loco_count = loco_count_face_up(face_up);
    if loco_count >= LOCO_RESET_THRESHOLD {
        // Scarta tutte le 5
        for slot in face_up.iter_mut() {
            if let Some(card) = slot.take() {
                discards.push(card);
            }
        }
        // Rivela 5 nuove
        for slot in face_up.iter_mut() {
            if let Some(card) = draw_from_deck(draw_pile, discards, rng) {
                *slot = Some(card);
            }
            // else: mazzo esaurito, slot resta None
        }
        // Ricorsione: se ancora ≥3 locos, reset again
        if loco_count_face_up(face_up) >= LOCO_RESET_THRESHOLD {
            check_and_reset_loco_threshold(face_up, draw_pile, discards, rng);
        }
        return true;
    }
    false
}

/// Numero di carte rimaste (mazzo + scarti + scoperte).
#[allow(dead_code)]
pub fn total_remaining(
    draw_pile: &[CardColor],
    face_up: &[Option<CardColor>],
    discards: &[CardColor],
) -> usize {
    draw_pile.len() + face_up.iter().filter(|c| c.is_some()).count() + discards.len()
}
