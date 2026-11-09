//! Operazioni sul mazzo treni: pesca, rimescolamento, soglia locomotive.

use ttr_proto::CardColor;

use crate::rng::SeededRng;
use crate::types::*;

/// Costruisce un mazzo treni completo, non mescolato.
pub fn build_train_deck(config: &DeckConfig) -> Vec<CardColor> {
    let mut cards: Vec<CardColor> = Vec::with_capacity(110);
    let colors = [
        CardColor::Red,
        CardColor::Orange,
        CardColor::Yellow,
        CardColor::Green,
        CardColor::Blue,
        CardColor::Purple,
        CardColor::Black,
        CardColor::White,
    ];
    for &color in &colors {
        for _ in 0..config.per_color {
            cards.push(color);
        }
    }
    for _ in 0..config.locomotives {
        cards.push(CardColor::Locomotive);
    }
    cards
}

/// Pesca una carta dal mazzo coperto.
/// Se vuoto, rimescola gli scarti. Se sono vuoti anche gli scarti, crea un
/// mazzo completo nuovo (D27): le carte in mano restano in gioco, quindi il
/// totale può superare quello di un mazzo. Restituisce None solo se la
/// configurazione non contiene carte.
pub fn draw_from_deck(
    draw_pile: &mut Vec<CardColor>,
    discards: &mut Vec<CardColor>,
    rng: &mut SeededRng,
    deck: &DeckConfig,
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
    // Mazzo e scarti esauriti: mazzo nuovo (D27)
    *draw_pile = build_train_deck(deck);
    rng.shuffle(draw_pile);
    draw_pile.pop()
}

/// Pesca una carta dal face-up all'indice dato.
/// La sostituisce dal mazzo. Controlla la soglia locomotive dopo.
pub fn draw_from_face_up(
    index: usize,
    face_up: &mut [Option<CardColor>],
    draw_pile: &mut Vec<CardColor>,
    discards: &mut Vec<CardColor>,
    rng: &mut SeededRng,
    deck: &DeckConfig,
) -> Option<CardColor> {
    if index >= face_up.len() {
        return None;
    }
    let card = face_up[index].take()?;

    // Rimpiazza dal mazzo
    if let Some(replacement) = draw_from_deck(draw_pile, discards, rng, deck) {
        face_up[index] = Some(replacement);
    }
    // else: la configurazione non contiene carte, lo slot resta None

    // Controlla soglia ≥3 locomotive
    check_and_reset_loco_threshold(face_up, draw_pile, discards, rng, deck);

    Some(card)
}

/// Conta le locomotive visibili nelle scoperte.
pub fn loco_count_face_up(face_up: &[Option<CardColor>]) -> usize {
    face_up
        .iter()
        .filter(|c| matches!(c, Some(CardColor::Locomotive)))
        .count()
}

/// Se ≥3 locomotive scoperte, scarta tutte e 5 e rivela 5 nuove, ripetendo
/// finché le scoperte scendono sotto soglia.
///
/// Caso senza uscita (non coperto dal piano §3.1): se dopo lo scarto mazzo e
/// scarti contengono al massimo 2 carte non-locomotiva, nessuna nuova scoperta
/// può scendere sotto soglia. Gli scarti tornano nel mazzo, si rimescola una
/// volta, si riscoprono 5 carte e ci si ferma anche con ≥3 locomotive. Il pool
/// contiene sempre le ≥3 locomotive appena scartate, quindi questo ramo scatta
/// solo negli stati che prima non terminavano: con un pool di almeno 5 carte i
/// replay esistenti non cambiano. Se il pool ha meno di 5 carte, le mancanti
/// arrivano da un mazzo nuovo (D27) e ci si ferma comunque dopo quella passata.
///
/// Ritorna true se è avvenuto un reset.
pub fn check_and_reset_loco_threshold(
    face_up: &mut [Option<CardColor>],
    draw_pile: &mut Vec<CardColor>,
    discards: &mut Vec<CardColor>,
    rng: &mut SeededRng,
    deck: &DeckConfig,
) -> bool {
    let mut reset = false;
    while loco_count_face_up(face_up) >= LOCO_RESET_THRESHOLD {
        reset = true;
        // Scarta tutte le 5
        for slot in face_up.iter_mut() {
            if let Some(card) = slot.take() {
                discards.push(card);
            }
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
        }

        // Rivela 5 nuove
        for slot in face_up.iter_mut() {
            if let Some(card) = draw_from_deck(draw_pile, discards, rng, deck) {
                *slot = Some(card);
            }
            // else: la configurazione non contiene carte, slot resta None
        }

        if dead_end {
            break;
        }
    }
    reset
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
