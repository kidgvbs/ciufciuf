//! Conversione core ↔ proto: `PublicState`, `PrivateState`, `ErrorReason`, `FinalScoreInfo`.

use std::collections::{BTreeMap, HashSet};

use ttr_core::error::{EngineError, GamePhase};
use ttr_core::{Action, ActionRecord, GameEvent};
use ttr_proto::{
    BorrowedRoute, CardColor, ClaimedRoute, ErrorReason, FinalScoreInfo, GameState, LegalMoves,
    PlayerSummary, RECENT_MOVES_MAX, RecentMove, StationPlacement, TicketInfo, TunnelDecisionMoves,
    TunnelOutcome, TunnelReveal,
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

    let tunnel_reveal = session
        .engine
        .as_ref()
        .and_then(|e| e.pending_tunnel())
        .map(|pending| TunnelReveal {
            route_id: pending.route_id.clone(),
            cards: pending.revealed.clone(),
            color: pending.color,
            extra_cost: pending.extra,
        });

    // `station_placements` è già in ordine di id città.
    let stations: Vec<StationPlacement> = session
        .engine
        .as_ref()
        .map(|e| {
            e.station_placements()
                .into_iter()
                .filter_map(|(city_id, owner)| {
                    session
                        .player_id_for_index(owner)
                        .map(|player_id| StationPlacement {
                            city_id: city_id.clone(),
                            player_id: player_id.to_string(),
                        })
                })
                .collect()
        })
        .unwrap_or_default();

    let recent_moves = session
        .engine
        .as_ref()
        .map(|e| recent_moves_from_log(e.action_log(), |i| session.player_id_for_index(i)))
        .unwrap_or_default();

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
        tunnel_reveal,
        stations,
        final_scores: session.final_scores.clone(),
        longest_path_player_ids: session.longest_path_player_ids.clone(),
        longest_path_route_ids: session.longest_path_route_ids.clone(),
        recent_moves,
    }
}

// ── Ultime mosse ─────────────────────────────────────────────────────

/// Una pescata di carte è "aperta" se la prima carta non ha chiuso il turno
/// (manca `DrawTurnComplete`): la seconda carta è un altro record dello stesso turno.
fn is_open_draw(record: &ActionRecord) -> bool {
    matches!(record.action, Action::DrawTrainCard { .. })
        && !record
            .events
            .iter()
            .any(|e| matches!(e, GameEvent::DrawTurnComplete { .. }))
}

/// Deriva le ultime `RECENT_MOVES_MAX` mosse dal log delle azioni, dalla più
/// vecchia alla più recente. Il draft iniziale è escluso; le carte pescate dal
/// mazzo non vengono mai esposte.
///
/// Scorre solo la coda del log: ogni mossa occupa al più due record (pescata di
/// due carte, biglietti + scelta, tunnel + decisione), quindi `2 * MAX` record
/// bastano; un record iniziale "orfano" (seguito di una mossa il cui primo record
/// è fuori finestra) viene ignorato, quindi se ne legge uno in più, e la finestra
/// viene estesa di uno se taglierebbe una mossa a metà.
fn recent_moves_from_log<'a>(
    log: &[ActionRecord],
    player_id: impl Fn(usize) -> Option<&'a str>,
) -> Vec<RecentMove> {
    let mut start = log.len().saturating_sub(2 * RECENT_MOVES_MAX + 1);
    if start > 0
        && (is_open_draw(&log[start - 1])
            || matches!(
                log[start].action,
                Action::ChooseTickets { .. } | Action::TunnelDecision { .. }
            ))
    {
        start -= 1;
    }

    let mut moves: Vec<RecentMove> = Vec::new();
    // Indice del giocatore se l'ultima voce è una pescata di carte ancora aperta.
    let mut open_draw: Option<usize> = None;
    for record in &log[start..] {
        let was_open_draw = open_draw.take();
        let Some(pid) = player_id(record.action.player_index()) else {
            continue;
        };
        let player_id = pid.to_string();
        match &record.action {
            Action::DrawTrainCard { player_index, .. } => {
                let (mut deck, mut face_up) = (0u32, Vec::new());
                for event in &record.events {
                    match event {
                        GameEvent::CardDrawnFromDeck { .. } => deck += 1,
                        GameEvent::CardDrawnFromFaceUp { card, .. } => face_up.push(*card),
                        _ => {}
                    }
                }
                if was_open_draw == Some(*player_index) {
                    if let Some(RecentMove::DrawCards {
                        deck: d,
                        face_up: f,
                        ..
                    }) = moves.last_mut()
                    {
                        *d += deck;
                        f.extend(face_up);
                    }
                } else {
                    moves.push(RecentMove::DrawCards {
                        player_id,
                        deck,
                        face_up,
                    });
                }
                if is_open_draw(record) {
                    open_draw = Some(*player_index);
                }
            }
            Action::DrawTickets { .. } => moves.push(RecentMove::DrawTickets {
                player_id,
                kept: None,
            }),
            Action::ChooseTickets { .. } => {
                // Solo la scelta mid-game completa una voce `DRAW_TICKETS`; quelle del
                // draft iniziale non hanno una voce pendente.
                let kept = record.events.iter().find_map(|e| match e {
                    GameEvent::TicketsChosen { kept, .. } => u32::try_from(kept.len()).ok(),
                    _ => None,
                });
                if let Some(RecentMove::DrawTickets {
                    player_id: pending_pid,
                    kept: slot @ None,
                }) = moves.last_mut()
                    && *pending_pid == player_id
                {
                    *slot = kept;
                }
            }
            Action::ClaimRoute { route_id, .. } => {
                let claimed_points = record.events.iter().find_map(|e| match e {
                    GameEvent::RouteClaimed { points, .. } => Some(*points),
                    _ => None,
                });
                let revealed = record.events.iter().find_map(|e| match e {
                    GameEvent::TunnelRevealed {
                        cards,
                        color,
                        extra_cost,
                        ..
                    } => Some((cards, *color, *extra_cost)),
                    _ => None,
                });
                match (revealed, claimed_points) {
                    (Some((cards, color, extra_cost)), claimed) => {
                        moves.push(RecentMove::Tunnel {
                            player_id,
                            route_id: route_id.clone(),
                            revealed: cards.clone(),
                            color,
                            extra_cost,
                            outcome: if claimed.is_some() {
                                TunnelOutcome::Claimed
                            } else {
                                TunnelOutcome::Pending
                            },
                        });
                    }
                    (None, Some(points)) => moves.push(RecentMove::ClaimRoute {
                        player_id,
                        route_id: route_id.clone(),
                        points,
                    }),
                    (None, None) => {}
                }
            }
            Action::TunnelDecision { .. } => {
                let declined = record
                    .events
                    .iter()
                    .any(|e| matches!(e, GameEvent::TunnelDeclined { .. }));
                if let Some(RecentMove::Tunnel {
                    player_id: pending_pid,
                    outcome: outcome @ TunnelOutcome::Pending,
                    ..
                }) = moves.last_mut()
                    && *pending_pid == player_id
                {
                    *outcome = if declined {
                        TunnelOutcome::Declined
                    } else {
                        TunnelOutcome::Paid
                    };
                }
            }
            Action::BuildStation { city_id, .. } => moves.push(RecentMove::BuildStation {
                player_id,
                city_id: city_id.clone(),
            }),
        }
    }

    let excess = moves.len().saturating_sub(RECENT_MOVES_MAX);
    moves.drain(..excess);
    moves
}

fn game_phase_to_game_state(phase: GamePhase) -> GameState {
    match phase {
        GamePhase::InitialDraft => GameState::InitialDraft,
        GamePhase::PlayerTurn => GameState::PlayerTurn,
        GamePhase::SecondCardDraw => GameState::SecondCardDraw,
        GamePhase::TunnelResolution => GameState::TunnelResolution,
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

    // Senza motore (pre-start) il ruleset non è noto: default 45 treni.
    let max_trains: u32 = session
        .engine
        .as_ref()
        .map_or(45, |engine| engine.trains_per_player());

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
                stations_remaining: session
                    .engine
                    .as_ref()
                    .map_or(0, |engine| engine.stations_remaining(engine_idx)),
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
            station_cities: lm.station_cities,
            station_cost: lm.station_cost,
            tunnel: lm.tunnel.map(|t| TunnelDecisionMoves {
                route_id: t.route_id,
                extra_cost: t.extra_cost,
                color: t.color,
                can_pay: t.can_pay,
            }),
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
                station_points: fs.station_points,
                total: fs.total,
                winner: fs.winner,
                borrowed_routes: fs
                    .borrowed_routes
                    .iter()
                    .map(|(city_id, route_id)| BorrowedRoute {
                        city_id: city_id.clone(),
                        route_id: route_id.clone(),
                    })
                    .collect(),
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
        EngineError::InsufficientCards { .. }
        | EngineError::InsufficientStationCards { .. }
        | EngineError::InsufficientTunnelCards { .. } => ErrorReason::InsufficientCards,
        EngineError::TicketChoiceBelowMin { .. }
        | EngineError::TicketChoiceTooMany { .. }
        | EngineError::TicketNotOffered { .. } => ErrorReason::TicketChoiceOutOfRange,
        EngineError::FaceUpIndexOutOfRange { .. }
        | EngineError::CardNotAvailable { .. }
        | EngineError::LocomotiveAsSecondCard
        | EngineError::DeckEmpty
        | EngineError::PlayerNotFound { .. }
        | EngineError::RouteNotFound { .. }
        | EngineError::CityNotFound { .. }
        | EngineError::StationCityOccupied { .. }
        | EngineError::NoStationsLeft
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
        EngineError::InsufficientStationCards { .. } => Some("BUILD_STATION".into()),
        EngineError::InsufficientTunnelCards { .. } => Some("TUNNEL_DECISION".into()),
        EngineError::CityNotFound { .. }
        | EngineError::StationCityOccupied { .. }
        | EngineError::NoStationsLeft => Some("BUILD_STATION".into()),
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
        assert_eq!(
            engine_error_to_reason(&EngineError::InsufficientStationCards { needed: 2 }),
            ErrorReason::InsufficientCards
        );
        assert_eq!(
            engine_error_to_reason(&EngineError::InsufficientTunnelCards {
                route_id: "r1".into(),
                needed: 1
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
                EngineError::InvalidPayload {
                    reason: "test".into(),
                },
                "InvalidPayload",
            ),
            (EngineError::DraftNotComplete, "DraftNotComplete"),
            (
                EngineError::CityNotFound {
                    city_id: "atlantis".into(),
                },
                "CityNotFound",
            ),
            (
                EngineError::StationCityOccupied {
                    city_id: "paris".into(),
                },
                "StationCityOccupied",
            ),
            (EngineError::NoStationsLeft, "NoStationsLeft"),
        ] {
            assert_eq!(
                engine_error_to_reason(&err),
                ErrorReason::InvalidAction,
                "{label} → InvalidAction"
            );
        }
    }

    // ── recent_moves_from_log ────────────────────────────────────────

    use ttr_core::DrawSource;

    fn pid(i: usize) -> Option<&'static str> {
        ["p0", "p1", "p2"].get(i).copied()
    }

    fn rec(action: Action, events: Vec<GameEvent>) -> ActionRecord {
        ActionRecord { action, events }
    }

    fn deck_draw(p: usize, card: CardColor, done: bool) -> ActionRecord {
        let mut events = vec![GameEvent::CardDrawnFromDeck {
            player_index: p,
            card,
        }];
        if done {
            events.push(GameEvent::DrawTurnComplete { player_index: p });
        }
        rec(
            Action::DrawTrainCard {
                player_index: p,
                source: DrawSource::Deck,
            },
            events,
        )
    }

    fn face_up_draw(p: usize, card: CardColor, done: bool) -> ActionRecord {
        let mut events = vec![
            GameEvent::CardDrawnFromFaceUp {
                player_index: p,
                index: 2,
                card,
            },
            GameEvent::FaceUpRevealed {
                index: 2,
                card: CardColor::Black,
            },
        ];
        if done {
            events.push(GameEvent::DrawTurnComplete { player_index: p });
        }
        rec(
            Action::DrawTrainCard {
                player_index: p,
                source: DrawSource::FaceUp { index: 2 },
            },
            events,
        )
    }

    fn claim(p: usize, route: &str, events: Vec<GameEvent>) -> ActionRecord {
        rec(
            Action::ClaimRoute {
                player_index: p,
                route_id: route.into(),
                cards_discarded: vec![],
            },
            events,
        )
    }

    fn route_claimed(p: usize, route: &str, points: u32) -> GameEvent {
        GameEvent::RouteClaimed {
            route_id: route.into(),
            player_index: p,
            cards_used: vec![CardColor::Red],
            points,
            trains_placed: 2,
        }
    }

    fn tunnel_revealed(p: usize, route: &str, extra: u32) -> GameEvent {
        GameEvent::TunnelRevealed {
            player_index: p,
            route_id: route.into(),
            cards: vec![CardColor::Red, CardColor::Locomotive],
            color: CardColor::Red,
            extra_cost: extra,
        }
    }

    fn tunnel_decision(p: usize, pay: bool, events: Vec<GameEvent>) -> ActionRecord {
        rec(
            Action::TunnelDecision {
                player_index: p,
                pay,
                cards_discarded: vec![],
            },
            events,
        )
    }

    fn recent(log: &[ActionRecord]) -> Vec<RecentMove> {
        recent_moves_from_log(log, pid)
    }

    #[test]
    fn recent_moves_merges_two_draws_and_hides_deck_cards() {
        let log = [
            deck_draw(0, CardColor::Green, false),
            face_up_draw(0, CardColor::Blue, true),
            face_up_draw(1, CardColor::Locomotive, true),
        ];
        assert_eq!(
            recent(&log),
            vec![
                RecentMove::DrawCards {
                    player_id: "p0".into(),
                    deck: 1,
                    face_up: vec![CardColor::Blue],
                },
                RecentMove::DrawCards {
                    player_id: "p1".into(),
                    deck: 0,
                    face_up: vec![CardColor::Locomotive],
                },
            ]
        );
        let json = serde_json::to_string(&recent(&log)).unwrap();
        assert!(!json.contains("Green"), "deck card leaked: {json}");
        assert!(!json.contains("\"card\""), "card field leaked: {json}");
    }

    #[test]
    fn recent_moves_draw_in_progress_is_single_entry() {
        let log = [deck_draw(0, CardColor::Green, false)];
        assert_eq!(
            recent(&log),
            vec![RecentMove::DrawCards {
                player_id: "p0".into(),
                deck: 1,
                face_up: vec![],
            }]
        );
    }

    #[test]
    fn recent_moves_draft_excluded_and_tickets_pending_then_kept() {
        let choose = |p: usize, kept: usize| {
            rec(
                Action::ChooseTickets {
                    player_index: p,
                    keep: vec![],
                },
                vec![GameEvent::TicketsChosen {
                    player_index: p,
                    kept: (0..kept).map(|i| format!("t{i}")).collect(),
                    discarded: vec![],
                }],
            )
        };
        let draw_tickets = rec(
            Action::DrawTickets { player_index: 1 },
            vec![GameEvent::TicketsOffered {
                player_index: 1,
                ticket_ids: vec!["a".into(), "b".into(), "c".into()],
            }],
        );

        // Solo draft: nessuna voce.
        assert!(recent(&[choose(0, 3), choose(1, 2)]).is_empty());

        // Pendente.
        let mut log = vec![choose(0, 3), choose(1, 2), draw_tickets];
        assert_eq!(
            recent(&log),
            vec![RecentMove::DrawTickets {
                player_id: "p1".into(),
                kept: None,
            }]
        );

        // Scelta mid-game: stessa voce con `kept`.
        log.push(choose(1, 2));
        assert_eq!(
            recent(&log),
            vec![RecentMove::DrawTickets {
                player_id: "p1".into(),
                kept: Some(2),
            }]
        );
    }

    #[test]
    fn recent_moves_claim_route_and_station() {
        let log = [
            claim(0, "r1", vec![route_claimed(0, "r1", 4)]),
            rec(
                Action::BuildStation {
                    player_index: 1,
                    city_id: "paris".into(),
                    cards_discarded: vec![],
                },
                vec![GameEvent::StationBuilt {
                    player_index: 1,
                    city_id: "paris".into(),
                    cards_used: vec![CardColor::Red],
                }],
            ),
        ];
        assert_eq!(
            recent(&log),
            vec![
                RecentMove::ClaimRoute {
                    player_id: "p0".into(),
                    route_id: "r1".into(),
                    points: 4,
                },
                RecentMove::BuildStation {
                    player_id: "p1".into(),
                    city_id: "paris".into(),
                },
            ]
        );
    }

    fn tunnel_entry(outcome: TunnelOutcome, extra_cost: u32) -> RecentMove {
        RecentMove::Tunnel {
            player_id: "p0".into(),
            route_id: "t1".into(),
            revealed: vec![CardColor::Red, CardColor::Locomotive],
            color: CardColor::Red,
            extra_cost,
            outcome,
        }
    }

    #[test]
    fn recent_moves_tunnel_claimed_pending_paid_declined() {
        // Costo extra 0: rotta presa subito.
        let claimed = claim(
            0,
            "t1",
            vec![tunnel_revealed(0, "t1", 0), route_claimed(0, "t1", 2)],
        );
        assert_eq!(
            recent(std::slice::from_ref(&claimed)),
            vec![tunnel_entry(TunnelOutcome::Claimed, 0)]
        );

        // Costo extra > 0: in attesa della decisione.
        let pending = claim(0, "t1", vec![tunnel_revealed(0, "t1", 2)]);
        assert_eq!(
            recent(std::slice::from_ref(&pending)),
            vec![tunnel_entry(TunnelOutcome::Pending, 2)]
        );

        // Pagato: stessa voce aggiornata, nessuna voce nuova.
        let paid = tunnel_decision(0, true, vec![route_claimed(0, "t1", 2)]);
        assert_eq!(
            recent(&[pending.clone(), paid]),
            vec![tunnel_entry(TunnelOutcome::Paid, 2)]
        );

        // Rinuncia.
        let declined = tunnel_decision(
            0,
            false,
            vec![GameEvent::TunnelDeclined {
                player_index: 0,
                route_id: "t1".into(),
            }],
        );
        assert_eq!(
            recent(&[pending, declined]),
            vec![tunnel_entry(TunnelOutcome::Declined, 2)]
        );
    }

    #[test]
    fn recent_moves_capped_at_max_keeping_latest() {
        // 20 mosse da una carta in totale: route r0..r19.
        let log: Vec<ActionRecord> = (0..20)
            .map(|i| {
                let route = format!("r{i}");
                claim(i % 3, &route, vec![route_claimed(i % 3, &route, 1)])
            })
            .collect();
        let moves = recent(&log);
        assert_eq!(moves.len(), RECENT_MOVES_MAX);
        let route_ids: Vec<&str> = moves
            .iter()
            .map(|m| match m {
                RecentMove::ClaimRoute { route_id, .. } => route_id.as_str(),
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(
            route_ids,
            ["r12", "r13", "r14", "r15", "r16", "r17", "r18", "r19"]
        );
    }

    #[test]
    fn recent_moves_window_never_splits_two_record_moves() {
        // Log lungo di sole pescate da due record: la finestra di coda non deve
        // produrre voci orfane né perdere carte, per qualunque lunghezza.
        for turns in 1..=24 {
            let mut log = Vec::new();
            for t in 0..turns {
                let p = t % 3;
                log.push(deck_draw(p, CardColor::Green, false));
                log.push(face_up_draw(p, CardColor::Blue, true));
            }
            let moves = recent(&log);
            assert_eq!(moves.len(), turns.min(RECENT_MOVES_MAX), "turns={turns}");
            for m in &moves {
                assert!(
                    matches!(m, RecentMove::DrawCards { deck: 1, face_up, .. } if face_up.len() == 1),
                    "turns={turns}: {m:?}"
                );
            }
        }
        // Tunnel + decisione in coda a un log lungo.
        let mut log = vec![claim(0, "x", vec![route_claimed(0, "x", 1)]); 16];
        log.push(claim(0, "t1", vec![tunnel_revealed(0, "t1", 2)]));
        log.push(tunnel_decision(0, true, vec![route_claimed(0, "t1", 2)]));
        let moves = recent(&log);
        assert_eq!(moves.len(), RECENT_MOVES_MAX);
        assert_eq!(moves.last(), Some(&tunnel_entry(TunnelOutcome::Paid, 2)));

        // Decisione di tunnel all'inizio della finestra di coda: la voce aggiornata
        // resta fuori dalle ultime mosse e non genera voci orfane.
        let mut log = vec![claim(0, "t1", vec![tunnel_revealed(0, "t1", 2)])];
        log.push(tunnel_decision(0, true, vec![route_claimed(0, "t1", 2)]));
        log.extend((0..16).map(|i| {
            let route = format!("r{i}");
            claim(1, &route, vec![route_claimed(1, &route, 1)])
        }));
        let moves = recent(&log);
        assert_eq!(moves.len(), RECENT_MOVES_MAX);
        assert!(
            moves
                .iter()
                .all(|m| matches!(m, RecentMove::ClaimRoute { .. }))
        );
    }
}
