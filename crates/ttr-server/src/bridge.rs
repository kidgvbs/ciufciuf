//! Conversione core ↔ proto: `PublicState`, `PrivateState`, `ErrorReason`, `FinalScoreInfo`.

use std::collections::{BTreeMap, HashSet};

use ttr_core::error::{EngineError, GamePhase};
use ttr_proto::{
    CardColor, ClaimedRoute, ErrorReason, FinalScoreInfo, GameState, LegalMoves, PlayerSummary,
    TicketInfo,
};

use crate::session::{GameSession, SessionPlayer};

// ── PublicState ──────────────────────────────────────────────────────

pub(crate) fn build_public_state(
    session: &GameSession,
    connected_player_ids: &HashSet<String>,
) -> ttr_proto::PublicState {
    let state = if session.started {
        if let Some(ref engine) = session.engine {
            game_phase_to_game_state(engine.phase())
        } else {
            GameState::WaitingForPlayers
        }
    } else {
        GameState::WaitingForPlayers
    };

    let current_turn_player_id = if session.started {
        session.engine.as_ref().and_then(|e| {
            if matches!(e.phase(), GamePhase::GameOver) {
                None
            } else {
                session
                    .player_id_for_index(e.current_player())
                    .map(|s| s.to_string())
            }
        })
    } else {
        None
    };

    let last_round = session
        .engine
        .as_ref()
        .map(|e| e.last_round())
        .unwrap_or(false);

    let last_round_trigger_player_id = session.engine.as_ref().and_then(|e| {
        e.last_round_trigger_player()
            .and_then(|idx| session.player_id_for_index(idx).map(|s| s.to_string()))
    });

    let visible_deck: Vec<Option<CardColor>> = session
        .engine
        .as_ref()
        .map(|e| e.face_up_cards().to_vec())
        .unwrap_or_default();

    // Numero di carte nel mazzo coperto (0 pre-start, Engine non ancora costruito).
    let deck_size = session.engine.as_ref().map(|e| e.deck_size()).unwrap_or(0);

    let claimed_routes: Vec<ClaimedRoute> = if let Some(ref engine) = session.engine {
        let mut claimed: Vec<ClaimedRoute> = Vec::new();
        for (pid, &idx) in &session.player_index {
            // `idx` è la posizione in `session.players`, non l'indice del motore.
            let engine_idx = session.players.get(idx).and_then(|p| p.index);
            if let Some(player) = engine_idx.and_then(|ei| engine.player(ei)) {
                for route_id in player.claimed_routes.keys() {
                    claimed.push(ClaimedRoute {
                        route_id: route_id.clone(),
                        claimed_by_player_id: pid.clone(),
                    });
                }
            }
        }
        claimed.sort_by(|a, b| {
            a.claimed_by_player_id
                .cmp(&b.claimed_by_player_id)
                .then_with(|| a.route_id.cmp(&b.route_id))
        });
        claimed
    } else {
        Vec::new()
    };

    let players_summary = build_players_summary(session, connected_player_ids);

    ttr_proto::PublicState {
        game_id: session.game_id.clone(),
        protocol_version: 1,
        state,
        current_turn_player_id,
        last_round,
        last_round_trigger_player_id,
        visible_deck,
        deck_size,
        claimed_routes,
        players_summary,
        tunnel_reveal: None,
        stations: vec![],
        final_scores: session.final_scores.clone(),
        longest_path_player_ids: session.longest_path_player_ids.clone(),
    }
}

fn game_phase_to_game_state(phase: GamePhase) -> GameState {
    match phase {
        GamePhase::InitialDraft => GameState::InitialDraft,
        GamePhase::PlayerTurn => GameState::PlayerTurn,
        GamePhase::SecondCardDraw => GameState::SecondCardDraw,
        GamePhase::TicketChoice => GameState::TicketChoice,
        GamePhase::LastRound => GameState::LastRound,
        GamePhase::GameOver => GameState::GameOver,
        // Fasi future del motore senza un GameState dedicato.
        other => {
            tracing::error!(phase = ?other, "fase del motore senza GameState corrispondente");
            GameState::PlayerTurn
        }
    }
}

fn build_players_summary(
    session: &GameSession,
    connected_player_ids: &HashSet<String>,
) -> Vec<PlayerSummary> {
    // Usa l'indice del motore (SessionPlayer.index) quando disponibile;
    // altrimenti la posizione in self.players (idx) come fallback pre-start.
    let mut ordered: Vec<(usize, &String, &SessionPlayer)> = session
        .player_index
        .iter()
        .filter_map(|(pid, &idx)| {
            session.players.get(idx).map(|p| {
                let engine_idx = p.index.unwrap_or(idx);
                (engine_idx, pid, p)
            })
        })
        .collect();
    ordered.sort_by_key(|(ei, _, _)| *ei);

    let max_trains: u32 = 45; // USA default

    ordered
        .into_iter()
        .map(|(engine_idx, pid, p)| {
            let (trains, score, hand_count, tickets_count) =
                if let Some(ref engine) = session.engine {
                    let player = engine.player(engine_idx);
                    (
                        player.map(|pl| pl.trains_remaining).unwrap_or(max_trains),
                        player.map(|pl| pl.score).unwrap_or(0),
                        player.map(|pl| pl.hand.len() as u32).unwrap_or(0),
                        player.map(|pl| pl.tickets.len() as u32).unwrap_or(0),
                    )
                } else {
                    (max_trains, 0, 0, 0)
                };

            PlayerSummary {
                player_id: pid.clone(),
                name: p.name.clone(),
                color: p.color,
                trains_remaining: trains,
                stations_remaining: 0,
                score,
                cards_hand_count: hand_count,
                tickets_count,
                connected: connected_player_ids.contains(pid),
            }
        })
        .collect()
}

// ── PrivateState ─────────────────────────────────────────────────────

pub(crate) fn build_private_state(
    session: &GameSession,
    player_id: &str,
    player_index: Option<usize>,
) -> Option<ttr_proto::PrivateState> {
    let engine = session.engine.as_ref()?;
    let map = session.map_data.as_ref()?;
    let pi = player_index?;

    if pi >= engine.num_players() {
        return None;
    }

    let hand: Vec<CardColor> = engine.player_hand(pi).unwrap_or(&[]).to_vec();

    // Biglietti posseduti
    let tickets: Vec<TicketInfo> = {
        let player = engine.player(pi)?;
        let mut tix: Vec<TicketInfo> = player
            .tickets
            .iter()
            .map(|(tid, ts)| TicketInfo {
                id: tid.clone(),
                points: ts.points,
                completed: ts.completed.unwrap_or(false),
            })
            .collect();
        tix.sort_by(|a, b| a.id.cmp(&b.id));
        tix
    };

    // Biglietti offerti: solo quelli che il giocatore può scegliere ora.
    let offered_tickets: Vec<TicketInfo> = build_offered_tickets(engine, map, pi);

    let legal_moves = {
        let lm = engine.legal_moves(pi);
        LegalMoves {
            routes: lm.routes,
            draw_face_up: lm.draw_face_up,
            can_draw_deck: lm.can_draw_deck,
            can_draw_tickets: lm.can_draw_tickets,
        }
    };

    let seq = session.outbound_seq.get(player_id).copied().unwrap_or(0);

    Some(ttr_proto::PrivateState {
        seq,
        hand,
        tickets,
        offered_tickets,
        legal_moves,
    })
}

/// Biglietti offerti al giocatore: quelli del draft iniziale non ancora scelto
/// o, a metà partita, quelli della sua scelta in corso.
fn build_offered_tickets(
    engine: &ttr_core::Engine,
    map: &ttr_core::MapData,
    pi: usize,
) -> Vec<TicketInfo> {
    let phase = engine.phase();
    let mut offered = Vec::new();

    match phase {
        GamePhase::InitialDraft => {
            let player = engine.player(pi);
            let done = player.map(|p| p.initial_draft_done).unwrap_or(true);
            if !done && let Some(initial) = engine.initial_tickets_for(pi) {
                for tid in initial {
                    let points = map
                        .tickets
                        .iter()
                        .find(|t| &t.id == tid)
                        .map(|t| t.points)
                        .unwrap_or(0);
                    offered.push(TicketInfo {
                        id: tid.clone(),
                        points,
                        completed: false,
                    });
                }
            }
        }
        GamePhase::TicketChoice if engine.current_player() == pi => {
            for tid in engine.pending_tickets() {
                let points = map
                    .tickets
                    .iter()
                    .find(|t| &t.id == tid)
                    .map(|t| t.points)
                    .unwrap_or(0);
                offered.push(TicketInfo {
                    id: tid.clone(),
                    points,
                    completed: false,
                });
            }
        }
        _ => {}
    }

    offered.sort_by(|a, b| a.id.cmp(&b.id));
    offered
}

// ── FinalScoreInfo ───────────────────────────────────────────────────

pub(crate) fn build_final_scores(
    final_scores: &[ttr_core::event::FinalScore],
    _longest_path_players: &[usize],
    players: &[SessionPlayer],
    player_index: &BTreeMap<String, usize>,
) -> Vec<FinalScoreInfo> {
    // Mappa engine_index → player_id usando SessionPlayer.index.
    let index_to_id: BTreeMap<usize, &String> = player_index
        .iter()
        .filter_map(|(id, &session_pos)| {
            players
                .get(session_pos)
                .and_then(|p| p.index)
                .map(|ei| (ei, id))
        })
        .collect();

    final_scores
        .iter()
        .map(|fs| {
            let player_id = index_to_id
                .get(&fs.player_index)
                .map(|s| s.to_string())
                .unwrap_or_default();
            FinalScoreInfo {
                player_id,
                route_points: fs.route_points,
                ticket_completed: fs.ticket_completed,
                ticket_failed: fs.ticket_failed,
                longest_path_bonus: fs.longest_path_bonus,
                total: fs.total,
                winner: fs.winner,
            }
        })
        .collect()
}

// ── EngineError → ErrorReason ────────────────────────────────────────

pub(crate) fn engine_error_to_reason(err: &EngineError) -> ErrorReason {
    match err {
        EngineError::WrongState { .. } => ErrorReason::WrongState,
        EngineError::NotYourTurn { .. } => ErrorReason::NotYourTurn,
        EngineError::RouteAlreadyClaimed { .. } => ErrorReason::RouteAlreadyClaimed,
        EngineError::RouteBlockedDouble { .. } | EngineError::SamePlayerDouble { .. } => {
            ErrorReason::RouteBlockedDoubleVariant
        }
        EngineError::InsufficientCards { .. } => ErrorReason::InsufficientCards,
        EngineError::TicketChoiceBelowMin { .. }
        | EngineError::TicketChoiceTooMany { .. }
        | EngineError::TicketNotOffered { .. } => ErrorReason::TicketChoiceOutOfRange,
        EngineError::FaceUpIndexOutOfRange { .. }
        | EngineError::CardNotAvailable { .. }
        | EngineError::LocomotiveAsSecondCard
        | EngineError::DeckEmpty
        | EngineError::PlayerNotFound { .. }
        | EngineError::RouteNotFound { .. }
        | EngineError::InvalidRouteColor { .. }
        | EngineError::InvalidPayload { .. }
        | EngineError::DraftNotComplete => ErrorReason::InvalidAction,
        // Errori futuri del motore: azione rifiutata senza una ragione dedicata.
        _ => ErrorReason::InvalidAction,
    }
}

/// Costruisce un `actionRef` descrittivo per l'errore.
pub(crate) fn error_action_ref(err: &EngineError) -> Option<String> {
    match err {
        EngineError::RouteAlreadyClaimed { route_id } => Some(format!("CLAIM_ROUTE {}", route_id)),
        EngineError::RouteBlockedDouble { route_id }
        | EngineError::SamePlayerDouble { route_id, .. } => {
            Some(format!("CLAIM_ROUTE {}", route_id))
        }
        EngineError::InsufficientCards { route_id, .. } => {
            Some(format!("CLAIM_ROUTE {}", route_id))
        }
        EngineError::FaceUpIndexOutOfRange { index, .. } => {
            Some(format!("DRAW_TRAIN_CARD faceUp[{}]", index))
        }
        EngineError::TicketChoiceBelowMin { .. }
        | EngineError::TicketChoiceTooMany { .. }
        | EngineError::TicketNotOffered { .. } => Some("CHOOSE_TICKETS".into()),
        _ => None,
    }
}

// ── Test unitari ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ttr_core::error::EngineError;

    #[test]
    fn engine_error_to_reason_exhaustive() {
        // Ogni variante di EngineError mappa al ErrorReason corretto.

        // WrongState
        assert_eq!(
            engine_error_to_reason(&EngineError::WrongState {
                state: GamePhase::PlayerTurn
            }),
            ErrorReason::WrongState
        );

        // NotYourTurn
        assert_eq!(
            engine_error_to_reason(&EngineError::NotYourTurn {
                player_index: 0,
                current_player: 1
            }),
            ErrorReason::NotYourTurn
        );

        // RouteAlreadyClaimed
        assert_eq!(
            engine_error_to_reason(&EngineError::RouteAlreadyClaimed {
                route_id: "r1".into()
            }),
            ErrorReason::RouteAlreadyClaimed
        );

        // RouteBlockedDoubleVariant (RouteBlockedDouble + SamePlayerDouble)
        assert_eq!(
            engine_error_to_reason(&EngineError::RouteBlockedDouble {
                route_id: "r1".into()
            }),
            ErrorReason::RouteBlockedDoubleVariant
        );
        assert_eq!(
            engine_error_to_reason(&EngineError::SamePlayerDouble {
                route_id: "r1".into(),
                player: 0
            }),
            ErrorReason::RouteBlockedDoubleVariant
        );

        // InsufficientCards
        assert_eq!(
            engine_error_to_reason(&EngineError::InsufficientCards {
                route_id: "r1".into(),
                needed: 3
            }),
            ErrorReason::InsufficientCards
        );

        // TicketChoiceOutOfRange (BelowMin + TooMany + NotOffered)
        assert_eq!(
            engine_error_to_reason(&EngineError::TicketChoiceBelowMin { kept: 1, min: 2 }),
            ErrorReason::TicketChoiceOutOfRange
        );
        assert_eq!(
            engine_error_to_reason(&EngineError::TicketChoiceTooMany {
                kept: 5,
                available: 3
            }),
            ErrorReason::TicketChoiceOutOfRange
        );
        assert_eq!(
            engine_error_to_reason(&EngineError::TicketNotOffered {
                ticket_id: "t1".into()
            }),
            ErrorReason::TicketChoiceOutOfRange
        );

        // InvalidAction (tutti i restanti)
        for (err, label) in [
            (
                EngineError::FaceUpIndexOutOfRange { index: 5, max: 4 },
                "FaceUpIndexOutOfRange",
            ),
            (
                EngineError::LocomotiveAsSecondCard,
                "LocomotiveAsSecondCard",
            ),
            (
                EngineError::CardNotAvailable { index: 0 },
                "CardNotAvailable",
            ),
            (EngineError::DeckEmpty, "DeckEmpty"),
            (EngineError::PlayerNotFound { index: 42 }, "PlayerNotFound"),
            (
                EngineError::RouteNotFound {
                    route_id: "nx".into(),
                },
                "RouteNotFound",
            ),
            (
                EngineError::InvalidRouteColor {
                    color: "Magenta".into(),
                },
                "InvalidRouteColor",
            ),
            (
                EngineError::InvalidPayload {
                    reason: "test".into(),
                },
                "InvalidPayload",
            ),
            (EngineError::DraftNotComplete, "DraftNotComplete"),
        ] {
            assert_eq!(
                engine_error_to_reason(&err),
                ErrorReason::InvalidAction,
                "{label} → InvalidAction"
            );
        }
    }
}
