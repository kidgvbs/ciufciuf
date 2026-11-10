//! CiufCiuf! — Tipi condivisi del contratto WebSocket.
//!
//! Questo crate definisce i tipi serializzabili (serde) che costituiscono
//! il contratto tra server Rust e client TypeScript. L'export TypeScript
//! via `ts-rs` garantisce che i tipi non vadano in drift.
//!
//! # Convenzioni di identificazione
//!
//! - **CityId**: lowercase snake case, es. `"new_york"`, `"salt_lake_city"`.
//! - **RouteId**: `{cityA}_{cityB}_{n}` con città in ordine lessicografico,
//!   suffisso `_1` / `_2` per le doppie, es. `"denver_phoenix_1"`.
//!   (Nuovo: `_1` anche per le rotte singole, così il formato è uniforme.)
//! - **TicketId**: `{cityA}_{cityB}` con città in ordine lessicografico,
//!   es. `"los_angeles_new_york"`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ── Colori ──────────────────────────────────────────────────────────

/// Colore di una carta treno. `Locomotive` è il jolly / rainbow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export)]
pub enum CardColor {
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    Black,
    White,
    /// Jolly — sostituisce qualunque colore, conta come locomotiva per le regole di pesca.
    Locomotive,
}

/// Colore giocatore (plancia + trenini).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export)]
pub enum PlayerColor {
    Red,
    Blue,
    Green,
    Yellow,
    Black,
}

// ── Stati di gioco ──────────────────────────────────────────────────

/// Macchina a stati del server.
///
/// L'enum è marcato `#[non_exhaustive]` per consentire l'aggiunta di stati
/// futuri (es. `WAITING_FOR_PLAYERS` e stati Europe) senza breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export)]
#[non_exhaustive]
pub enum GameState {
    /// Lobby: in attesa di giocatori. I client collegati vedono i join in tempo reale.
    WaitingForPlayers,
    /// Draft iniziale biglietti — tutti in parallelo.
    InitialDraft,
    /// Turno normale: il giocatore sceglie una delle 3 (o 4 in Europe) azioni.
    PlayerTurn,
    /// Seconda pescata carta (solo se la prima NON era locomotiva scoperta).
    SecondCardDraw,
    /// Scelta biglietti dopo pesca mid-game.
    TicketChoice,
    /// Ultimo giro: banner attivo, contatore turni rimasti.
    LastRound,
    /// Europe: risoluzione tunnel dopo `CLAIM_ROUTE` su rotta tunnel.
    TunnelResolution,
    /// Europe: assegnazione stazioni a fine partita (server-side).
    StationScoring,
    /// Partita terminata, schermata punteggi.
    GameOver,
}

// ── Messaggi inbound (client → server) ──────────────────────────────

/// Azione inbound (client → server). Messaggio inviato su `game/{gameId}/action`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "actionType")]
#[ts(export)]
pub enum InboundAction {
    /// Pescare una carta treno dal mazzo coperto.
    #[serde(rename = "DRAW_TRAIN_CARD")]
    DrawTrainCard(DrawCardPayload),
    /// Pescare nuovi biglietti mid-game (azione nº 3 del turno).
    #[serde(rename = "DRAW_TICKETS")]
    DrawTickets(DrawTicketsPayload),
    /// Scegliere quali biglietti tenere (initial draft o mid-game).
    #[serde(rename = "CHOOSE_TICKETS")]
    ChooseTickets(ChooseTicketsPayload),
    /// Reclamare una rotta.
    #[serde(rename = "CLAIM_ROUTE")]
    ClaimRoute(ClaimRoutePayload),
    /// Richiedere lo snapshot completo dopo reconnect.
    #[serde(rename = "REQUEST_SNAPSHOT")]
    RequestSnapshot(RequestSnapshotPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DrawCardPayload {
    pub player_id: String,
    pub source: DrawSource,
    /// Numero di sequenza client per idempotenza: un valore minore o uguale
    /// all'ultimo ricevuto è un duplicato scartato. Assente = nessuna deduplica.
    #[ts(optional)]
    pub client_seq: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum DrawSource {
    Deck,
    FaceUp { index: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DrawTicketsPayload {
    pub player_id: String,
    #[ts(optional)]
    pub client_seq: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChooseTicketsPayload {
    pub player_id: String,
    /// Id dei biglietti da tenere (in ordine lessicografico).
    pub keep: Vec<TicketId>,
    #[ts(optional)]
    pub client_seq: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ClaimRoutePayload {
    pub player_id: String,
    pub route_id: RouteId,
    /// Suggerimento: il server valida e risolve il pagamento (locomotive automatiche).
    pub cards_discarded: Vec<CardColor>,
    #[ts(optional)]
    pub client_seq: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RequestSnapshotPayload {
    pub player_id: String,
    #[ts(optional)]
    pub client_seq: Option<u64>,
}

// ── Envelope outbound ───────────────────────────────────────────────

/// Messaggio outbound inviato sul WebSocket.
///
/// L'envelope usa adjacent tagging: `{"type":"PUBLIC_STATE","payload":{…}}`.
/// Ogni canale di notifica è una variante di questo enum.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "payload")]
#[ts(export)]
pub enum ServerMessage {
    /// Broadcast pubblico a tutti i client (giocatori e spettatori).
    #[serde(rename = "PUBLIC_STATE")]
    PublicState(PublicState),
    /// Unicast privato a un singolo giocatore.
    #[serde(rename = "PRIVATE_STATE")]
    PrivateState(PrivateState),
    /// Errore su un'azione, inviato solo al giocatore interessato.
    #[serde(rename = "ERROR")]
    Error(ErrorPayload),
    /// Snapshot completo dopo (re)connect o su richiesta.
    #[serde(rename = "SNAPSHOT")]
    Snapshot(Snapshot),
}

// ── Messaggi outbound: payload ──────────────────────────────────────

/// Broadcast pubblico: stato della partita visibile a tutti.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PublicState {
    pub game_id: String,
    pub protocol_version: u32,
    pub state: GameState,
    pub current_turn_player_id: Option<String>,
    pub last_round: bool,
    pub last_round_trigger_player_id: Option<String>,
    /// Carte scoperte: `None` = slot vuoto. Gli indici di `drawFaceUp` restano allineati.
    pub visible_deck: Vec<Option<CardColor>>,
    /// Numero di carte nel mazzo coperto (solo la pila pesca, NON gli scarti).
    pub deck_size: u32,
    pub claimed_routes: Vec<ClaimedRoute>,
    pub players_summary: Vec<PlayerSummary>,
    /// Europe: in `TUNNEL_RESOLUTION`, le carte rivelate e costo extra.
    pub tunnel_reveal: Option<TunnelReveal>,
    /// Europe: stazioni costruite.
    pub stations: Vec<StationPlacement>,
    /// Punteggi finali: `None` finché la partita non è terminata.
    pub final_scores: Option<Vec<FinalScoreInfo>>,
    /// Vincitori del longest path: id giocatori, non indici.
    pub longest_path_player_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ClaimedRoute {
    pub route_id: RouteId,
    pub claimed_by_player_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PlayerSummary {
    pub player_id: String,
    pub name: String,
    pub color: PlayerColor,
    pub trains_remaining: u32,
    pub stations_remaining: u32,
    pub score: i32,
    pub cards_hand_count: u32,
    pub tickets_count: u32,
    /// `true` se il giocatore ha ≥1 connessione WS attiva (presence runtime).
    pub connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TunnelReveal {
    pub route_id: RouteId,
    pub cards: Vec<CardColor>,
    pub extra_cost: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StationPlacement {
    pub city_id: CityId,
    pub player_id: String,
}

/// Punteggio finale per giocatore.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FinalScoreInfo {
    pub player_id: String,
    pub route_points: i32,
    pub ticket_completed: i32,
    pub ticket_failed: i32,
    pub longest_path_bonus: i32,
    pub total: i32,
    /// Vincitore (pari merito: più vincitori). Calcolato dal motore con gli
    /// spareggi: totale, biglietti completati, bonus percorso più lungo.
    #[serde(default)]
    pub winner: bool,
}

/// Broadcast unicast privato: stato visibile solo al giocatore interessato.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PrivateState {
    /// Numero di sequenza outbound: cresce a ogni messaggio privato per il giocatore.
    pub seq: u64,
    pub hand: Vec<CardColor>,
    pub tickets: Vec<TicketInfo>,
    /// Biglietti attualmente offerti per la scelta.
    pub offered_tickets: Vec<TicketInfo>,
    pub legal_moves: LegalMoves,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TicketInfo {
    pub id: TicketId,
    pub points: u32,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LegalMoves {
    pub routes: Vec<RouteId>,
    pub draw_face_up: Vec<usize>,
    pub can_draw_deck: bool,
    pub can_draw_tickets: bool,
}

/// Payload di errore (dentro `ServerMessage::Error`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ErrorPayload {
    pub seq: u64,
    pub reason: ErrorReason,
    pub action_ref: Option<String>,
    /// Messaggio leggibile (italiano) per l'utente, se disponibile.
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export)]
pub enum ErrorReason {
    // Errori di gioco (da EngineError)
    InsufficientCards,
    RouteAlreadyClaimed,
    RouteBlockedDoubleVariant,
    InvalidAction,
    NotYourTurn,
    WrongState,
    TicketChoiceOutOfRange,
    // Errori server-level
    GameNotFound,
    Unauthorized,
    GameFull,
    NameTaken,
}

/// Snapshot completo dopo (re)connect.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Snapshot {
    pub protocol_version: u32,
    pub public: PublicState,
    /// `None` per gli spettatori (table view senza `playerId`), `Some` per i giocatori.
    pub private: Option<PrivateState>,
}

// ── Newtype id ──────────────────────────────────────────────────────

/// Identificatore città. Forma: lowercase snake (`"new_york"`, `"salt_lake_city"`).
pub type CityId = String;

/// Identificatore rotta. Forma: `{cityA}_{cityB}_{n}` con città in ordine lessicografico.
/// Le rotte singole usano `_1`; le doppie usano `_1` e `_2`.
pub type RouteId = String;

/// Identificatore biglietto. Forma: `{cityA}_{cityB}` in ordine lessicografico.
pub type TicketId = String;

// ── Test round-trip serde ───────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_card_color() {
        let vals = [
            CardColor::Red,
            CardColor::Orange,
            CardColor::Yellow,
            CardColor::Green,
            CardColor::Blue,
            CardColor::Purple,
            CardColor::Black,
            CardColor::White,
            CardColor::Locomotive,
        ];
        for v in vals {
            let json = serde_json::to_string(&v).unwrap();
            let back: CardColor = serde_json::from_str(&json).unwrap();
            assert_eq!(v, back);
        }
    }

    #[test]
    fn roundtrip_player_color() {
        let vals = [
            PlayerColor::Red,
            PlayerColor::Blue,
            PlayerColor::Green,
            PlayerColor::Yellow,
            PlayerColor::Black,
        ];
        for v in vals {
            let json = serde_json::to_string(&v).unwrap();
            let back: PlayerColor = serde_json::from_str(&json).unwrap();
            assert_eq!(v, back);
        }
    }

    #[test]
    fn roundtrip_game_state() {
        let json = serde_json::to_string(&GameState::PlayerTurn).unwrap();
        assert_eq!(json, "\"PLAYER_TURN\"");
        let back: GameState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, GameState::PlayerTurn);
    }

    #[test]
    fn roundtrip_game_state_waiting() {
        let json = serde_json::to_string(&GameState::WaitingForPlayers).unwrap();
        assert_eq!(json, "\"WAITING_FOR_PLAYERS\"");
        let back: GameState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, GameState::WaitingForPlayers);
    }

    #[test]
    fn roundtrip_inbound_draw() {
        let msg = InboundAction::DrawTrainCard(DrawCardPayload {
            player_id: "p1".into(),
            source: DrawSource::FaceUp { index: 2 },
            client_seq: Some(42),
        });
        let json = serde_json::to_string(&msg).unwrap();
        let back: InboundAction = serde_json::from_str(&json).unwrap();
        match back {
            InboundAction::DrawTrainCard(p) => {
                assert_eq!(p.player_id, "p1");
                assert!(matches!(p.source, DrawSource::FaceUp { index: 2 }));
                assert_eq!(p.client_seq, Some(42));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn roundtrip_inbound_draw_no_seq() {
        let msg = InboundAction::DrawTrainCard(DrawCardPayload {
            player_id: "p1".into(),
            source: DrawSource::Deck,
            client_seq: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        // client_seq=None non deve apparire nel JSON
        assert!(!json.contains("clientSeq"));
        assert!(json.contains("DRAW_TRAIN_CARD"));
        let _back: InboundAction = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn roundtrip_inbound_choose() {
        let msg = InboundAction::ChooseTickets(ChooseTicketsPayload {
            player_id: "p2".into(),
            keep: vec!["denver_phoenix".into(), "seattle_new_york".into()],
            client_seq: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        let back: InboundAction = serde_json::from_str(&json).unwrap();
        match back {
            InboundAction::ChooseTickets(p) => assert_eq!(p.keep.len(), 2),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn roundtrip_inbound_draw_tickets() {
        let msg = InboundAction::DrawTickets(DrawTicketsPayload {
            player_id: "p2".into(),
            client_seq: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("DRAW_TICKETS"));
        let back: InboundAction = serde_json::from_str(&json).unwrap();
        match back {
            InboundAction::DrawTickets(p) => assert_eq!(p.player_id, "p2"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn roundtrip_inbound_request_snapshot() {
        let msg = InboundAction::RequestSnapshot(RequestSnapshotPayload {
            player_id: "p3".into(),
            client_seq: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        let _back: InboundAction = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn roundtrip_public_state() {
        let state = PublicState {
            game_id: "g1".into(),
            protocol_version: 1,
            state: GameState::PlayerTurn,
            current_turn_player_id: Some("p1".into()),
            last_round: false,
            last_round_trigger_player_id: None,
            visible_deck: vec![
                Some(CardColor::Red),
                Some(CardColor::Blue),
                Some(CardColor::Locomotive),
            ],
            deck_size: 0,
            claimed_routes: vec![],
            players_summary: vec![],
            tunnel_reveal: None,
            stations: vec![],
            final_scores: None,
            longest_path_player_ids: None,
        };
        let json = serde_json::to_string(&state).unwrap();
        let back: PublicState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.game_id, "g1");
        assert_eq!(back.visible_deck.len(), 3);
        assert!(back.final_scores.is_none());
    }

    #[test]
    fn roundtrip_public_state_final_scores() {
        let state = PublicState {
            game_id: "g1".into(),
            protocol_version: 1,
            state: GameState::GameOver,
            current_turn_player_id: None,
            last_round: false,
            last_round_trigger_player_id: None,
            visible_deck: vec![],
            deck_size: 0,
            claimed_routes: vec![],
            players_summary: vec![],
            tunnel_reveal: None,
            stations: vec![],
            final_scores: Some(vec![FinalScoreInfo {
                player_id: "p1".into(),
                route_points: 42,
                ticket_completed: 30,
                ticket_failed: -10,
                longest_path_bonus: 10,
                total: 72,
                winner: true,
            }]),
            longest_path_player_ids: Some(vec!["p1".into()]),
        };
        let json = serde_json::to_string(&state).unwrap();
        let back: PublicState = serde_json::from_str(&json).unwrap();
        let scores = back.final_scores.unwrap();
        assert_eq!(scores[0].total, 72);
        assert_eq!(back.longest_path_player_ids.unwrap(), vec!["p1"]);
    }

    #[test]
    fn roundtrip_public_state_visible_deck_none() {
        let state = PublicState {
            game_id: "g1".into(),
            protocol_version: 1,
            state: GameState::PlayerTurn,
            current_turn_player_id: Some("p1".into()),
            last_round: false,
            last_round_trigger_player_id: None,
            visible_deck: vec![Some(CardColor::Red), None, Some(CardColor::Blue)],
            deck_size: 0,
            claimed_routes: vec![],
            players_summary: vec![],
            tunnel_reveal: None,
            stations: vec![],
            final_scores: None,
            longest_path_player_ids: None,
        };
        let json = serde_json::to_string(&state).unwrap();
        let back: PublicState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.visible_deck.len(), 3);
        assert!(back.visible_deck[1].is_none());
    }

    #[test]
    fn roundtrip_private_state() {
        let state = PrivateState {
            seq: 42,
            hand: vec![CardColor::Red, CardColor::Blue, CardColor::Locomotive],
            tickets: vec![TicketInfo {
                id: "denver_phoenix".into(),
                points: 11,
                completed: false,
            }],
            offered_tickets: vec![],
            legal_moves: LegalMoves {
                routes: vec!["denver_phoenix_1".into()],
                draw_face_up: vec![0, 2],
                can_draw_deck: true,
                can_draw_tickets: true,
            },
        };
        let json = serde_json::to_string(&state).unwrap();
        let back: PrivateState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.seq, 42);
        assert_eq!(back.hand.len(), 3);
        assert!(back.legal_moves.can_draw_deck);
        assert!(back.offered_tickets.is_empty());
    }

    #[test]
    fn error_reason_deser() {
        let json = "\"INSUFFICIENT_CARDS\"";
        let reason: ErrorReason = serde_json::from_str(json).unwrap();
        assert!(matches!(reason, ErrorReason::InsufficientCards));
    }

    #[test]
    fn error_reason_new_variants() {
        for json in [
            "\"GAME_NOT_FOUND\"",
            "\"UNAUTHORIZED\"",
            "\"GAME_FULL\"",
            "\"NAME_TAKEN\"",
        ] {
            let reason: ErrorReason = serde_json::from_str(json).unwrap();
            let back_str = serde_json::to_string(&reason).unwrap();
            assert_eq!(back_str, json, "roundtrip failed for {}", json);
        }
    }

    #[test]
    fn roundtrip_server_message_public() {
        let msg = ServerMessage::PublicState(PublicState {
            game_id: "g1".into(),
            protocol_version: 1,
            state: GameState::WaitingForPlayers,
            current_turn_player_id: None,
            last_round: false,
            last_round_trigger_player_id: None,
            visible_deck: vec![],
            deck_size: 0,
            claimed_routes: vec![],
            players_summary: vec![],
            tunnel_reveal: None,
            stations: vec![],
            final_scores: None,
            longest_path_player_ids: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"PUBLIC_STATE\""));
        assert!(json.contains("\"payload\""));
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::PublicState(ps) => {
                assert_eq!(ps.game_id, "g1");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn roundtrip_server_message_error() {
        let msg = ServerMessage::Error(ErrorPayload {
            seq: 7,
            reason: ErrorReason::InsufficientCards,
            action_ref: Some("CLAIM_ROUTE denver_phoenix_1".into()),
            detail: Some("carte insufficienti".into()),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"ERROR\""));
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::Error(ep) => {
                assert_eq!(ep.seq, 7);
                assert_eq!(ep.action_ref.unwrap(), "CLAIM_ROUTE denver_phoenix_1");
                assert_eq!(ep.detail.as_deref(), Some("carte insufficienti"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn roundtrip_server_message_snapshot() {
        let msg = ServerMessage::Snapshot(Snapshot {
            protocol_version: 1,
            public: PublicState {
                game_id: "g1".into(),
                protocol_version: 1,
                state: GameState::PlayerTurn,
                current_turn_player_id: Some("p1".into()),
                last_round: false,
                last_round_trigger_player_id: None,
                visible_deck: vec![],
                deck_size: 0,
                claimed_routes: vec![],
                players_summary: vec![],
                tunnel_reveal: None,
                stations: vec![],
                final_scores: None,
                longest_path_player_ids: None,
            },
            private: Some(PrivateState {
                seq: 5,
                hand: vec![],
                tickets: vec![],
                offered_tickets: vec![],
                legal_moves: LegalMoves {
                    routes: vec![],
                    draw_face_up: vec![],
                    can_draw_deck: false,
                    can_draw_tickets: false,
                },
            }),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"SNAPSHOT\""));
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::Snapshot(snap) => {
                assert!(snap.private.is_some());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn snapshot_spectator_private_none() {
        let msg = ServerMessage::Snapshot(Snapshot {
            protocol_version: 1,
            public: PublicState {
                game_id: "g1".into(),
                protocol_version: 1,
                state: GameState::PlayerTurn,
                current_turn_player_id: Some("p1".into()),
                last_round: false,
                last_round_trigger_player_id: None,
                visible_deck: vec![],
                deck_size: 0,
                claimed_routes: vec![],
                players_summary: vec![],
                tunnel_reveal: None,
                stations: vec![],
                final_scores: None,
                longest_path_player_ids: None,
            },
            private: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::Snapshot(snap) => {
                assert!(snap.private.is_none());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn roundtrip_visible_deck_empty_slot() {
        // Slot vuoto = null nel JSON
        let deck: Vec<Option<CardColor>> = vec![Some(CardColor::Red), None, None];
        let json = serde_json::to_string(&deck).unwrap();
        assert_eq!(json, "[\"RED\",null,null]");
        let back: Vec<Option<CardColor>> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 3);
        assert_eq!(back[0], Some(CardColor::Red));
        assert_eq!(back[1], None);
    }
}
