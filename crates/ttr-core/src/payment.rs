//! Pagamento delle rotte: funzione pura che assegna le carte agli spazi.
//!
//! Una rotta di lunghezza `len` ha `len` spazi: `ferries` spazi locomotiva (traghetto) e
//! gli altri spazi colore. Con le sostituzioni del ruleset (`ferries.anyCardsPerLocomotive`,
//! `Route::any_cards_per_space`) uno spazio si può pagare anche con un gruppo di carte
//! qualsiasi, locomotive comprese. Un pagamento è valido se TUTTE le carte dichiarate si
//! assegnano esattamente agli spazi, senza carte avanzate:
//!
//! - spazio colore: 1 carta del colore (sulle grigie un solo colore per tutte le carte
//!   singole colorate), 1 locomotiva (solo dove il ruleset la ammette) o un gruppo;
//! - spazio locomotiva: 1 locomotiva (sempre ammessa) o un gruppo.
//!
//! Senza sostituzioni né restrizioni vale la regola classica: `len` carte del colore,
//! con le locomotive jolly e almeno `ferries` locomotive.

use std::collections::BTreeSet;

use ttr_proto::CardColor;

use crate::error::EngineInitError;
use crate::types::{COLOR_COUNT, MapData, Route, RouteColor, Ruleset, TRAIN_COLORS};

/// Valore di `locomotives.usableOn` per i tunnel.
pub const USABLE_ON_TUNNEL: &str = "tunnel";
/// Valore di `locomotives.usableOn` per i traghetti (rotte con `ferries > 0`).
pub const USABLE_ON_FERRY: &str = "ferry";

impl Ruleset {
    /// Vero se una locomotiva può pagare da sola uno spazio colore della rotta.
    #[must_use]
    pub fn locomotive_usable_on(&self, route: &Route) -> bool {
        let Some(list) = self.locomotives.as_ref().and_then(|l| l.usable_on.as_ref()) else {
            return true;
        };
        let has = |value: &str| list.iter().any(|v| v == value);
        (route.tunnel && has(USABLE_ON_TUNNEL)) || (route.ferries > 0 && has(USABLE_ON_FERRY))
    }

    /// Vero se la locomotiva scoperta conta come una carta normale (`freeDraw`).
    #[must_use]
    pub fn free_locomotive_draw(&self) -> bool {
        self.locomotives.as_ref().is_some_and(|l| l.free_draw)
    }

    /// Vero se con 3 locomotive scoperte il mercato si scarta e rimpiazza (`faceUpReset`,
    /// default `true`).
    #[must_use]
    pub fn face_up_reset_enabled(&self) -> bool {
        self.locomotives.as_ref().is_none_or(|l| l.face_up_reset)
    }

    /// `locomotives.usableOn`, `ferries.anyCardsPerLocomotive` e `anyCardsPerSpace`.
    pub(crate) fn check_payment_rules(&self, map: &MapData) -> Result<(), EngineInitError> {
        if let Some(list) = self.locomotives.as_ref().and_then(|l| l.usable_on.as_ref()) {
            if list.is_empty() {
                return Err(EngineInitError::LocomotivesUsableOnEmpty);
            }
            let mut seen: BTreeSet<&str> = BTreeSet::new();
            for value in list {
                if value != USABLE_ON_TUNNEL && value != USABLE_ON_FERRY {
                    return Err(EngineInitError::LocomotivesUsableOnUnknown {
                        value: value.clone(),
                    });
                }
                if !seen.insert(value.as_str()) {
                    return Err(EngineInitError::LocomotivesUsableOnDuplicate {
                        value: value.clone(),
                    });
                }
            }
        }
        if let Some(ferries) = &self.ferries
            && ferries.any_cards_per_locomotive < 2
        {
            return Err(EngineInitError::FerriesAnyCardsTooSmall {
                value: ferries.any_cards_per_locomotive,
            });
        }
        for route in &map.routes {
            if let Some(value) = route.any_cards_per_space
                && value < 2
            {
                return Err(EngineInitError::RouteAnyCardsPerSpaceTooSmall {
                    route_id: route.id.clone(),
                    value,
                });
            }
        }
        Ok(())
    }
}

/// Conteggio di un insieme di carte per colore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tally {
    per_color: [usize; COLOR_COUNT],
    locomotives: usize,
    total: usize,
}

impl Tally {
    pub(crate) fn of(cards: &[CardColor]) -> Self {
        let mut tally = Self {
            per_color: [0; COLOR_COUNT],
            locomotives: 0,
            total: cards.len(),
        };
        for card in cards {
            match TRAIN_COLORS.iter().position(|c| c == card) {
                Some(index) => tally.per_color[index] += 1,
                None => tally.locomotives += 1,
            }
        }
        tally
    }

    /// Carte del colore dato (le locomotive non sono un colore).
    fn of_color(&self, color: CardColor) -> usize {
        TRAIN_COLORS
            .iter()
            .position(|c| *c == color)
            .map_or(0, |index| self.per_color[index])
    }
}

/// Perché un pagamento è rifiutato.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PaymentError {
    /// Carte in meno del necessario (o locomotive del traghetto mancanti).
    NotEnough,
    /// Le carte non si assegnano esattamente agli spazi della rotta.
    Invalid(String),
}

/// Regole di pagamento di una rotta, risolte da rotta e ruleset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PaymentSpec {
    len: usize,
    ferries: usize,
    color: RouteColor,
    /// Una locomotiva può pagare da sola uno spazio colore.
    single_loco: bool,
    /// Carte qualsiasi che valgono una locomotiva sugli spazi locomotiva del traghetto.
    loco_group: Option<usize>,
    /// Dimensioni dei gruppi di carte qualsiasi per uno spazio colore, ordinate.
    color_groups: Vec<usize>,
    /// Spazi onda che una carta Traghetto copre al massimo su questa rotta (`wavesPerCard`);
    /// 0 se il ruleset non ha carte Traghetto o la rotta non ha onde.
    waves_per_ferry_card: usize,
    /// Carte Traghetto giocate (per `verify`) o in mano (per `payable_from`); 0 di default.
    ferry_cards: usize,
}

impl PaymentSpec {
    pub(crate) fn new(route: &Route, color: RouteColor, ruleset: &Ruleset) -> Self {
        let ferry_any = if route.ferries > 0 {
            ruleset
                .ferries
                .as_ref()
                .map(|f| f.any_cards_per_locomotive as usize)
        } else {
            None
        };
        let mut spec = Self::from_parts(
            route.len as usize,
            route.ferries as usize,
            color,
            ruleset.locomotive_usable_on(route),
            ferry_any,
            route.any_cards_per_space.map(|v| v as usize),
        );
        if route.ferries > 0
            && let Some(config) = &ruleset.ferry_cards
        {
            spec.waves_per_ferry_card = config.waves_per_card as usize;
        }
        spec
    }

    fn from_parts(
        len: usize,
        ferries: usize,
        color: RouteColor,
        single_loco: bool,
        ferry_any: Option<usize>,
        space_any: Option<usize>,
    ) -> Self {
        let mut color_groups: Vec<usize> = ferry_any.into_iter().chain(space_any).collect();
        color_groups.sort_unstable();
        color_groups.dedup();
        Self {
            len,
            ferries,
            color,
            single_loco,
            loco_group: ferry_any,
            color_groups,
            waves_per_ferry_card: 0,
            ferry_cards: 0,
        }
    }

    /// Imposta le carte Traghetto giocate (`verify`) o in mano (`payable_from`): ognuna
    /// copre da 1 a `wavesPerCard` spazi onda, gli altri spazi onda vogliono locomotive.
    pub(crate) fn with_ferry_cards(mut self, ferry_cards: usize) -> Self {
        self.ferry_cards = ferry_cards;
        self
    }

    /// La rotta senza gli `waves` spazi onda coperti da carte Traghetto.
    fn without_waves(&self, waves: usize) -> Self {
        Self {
            len: self.len - waves,
            ferries: self.ferries - waves,
            ..self.clone()
        }
    }

    /// Spazi onda che le carte Traghetto possono coprire: da uno per carta fino a
    /// `wavesPerCard` a carta, mai più delle onde della rotta. L'intervallo è vuoto
    /// (`start > end`) se le carte non sono ammesse o sono più delle onde.
    fn covered_waves(&self) -> std::ops::RangeInclusive<usize> {
        if self.ferry_cards == 0 {
            return 0..=0;
        }
        self.ferry_cards
            ..=self
                .ferry_cards
                .saturating_mul(self.waves_per_ferry_card)
                .min(self.ferries)
    }

    /// Vero se la rotta ammette gruppi di carte qualsiasi (il pagamento può avere più
    /// di `len` carte).
    pub(crate) fn has_groups(&self) -> bool {
        self.loco_group.is_some() || !self.color_groups.is_empty()
    }

    /// Vero se una locomotiva può pagare da sola uno spazio colore.
    pub(crate) fn single_loco(&self) -> bool {
        self.single_loco
    }

    /// Numero di carte non Traghetto (locomotive comprese) che pagano la rotta senza
    /// gruppi di carte qualsiasi, con le carte Traghetto impostate; `None` se queste non
    /// sono ammesse o sono più delle onde.
    pub(crate) fn regular_card_range(&self) -> Option<std::ops::RangeInclusive<usize>> {
        let waves = self.covered_waves();
        (waves.start() <= waves.end()).then(|| self.len - waves.end()..=self.len - waves.start())
    }

    /// Verifica che tutte le carte e le carte Traghetto impostate si assegnino
    /// esattamente agli spazi.
    pub(crate) fn verify(&self, cards: &[CardColor]) -> Result<(), PaymentError> {
        let tally = Tally::of(cards);
        let waves = self.covered_waves();
        if waves.start() > waves.end() {
            return Err(PaymentError::Invalid(
                "carte Traghetto non ammesse su questa rotta o più delle onde da coprire"
                    .to_string(),
            ));
        }
        if waves
            .clone()
            .any(|covered| self.without_waves(covered).assignable(&tally, true))
        {
            return Ok(());
        }
        let most_covered = self.without_waves(*waves.end());
        if cards.len() < most_covered.len
            || (!self.has_groups() && tally.locomotives < most_covered.ferries)
        {
            return Err(PaymentError::NotEnough);
        }
        let least_covered = self.without_waves(*waves.start());
        if !self.has_groups() && !self.single_loco && tally.locomotives > least_covered.ferries {
            return Err(PaymentError::Invalid(
                "locomotive non ammesse su questa rotta".to_string(),
            ));
        }
        Err(PaymentError::Invalid(
            "le carte non si assegnano esattamente agli spazi della rotta".to_string(),
        ))
    }

    /// Vero se, tra le carte della mano e le carte Traghetto in mano impostate, esiste un
    /// sottoinsieme che paga la rotta. Più carte Traghetto coprono più onde, quindi
    /// conviene sempre usarle tutte (al massimo quante sono le onde).
    pub(crate) fn payable_from(&self, hand: &Tally) -> bool {
        let usable = self
            .clone()
            .with_ferry_cards(self.ferry_cards.min(self.ferries));
        let covered = *usable.covered_waves().end();
        usable.without_waves(covered).payable_from_plain(hand)
    }

    /// `payable_from` senza carte Traghetto.
    fn payable_from_plain(&self, hand: &Tally) -> bool {
        if self.has_groups() {
            return self.assignable(hand, false);
        }
        // Senza sostituzioni: `len` carte, almeno `ferries` locomotive; le altre del
        // colore (le locomotive solo dove ammesse).
        if hand.locomotives < self.ferries {
            return false;
        }
        let colored_spaces = self.len.saturating_sub(self.ferries);
        let best = self.best_color_count(hand);
        if self.single_loco {
            best + hand.locomotives >= self.len
        } else {
            best >= colored_spaces
        }
    }

    /// Massimo numero di carte di un colore ammesso dalla rotta.
    fn best_color_count(&self, hand: &Tally) -> usize {
        match self.color {
            RouteColor::Colored(color) => hand.of_color(color),
            RouteColor::Gray => hand.per_color.iter().copied().max().unwrap_or(0),
        }
    }

    /// Esiste un'assegnazione delle carte agli spazi? Con `exact` usa tutte le carte;
    /// altrimenti basta un sottoinsieme (la mano).
    fn assignable(&self, tally: &Tally, exact: bool) -> bool {
        let colored_spaces = self.len.saturating_sub(self.ferries);
        let colors: Vec<usize> = match self.color {
            RouteColor::Colored(color) => vec![tally.of_color(color)],
            RouteColor::Gray => tally.per_color.to_vec(),
        };
        for single_color_cards in colors {
            for colored_singles in 0..=single_color_cards.min(colored_spaces) {
                let loco_singles_max = if self.single_loco {
                    (colored_spaces - colored_singles).min(tally.locomotives)
                } else {
                    0
                };
                for colored_locos in 0..=loco_singles_max {
                    let locos_left = tally.locomotives - colored_locos;
                    for ferry_locos in 0..=self.ferries.min(locos_left) {
                        let ferry_groups = self.ferries - ferry_locos;
                        let ferry_group_cards = match self.loco_group {
                            Some(size) => ferry_groups * size,
                            None if ferry_groups == 0 => 0,
                            None => continue,
                        };
                        let singles = colored_singles + colored_locos + ferry_locos;
                        let Some(pool) = (tally.total - singles).checked_sub(ferry_group_cards)
                        else {
                            continue;
                        };
                        let color_group_spaces = colored_spaces - colored_singles - colored_locos;
                        if groups_fit(&self.color_groups, color_group_spaces, pool, exact) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

/// `spaces` spazi si riempiono con gruppi delle dimensioni date usando esattamente
/// `target` carte (`exact`) o al più `target`.
fn groups_fit(sizes: &[usize], spaces: usize, target: usize, exact: bool) -> bool {
    let Some((&size, rest)) = sizes.split_first() else {
        return spaces == 0 && (!exact || target == 0);
    };
    (0..=spaces).any(|n| {
        let used = n * size;
        used <= target && groups_fit(rest, spaces - n, target - used, exact)
    })
}

/// Controllo storico sulle carte dichiarate di una rotta senza sostituzioni: ogni carta
/// è del colore della rotta o locomotiva (jolly); sulle grigie le carte colorate hanno
/// tutte lo stesso colore.
pub(crate) fn check_declared_colors(
    color: RouteColor,
    declared: &[CardColor],
) -> Result<(), String> {
    match color {
        RouteColor::Colored(expected) => {
            for card in declared {
                if *card != expected && *card != CardColor::Locomotive {
                    return Err(format!(
                        "carta {card:?} non corrisponde al colore della rotta {expected:?}"
                    ));
                }
            }
        }
        RouteColor::Gray => {
            let non_loco_color = declared
                .iter()
                .find(|c| **c != CardColor::Locomotive)
                .copied();
            if let Some(color) = non_loco_color {
                for card in declared {
                    if *card != color && *card != CardColor::Locomotive {
                        return Err(format!(
                            "rotta grigia: carte di colori diversi {color:?} e {card:?}"
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use CardColor::{Blue, Green, Locomotive, Red};

    fn cards(parts: &[(CardColor, usize)]) -> Vec<CardColor> {
        parts
            .iter()
            .flat_map(|(c, n)| std::iter::repeat_n(*c, *n))
            .collect()
    }

    /// Rotta normale Nordic: locomotive vietate da sole, nessuna sostituzione.
    fn plain(len: usize, color: RouteColor, single_loco: bool) -> PaymentSpec {
        PaymentSpec::from_parts(len, 0, color, single_loco, None, None)
    }

    /// Traghetto Nordic: `ferries` spazi locomotiva, 3 carte qualsiasi per locomotiva.
    fn ferry(len: usize, ferries: usize, color: RouteColor) -> PaymentSpec {
        PaymentSpec::from_parts(len, ferries, color, true, Some(3), None)
    }

    /// Murmansk–Lieksa: grigia di 9, 4 carte qualsiasi per spazio, loco singola vietata.
    fn murmansk() -> PaymentSpec {
        PaymentSpec::from_parts(9, 0, RouteColor::Gray, false, None, Some(4))
    }

    #[test]
    fn classic_route_accepts_exact_color_and_wild_locomotives() {
        let spec = plain(3, RouteColor::Colored(Red), true);
        assert_eq!(spec.verify(&cards(&[(Red, 3)])), Ok(()));
        assert_eq!(spec.verify(&cards(&[(Red, 1), (Locomotive, 2)])), Ok(()));
        assert!(spec.verify(&cards(&[(Red, 2), (Blue, 1)])).is_err());
        assert_eq!(
            spec.verify(&cards(&[(Red, 2)])),
            Err(PaymentError::NotEnough)
        );
        // Carte avanzate: rifiutate.
        assert!(spec.verify(&cards(&[(Red, 4)])).is_err());
    }

    #[test]
    fn classic_gray_route_needs_one_color() {
        let spec = plain(2, RouteColor::Gray, true);
        assert_eq!(spec.verify(&cards(&[(Blue, 2)])), Ok(()));
        assert_eq!(spec.verify(&cards(&[(Locomotive, 2)])), Ok(()));
        assert_eq!(spec.verify(&cards(&[(Blue, 1), (Locomotive, 1)])), Ok(()));
        assert!(spec.verify(&cards(&[(Blue, 1), (Green, 1)])).is_err());
    }

    #[test]
    fn nordic_plain_route_refuses_locomotives() {
        let spec = plain(3, RouteColor::Colored(Red), false);
        assert_eq!(spec.verify(&cards(&[(Red, 3)])), Ok(()));
        assert!(matches!(
            spec.verify(&cards(&[(Red, 2), (Locomotive, 1)])),
            Err(PaymentError::Invalid(_))
        ));
        let gray = plain(2, RouteColor::Gray, false);
        assert!(gray.verify(&cards(&[(Locomotive, 2)])).is_err());
    }

    #[test]
    fn ferry_without_substitutions_needs_the_locomotives() {
        let spec = PaymentSpec::from_parts(4, 2, RouteColor::Colored(Blue), true, None, None);
        assert_eq!(spec.verify(&cards(&[(Blue, 2), (Locomotive, 2)])), Ok(()));
        assert_eq!(
            spec.verify(&cards(&[(Blue, 4)])),
            Err(PaymentError::NotEnough)
        );
    }

    #[test]
    fn ferry_three_any_cards_replace_a_locomotive() {
        // Traghetto di 3 spazi, 1 simbolo locomotiva, blu.
        let spec = ferry(3, 1, RouteColor::Colored(Blue));
        assert_eq!(spec.verify(&cards(&[(Blue, 2), (Locomotive, 1)])), Ok(()));
        // 3 carte qualsiasi al posto della locomotiva.
        assert_eq!(
            spec.verify(&cards(&[(Blue, 2), (Red, 2), (Green, 1)])),
            Ok(())
        );
        // Anche 3 carte qualsiasi per uno spazio colore (invece della carta blu).
        assert_eq!(
            spec.verify(&cards(&[(Locomotive, 1), (Red, 3), (Blue, 1)])),
            Ok(())
        );
        // Carta avanzata.
        assert!(
            spec.verify(&cards(&[(Blue, 2), (Red, 2), (Green, 2)]))
                .is_err()
        );
        // Troppo poche.
        assert!(spec.verify(&cards(&[(Blue, 2), (Red, 2)])).is_err());
    }

    #[test]
    fn gray_ferry_uses_one_color_for_single_cards_plus_groups() {
        let spec = ferry(3, 1, RouteColor::Gray);
        // 2 blu singole + locomotiva.
        assert_eq!(spec.verify(&cards(&[(Blue, 2), (Locomotive, 1)])), Ok(()));
        // 1 blu, 1 locomotiva, 3 carte qualsiasi per il secondo spazio colore.
        assert_eq!(
            spec.verify(&cards(&[(Blue, 1), (Locomotive, 1), (Red, 2), (Green, 1)])),
            Ok(())
        );
        // Due colori singoli diversi non vanno.
        assert!(
            spec.verify(&cards(&[(Blue, 1), (Red, 1), (Locomotive, 1)]))
                .is_err()
        );
    }

    #[test]
    fn murmansk_seven_green_plus_eight_any() {
        let spec = murmansk();
        // 7 verdi + 2 spazi da 4 carte qualsiasi (locomotive comprese).
        let pay = cards(&[(Green, 7), (Red, 3), (Locomotive, 3), (Blue, 2)]);
        assert_eq!(pay.len(), 15);
        assert_eq!(spec.verify(&pay), Ok(()));
        // 9 verdi classico.
        assert_eq!(spec.verify(&cards(&[(Green, 9)])), Ok(()));
        // Loco singola vietata, ma dentro un gruppo ok: 5 verdi + 1 loco + 3 qualsiasi...
        assert!(spec.verify(&cards(&[(Green, 8), (Locomotive, 1)])).is_err());
        // Tutto in gruppi: 36 carte qualsiasi.
        assert_eq!(
            spec.verify(&cards(&[(Red, 12), (Blue, 12), (Locomotive, 12)])),
            Ok(())
        );
        // Carte avanzate: 7 verdi + 7 qualsiasi non fanno 2 gruppi da 4.
        assert!(spec.verify(&cards(&[(Green, 7), (Red, 7)])).is_err());
        // 8 verdi + 4 qualsiasi: 8 singole non possono essere 8 spazi + 1 gruppo? sì: 8+1=9.
        assert_eq!(spec.verify(&cards(&[(Green, 8), (Red, 4)])), Ok(()));
        // Un solo colore singolo per carta: rosse e verdi singole insieme no.
        assert!(spec.verify(&cards(&[(Green, 4), (Red, 5)])).is_err());
    }

    #[test]
    fn switzerland_locomotive_only_on_tunnels() {
        let tunnel = plain(2, RouteColor::Colored(Red), true);
        let road = plain(2, RouteColor::Colored(Red), false);
        let pay = cards(&[(Red, 1), (Locomotive, 1)]);
        assert_eq!(tunnel.verify(&pay), Ok(()));
        assert!(road.verify(&pay).is_err());
        assert_eq!(road.verify(&cards(&[(Red, 2)])), Ok(()));
    }

    #[test]
    fn payable_from_matches_exact_verification_without_groups() {
        // Confronto esaustivo sulle mani piccole: la forma chiusa senza gruppi deve
        // coincidere con la ricerca generica (sottoinsieme esatto della mano).
        for &single_loco in &[true, false] {
            for color in [RouteColor::Gray, RouteColor::Colored(Red)] {
                for (len, ferries) in [(1, 0), (2, 0), (3, 0), (3, 1), (4, 2), (2, 2)] {
                    let spec =
                        PaymentSpec::from_parts(len, ferries, color, single_loco, None, None);
                    for red in 0..=4 {
                        for blue in 0..=4 {
                            for loco in 0..=4 {
                                let hand = cards(&[(Red, red), (Blue, blue), (Locomotive, loco)]);
                                let tally = Tally::of(&hand);
                                assert_eq!(
                                    spec.payable_from(&tally),
                                    spec.assignable(&tally, false),
                                    "{spec:?} mano r{red} b{blue} l{loco}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn payable_from_uses_groups_with_the_hand() {
        let spec = ferry(3, 1, RouteColor::Colored(Blue));
        // Nessuna loco: 2 blu + 3 qualsiasi.
        let hand = Tally::of(&cards(&[(Blue, 2), (Red, 3)]));
        assert!(spec.payable_from(&hand));
        let short = Tally::of(&cards(&[(Blue, 2), (Red, 2)]));
        assert!(!spec.payable_from(&short));
        // Murmansk con 4 locomotive + 5 verdi: 5 singole + 1 gruppo da 4 + ... 5+... = 9?
        // 5 verdi (5 spazi) + 4 locomotive (1 gruppo = 1 spazio) = 6 spazi: non basta.
        let m = murmansk();
        assert!(!m.payable_from(&Tally::of(&cards(&[(Green, 5), (Locomotive, 4)]))));
        // 6 verdi + 12 loco = 6 + 3 spazi.
        assert!(m.payable_from(&Tally::of(&cards(&[(Green, 6), (Locomotive, 12)]))));
    }

    #[test]
    fn declared_colors_keep_the_legacy_messages() {
        assert!(check_declared_colors(RouteColor::Colored(Red), &[Red, Locomotive]).is_ok());
        assert!(check_declared_colors(RouteColor::Colored(Red), &[Blue]).is_err());
        assert!(check_declared_colors(RouteColor::Gray, &[Blue, Green]).is_err());
        assert!(check_declared_colors(RouteColor::Gray, &[Locomotive, Blue, Locomotive]).is_ok());
    }
}
