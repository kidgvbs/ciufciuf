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

/// Carta pescata dal mazzo coperto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeckDraw {
    pub card: CardColor,
    /// true se per questa pesca è stato creato un mazzo completo nuovo (D27).
    pub rebuilt: bool,
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
) -> Option<DeckDraw> {
    if let Some(card) = draw_pile.pop() {
        return Some(DeckDraw {
            card,
            rebuilt: false,
        });
    }
    let rebuilt = discards.is_empty();
    if rebuilt {
        *draw_pile = build_train_deck(deck);
    } else {
        std::mem::swap(draw_pile, discards);
    }
    rng.shuffle(draw_pile);
    if draw_pile.is_empty() {
        return None;
    }
    events.push(GameEvent::DeckReshuffled);
    draw_pile.pop().map(|card| DeckDraw { card, rebuilt })
}

/// Rivela fino a `count` carte dal mazzo coperto per un tunnel (D30).
///
/// A mazzo vuoto rimescola gli scarti (una volta per carta mancante, quindi
/// al più una volta in pratica); se mazzo e scarti sono vuoti non crea un mazzo
/// nuovo e rivela meno carte. Ogni rimescolamento aggiunge `DeckReshuffled`.
pub fn reveal_from_deck(
    draw_pile: &mut Vec<CardColor>,
    discards: &mut Vec<CardColor>,
    rng: &mut SeededRng,
    count: u32,
    events: &mut Vec<GameEvent>,
) -> Vec<CardColor> {
    let mut revealed = Vec::with_capacity(count as usize);
    for _ in 0..count {
        if draw_pile.is_empty() && !discards.is_empty() {
            std::mem::swap(draw_pile, discards);
            rng.shuffle(draw_pile);
            events.push(GameEvent::DeckReshuffled);
        }
        let Some(card) = draw_pile.pop() else {
            break;
        };
        revealed.push(card);
    }
    revealed
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
    if let Some(DeckDraw {
        card: replacement, ..
    }) = draw_from_deck(draw_pile, discards, rng, deck, events)
    {
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
/// Se dopo quella rivelazione restano ≥3 locomotive scoperte, si ricontrolla
/// una sola volta per chiamata quando c'è un mazzo nuovo D27: (a) la
/// rivelazione ha esaurito mazzo e scarti, e allora lo si crea qui; (b)
/// `draw_from_deck` l'ha già creato durante la rivelazione. Così non restano
/// ≥3 locomotive davanti a un mazzo pieno di carte normali.
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
    let mut rebuilt = false;
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

        let mut rebuilt_in_reveal = false;
        for slot in face_up.iter_mut() {
            // Con una configurazione senza carte lo slot resta vuoto.
            if let Some(draw) = draw_from_deck(draw_pile, discards, rng, deck, events) {
                rebuilt_in_reveal |= draw.rebuilt;
                *slot = Some(draw.card);
            }
        }

        events.push(GameEvent::FaceUpReset {
            discarded,
            new_face_up: face_up.to_vec(),
        });

        if dead_end {
            if !rebuilt && loco_count_face_up(face_up) >= LOCO_RESET_THRESHOLD {
                if rebuilt_in_reveal {
                    // La rivelazione ha già creato il mazzo nuovo D27.
                    rebuilt = true;
                    continue;
                }
                if draw_pile.is_empty() && discards.is_empty() {
                    // Mazzo e scarti esauriti: D27, mazzo completo nuovo.
                    rebuilt = true;
                    *draw_pile = build_train_deck(deck);
                    if !draw_pile.is_empty() {
                        rng.shuffle(draw_pile);
                        events.push(GameEvent::DeckReshuffled);
                        continue;
                    }
                }
            }
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

    #[test]
    fn loco_reset_dead_end_with_empty_deck_rebuilds_and_rechecks() {
        let deck = DeckConfig {
            per_color: 12,
            locomotives: 14,
        };
        let mut rng = SeededRng::new(7);
        let mut face_up = vec![
            Some(CardColor::Orange),
            Some(CardColor::Locomotive),
            Some(CardColor::Locomotive),
            Some(CardColor::Locomotive),
            Some(CardColor::Orange),
        ];
        let mut draw_pile = Vec::new();
        let mut discards = Vec::new();
        let mut events = Vec::new();

        let reset = check_and_reset_loco_threshold(
            &mut face_up,
            &mut draw_pile,
            &mut discards,
            &mut rng,
            &deck,
            &mut events,
        );

        assert!(reset);
        assert!(loco_count_face_up(&face_up) <= 2);
        assert!(face_up.iter().all(Option::is_some));
        // Le 5 scoperte vecchie sono scarti, il resto è il mazzo nuovo.
        assert_eq!(draw_pile.len() + discards.len() + 5, 110 + 5);
        // Ordine: reset del dead end, mazzo nuovo, reset successivo.
        assert!(matches!(events[0], GameEvent::DeckReshuffled));
        assert!(matches!(events[1], GameEvent::FaceUpReset { .. }));
        assert!(matches!(events[2], GameEvent::DeckReshuffled));
        assert!(matches!(events.last(), Some(GameEvent::FaceUpReset { .. })));
    }
}
