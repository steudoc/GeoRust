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

// ---------------------------------------------------------------------
// STATO UTENTE
// ---------------------------------------------------------------------

/*
Stato di un utente della flotta, secondo le specifiche del progetto:
 - "Disconnected": nessuna posizione ricevuta recentemente
 - "Moving": le coordinate sono cambiate rispetto all'ultimo invio
 - "Still": le coordinate non cambiano da almeno 3 minuti
*/
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]   // Deriviamo tratto Default
#[serde(rename_all = "snake_case")]
pub enum UserState {
    #[default]      // specifichiamo che Disconnected è il valore di default
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

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

// ---------------------------------------------------------------------
// WEBSOCKET: Messaggi Client -> Server
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] // Aggiunto PartialEq cosi' da poter confrontare i messaggi nei test
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum WsClientMessage {
    Text {
        text: String,
        timestamp: DateTime<Utc>,
    },
    PositionUpdate {
        coordinata: tracking::Coordinata,
        /*
         Non inseriamo anche il timestamp, il server registrerà il tempo usando il timestamp in cui riceve il messaggio:
         evitiamo che il client invii timestamp dupllicati, nel futuro o nel passato
         */
    },
    TripCompleted,  // Comunicare al server che il viaggio è completo
    /*
    Invece che considerare la chiusura della WebSocket come fine del tragitto inviamo un messaggio esplicito, poichè
    la chiusura della websocket potrebbe causarsi anche per altre ragioni, es. errore di rete, crash dell'applicativo, interruzioni improvvise del server o client
     */
}

// ---------------------------------------------------------------------
// WEBSOCKET: Messaggi Server -> Client
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum WsServerMessage {
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

    PositionAccepted {  // Conferma acquisizione di una posizione
        stato: UserState,
        coord_ricevute: usize,
    },
    TripCompleted {     // Riepilogo del viaggio
        numero_coord: usize,
        tempo_movimento: u64,
        tempo_fermo: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }

    #[test]
    fn position_update_round_trip_preserves_coordinates() {
        let message = WsClientMessage::PositionUpdate {
            coordinata: tracking::Coordinata::new(45.0, 7.0).unwrap(),
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
                "coordinates": { "latitude": 100.0, "longitude": 7.0 }
            }
        }"#;

        assert!(serde_json::from_str::<WsClientMessage>(json).is_err());
    }
}
