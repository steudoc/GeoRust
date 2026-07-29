/*
Crate "common"

Contiene le strutture dati condivise tra client e server:
 - DTO per endpoint REST
 - Enum dello stato utente
 - Messaggi (chat diretta/broadcast)
*/

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

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
