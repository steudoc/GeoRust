/*
Crate "common"

Contiene le strutture dati condivise tra client e server:
 - DTO per endpoint REST
 - Enum dello stato utente
 - Messaggi (chat diretta/broadcast)
 - Messaggi WebSocket
*/

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub mod tracking;

/// Intervallo di tempo tra due posizioni
pub const POSITION_INTERVAL_SECONDS: u64 = 30;

// ---------------------------------------------------------------------
// STATO UTENTE
// ---------------------------------------------------------------------

/*
Stato di un utente della flotta, secondo le specifiche del progetto:
 - "Disconnected": nessuna posizione ricevuta recentemente
 - "Moving": le coordinate sono cambiate rispetto all'ultimo invio
 - "Still": le coordinate non cambiano da almeno 3 minuti
*/
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)] // Deriviamo tratto Default
#[serde(rename_all = "snake_case")]
pub enum UserState {
    #[default] // specifichiamo che Disconnected è il valore di default
    Disconnected,
    Moving,
    Still,
}

// ---------------------------------------------------------------------
// REST: Registrazione / Login
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterResponse {
    pub user_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub user_id: i64,
    pub token: String, // token facile da usare nelle req successive
}

// ---------------------------------------------------------------------
// MOVIMENTO
// ---------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct MovementStats {
    pub period: String,
    pub distance: f64,
    pub total_time: f64,
    pub total_pause: f64,
    pub avg_velocity: f64,
}

// ---------------------------------------------------------------------
// WEBSOCKET: Messaggi Client -> Server
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] // Aggiunto PartialEq cosi' da poter confrontare i messaggi nei test
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum WsClientMessage {
    StartTrip, // Inizia un nuovo trip
    Text {
        text: String,
        //timestamp: DateTime<Utc>,
    },
    PositionUpdate {
        coordinata: tracking::Coordinata,
        elapsed_seconds: u64,
    },
    TripCompleted, /*
                   Comunicare al server che il viaggio è completo
                   Invece che considerare la chiusura della WebSocket come fine del tragitto inviamo un messaggio esplicito, poiché
                   la chiusura della websocket potrebbe causarsi anche per altre ragioni, es. errore di rete, crash dell'applicativo, interruzioni improvvise del server o client
                    */
    DirectTextAck {
        id: i64,
    }, // Altri messaggi possono essere aggiunti qui
}

// ---------------------------------------------------------------------
// WEBSOCKET: Messaggi Server -> Client
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum WsServerMessage {
    TripStarted, // Il trip è stato creato
    BroadcastText {
        id: i64,
        text: String,
        timestamp: DateTime<Utc>,
    },
    DirectText {
        id: i64,
        text: String,
        timestamp: DateTime<Utc>,
    },
    Error {
        code: String,
        message: String,
    },
    // Altri messaggi possono essere aggiunti qui
    PositionAccepted {
        // Conferma acquisizione di una posizione
        stato: UserState,
        coord_ricevute: usize,
    },
    TripCompleted {
        // Riepilogo del viaggio
        numero_coord: usize,
        tempo_movimento: u64,
        tempo_fermo: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_update_round_trip_preserves_coordinates() {
        let message = WsClientMessage::PositionUpdate {
            coordinata: tracking::Coordinata::new(45.0, 7.0).unwrap(),
            elapsed_seconds: 30,
        };

        let json = serde_json::to_string(&message).unwrap();
        let decoded: WsClientMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, message);
    }

    #[test]
    fn position_update_rejects_invalid_coordinates() {
        let json = r#"{
            "type": "position_update",
            "payload": {
                "coordinata": { "latitudine": 100.0, "longitudine": 7.0 },
                "elapsed_seconds": 0
            }
        }"#;

        assert!(serde_json::from_str::<WsClientMessage>(json).is_err());
    }

    #[test]
    fn trip_start_messages_round_trip() {
        let client_json = serde_json::to_string(&WsClientMessage::StartTrip).unwrap();
        let server_json = serde_json::to_string(&WsServerMessage::TripStarted).unwrap();

        assert_eq!(client_json, r#"{"type":"start_trip"}"#);
        assert_eq!(server_json, r#"{"type":"trip_started"}"#);
        assert_eq!(
            serde_json::from_str::<WsClientMessage>(&client_json).unwrap(),
            WsClientMessage::StartTrip
        );
        assert_eq!(
            serde_json::from_str::<WsServerMessage>(&server_json).unwrap(),
            WsServerMessage::TripStarted
        );
    }
}
