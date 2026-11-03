//! CiufCiuf! — Tipi condivisi del contratto WebSocket.
//!
//! Questo crate definisce i tipi serializzabili (serde) che costituiscono
//! il contratto tra server Rust e client TypeScript. L'export TypeScript
//! via `ts-rs` garantisce che i tipi non vadano in drift (piano §11, R2).
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

/// Macchina a stati del server (piano §6).
///
/// L'enum è marcato `#[non_exhaustive]` per consentire l'aggiunta di stati
/// futuri (es. `WAITING_FOR_PLAYERS` e stati Europe) senza breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export)]
#[non_exhaustive]
pub enum GameState {
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

/// Azione inbound (client → server). Messaggio inviato su `game/{gameId}/action`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "actionType")]
#[ts(export)]
pub enum InboundAction {
    /// Pescare una carta treno dal mazzo coperto.
    #[serde(rename = "DRAW_TRAIN_CARD")]
    DrawTrainCard(DrawCardPayload),
    /// Pescare nuovi biglietti mid-game (azione nº 3 del turno, piano §3.1/§7.1).
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
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChooseTicketsPayload {
    pub player_id: String,
    /// Id dei biglietti da tenere (in ordine lessicografico).
    pub keep: Vec<TicketId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ClaimRoutePayload {
    pub player_id: String,
    pub route_id: RouteId,
    /// Suggerimento: il server valida e risolve il pagamento (locomotive automatiche).
    pub cards_discarded: Vec<CardColor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RequestSnapshotPayload {
    pub player_id: String,
}

// ── Messaggi outbound ───────────────────────────────────────────────

/// Broadcast pubblico: `game/{gameId}/state`
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PublicState {
    pub game_id: String,
    pub protocol_version: u32,
    pub state: GameState,
    pub current_turn_player_id: Option<String>,
    pub last_round: bool,
    pub last_round_trigger_player_id: Option<String>,
    pub visible_deck: Vec<CardColor>,
    pub claimed_routes: Vec<ClaimedRoute>,
    pub players_summary: Vec<PlayerSummary>,
    /// Europe: in `TUNNEL_RESOLUTION`, le carte rivelate e costo extra.
    pub tunnel_reveal: Option<TunnelReveal>,
    /// Europe: stazioni costruite.
    pub stations: Vec<StationPlacement>,
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

/// Broadcast unicast privato: `user/{playerId}/private`
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PrivateState {
    pub seq: u64,
    pub hand: Vec<CardColor>,
    pub tickets: Vec<TicketInfo>,
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

/// Errore: inviato sul canale privato del giocatore.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ErrorMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub seq: u64,
    pub reason: ErrorReason,
    pub action_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export)]
pub enum ErrorReason {
    InsufficientCards,
    RouteAlreadyClaimed,
    RouteBlockedDoubleVariant,
    InvalidAction,
    NotYourTurn,
    WrongState,
    TicketChoiceOutOfRange,
}

/// Snapshot completo dopo (re)connect.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Snapshot {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub protocol_version: u32,
    pub public: PublicState,
    pub private: PrivateState,
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
    fn roundtrip_inbound_draw() {
        let msg = InboundAction::DrawTrainCard(DrawCardPayload {
            player_id: "p1".into(),
            source: DrawSource::FaceUp { index: 2 },
        });
        let json = serde_json::to_string(&msg).unwrap();
        let back: InboundAction = serde_json::from_str(&json).unwrap();
        match back {
            InboundAction::DrawTrainCard(p) => {
                assert_eq!(p.player_id, "p1");
                assert!(matches!(p.source, DrawSource::FaceUp { index: 2 }));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn roundtrip_inbound_choose() {
        let msg = InboundAction::ChooseTickets(ChooseTicketsPayload {
            player_id: "p2".into(),
            keep: vec!["denver_phoenix".into(), "seattle_new_york".into()],
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
            visible_deck: vec![CardColor::Red, CardColor::Blue, CardColor::Locomotive],
            claimed_routes: vec![],
            players_summary: vec![],
            tunnel_reveal: None,
            stations: vec![],
        };
        let json = serde_json::to_string(&state).unwrap();
        let back: PublicState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.game_id, "g1");
        assert_eq!(back.visible_deck.len(), 3);
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
    }

    #[test]
    fn error_reason_deser() {
        let json = "\"INSUFFICIENT_CARDS\"";
        let reason: ErrorReason = serde_json::from_str(json).unwrap();
        assert!(matches!(reason, ErrorReason::InsufficientCards));
    }
}
