/*
Crate "common"

Contiene le strutture dati condivise tra client e server:
 - DTO per endpoint REST
 - Enum dello stato utente
 - Messaggi (chat diretta/broadcast)
*/

use chrono::{DateTime, Utc};
use serde::{Deserialize,Serialize};

// ---------------------------------------------------------------------
// STATO UTENTE
// ---------------------------------------------------------------------

/*
Stato di un utente della flotta, secondo le specifiche del progetto:
 - "Disconnected": nessuna posizione ricevuta recentemente
 - "Moving": le coordinate sono cambiate rispetto all'ultimo invio
 - "Still": le coordinate non cambiano da almeno 3 minuti
*/
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserState {
    Disconnected,
    Moving,
    Still,
}

impl Default for UserState {
    fn default() -> Self {
        UserState::Disconnected
    }
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
    pub token: String,  // token facile da usare nelle req successive
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum WsClientMessage {
    Text {
        text: String,
        timestamp: DateTime<Utc>,
    },
    // Altri messaggi possono essere aggiunti qui
}

// ---------------------------------------------------------------------
// WEBSOCKET: Messaggi Server -> Client
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

#[cfg(test)]
mod tests {
    //use super::*;

    #[test]
    fn it_works() {
        assert!(true);
    }
}
