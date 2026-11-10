//! Operazioni sul mazzo treni: pesca, rimescolamento, soglia locomotive.

use ttr_proto::CardColor;

use crate::event::GameEvent;
use crate::rng::SeededRng;
use crate::types::{DeckConfig, LOCO_RESET_THRESHOLD, TRAIN_COLORS};

/// Costruisce un mazzo treni completo, non mescolato.
pub fn build_train_deck(config: &DeckConfig) -> Vec<CardColor> {
    let per_color = config.per_color as usize;
    let locomotives = config.locomotives as usize;
    let mut cards: Vec<CardColor> =
        Vec::with_capacity(per_color.saturating_mul(TRAIN_COLORS.len()) + locomotives);
    for &color in &TRAIN_COLORS {
        cards.extend(std::iter::repeat_n(color, per_color));
    }
    cards.extend(std::iter::repeat_n(CardColor::Locomotive, locomotives));
    cards
}

/// Pesca una carta dal mazzo coperto.
///
/// A mazzo vuoto rimescola gli scarti; se anche gli scarti sono vuoti crea un
/// mazzo completo nuovo (D27, le carte in mano restano in gioco). Ogni
/// ricostruzione del mazzo aggiunge `DeckReshuffled` a `events`; restituisce
/// `None` solo se la configurazione non contiene carte.
pub fn draw_from_deck(
    draw_pile: &mut Vec<CardColor>,
    discards: &mut Vec<CardColor>,
    rng: &mut SeededRng,
    deck: &DeckConfig,
    events: &mut Vec<GameEvent>,
) -> Option<CardColor> {
    if let Some(card) = draw_pile.pop() {
        return Some(card);
    }
    if discards.is_empty() {
        *draw_pile = build_train_deck(deck);
    } else {
        std::mem::swap(draw_pile, discards);
    }
    rng.shuffle(draw_pile);
    if draw_pile.is_empty() {
        return None;
    }
    events.push(GameEvent::DeckReshuffled);
    draw_pile.pop()
}

/// Pesca la carta scoperta all'indice dato e la sostituisce dal mazzo.
///
/// Aggiunge a `events`, in ordine, la carta apparsa nello slot, le eventuali
/// ricostruzioni del mazzo e il reset delle locomotive.
pub fn draw_from_face_up(
    index: usize,
    face_up: &mut [Option<CardColor>],
    draw_pile: &mut Vec<CardColor>,
    discards: &mut Vec<CardColor>,
    rng: &mut SeededRng,
    deck: &DeckConfig,
    events: &mut Vec<GameEvent>,
) -> Option<CardColor> {
    let card = face_up.get_mut(index)?.take()?;

    // Con una configurazione senza carte lo slot resta vuoto.
    if let Some(replacement) = draw_from_deck(draw_pile, discards, rng, deck, events) {
        face_up[index] = Some(replacement);
        events.push(GameEvent::FaceUpRevealed {
            index,
            card: replacement,
        });
    }

    check_and_reset_loco_threshold(face_up, draw_pile, discards, rng, deck, events);

    Some(card)
}

/// Conta le locomotive visibili nelle scoperte.
pub fn loco_count_face_up(face_up: &[Option<CardColor>]) -> usize {
    face_up
        .iter()
        .filter(|c| matches!(c, Some(CardColor::Locomotive)))
        .count()
}

/// Con almeno `LOCO_RESET_THRESHOLD` locomotive scoperte scarta tutte le
/// scoperte e ne rivela di nuove, ripetendo finché scendono sotto soglia.
///
/// Se dopo lo scarto mazzo e scarti contengono al più 2 carte non-locomotiva
/// nessuna ripetizione può uscire dalla soglia: gli scarti tornano nel mazzo,
/// si rimescola una volta e ci si ferma dopo aver rivelato le nuove scoperte.
/// Ogni reset aggiunge `FaceUpReset` a `events` (dopo gli eventuali
/// `DeckReshuffled`); restituisce true se è avvenuto almeno un reset.
pub fn check_and_reset_loco_threshold(
    face_up: &mut [Option<CardColor>],
    draw_pile: &mut Vec<CardColor>,
    discards: &mut Vec<CardColor>,
    rng: &mut SeededRng,
    deck: &DeckConfig,
    events: &mut Vec<GameEvent>,
) -> bool {
    let mut reset = false;
    while loco_count_face_up(face_up) >= LOCO_RESET_THRESHOLD {
        reset = true;

        let mut discarded: Vec<CardColor> = Vec::new();
        for card in face_up.iter_mut().filter_map(Option::take) {
            discards.push(card);
            discarded.push(card);
        }

        let non_locos = draw_pile
            .iter()
            .chain(discards.iter())
            .filter(|c| **c != CardColor::Locomotive)
            .count();
        let dead_end = non_locos <= 2;
        if dead_end {
            draw_pile.append(discards);
            rng.shuffle(draw_pile);
            events.push(GameEvent::DeckReshuffled);
        }

        for slot in face_up.iter_mut() {
            // Con una configurazione senza carte lo slot resta vuoto.
            if let Some(card) = draw_from_deck(draw_pile, discards, rng, deck, events) {
                *slot = Some(card);
            }
        }

        events.push(GameEvent::FaceUpReset {
            discarded,
            new_face_up: face_up.to_vec(),
        });

        if dead_end {
            break;
        }
    }
    reset
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_train_deck_has_configured_composition_in_order() {
        let deck = build_train_deck(&DeckConfig {
            per_color: 12,
            locomotives: 14,
        });
        assert_eq!(deck.len(), 110);
        assert_eq!(deck[0], CardColor::Red);
        assert_eq!(deck[12], CardColor::Orange);
        assert_eq!(deck[84], CardColor::White);
        assert_eq!(deck[96], CardColor::Locomotive);
        assert_eq!(
            deck.iter().filter(|c| **c == CardColor::Locomotive).count(),
            14
        );
    }
}
