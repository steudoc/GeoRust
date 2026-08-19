use std::sync::Arc;
use sqlx::Row;

use tokio::io::{
    self, 
    AsyncBufReadExt, 
    BufReader
};

use crate::messaging::MessageError;
use crate::stats;
use crate::AppState;

// helper per stampare il menu in modo pulito
fn print_help() {
    println!("============================================================");
    println!("                  ADMIN CONSOLE COMMANDS                    ");
    println!("============================================================");
    println!("  stats <user_id> <day|week|month>  - Calcola le statistiche di movimento");
    println!("  users                             - Elenca gli utenti attualmente connessi");
    println!("  msg send <user_id> <testo>        - Invia un messaggio a uno specifico utente");
    println!("  msg broadcast <testo>             - Invia un messaggio a tutti gli utenti");
    println!("  logs <user_id>                    - Visualizza gli ultimi messaggi/GPS ricevuti");
    println!("  clear                             - Pulisce lo schermo del terminale");
    println!("  help                              - Mostra questo menu");
    println!("============================================================");
}

pub async fn start_admin_console(state: Arc<AppState>) {
    let db_pool = state.db.clone();
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin).lines();

    // stampa del menu
    print!("\x1B[2J\x1B[1;1H");
    println!("Admin console ready.");
    print_help();

    while let Ok(Some(line)) = reader.next_line().await {
        let input = line.trim();
        if input.is_empty() {
            continue;
        }

        let parts: Vec<&str> = input.split_whitespace().collect();
        let command = parts[0];

        match command {
            "stats" => {
                if parts.len() == 3 {
                    if let Ok(user_id) = parts[1].parse::<i64>() {
                        let interval = parts[2];
                        if !["day", "week", "month"].contains(&interval) {
                            println!("Errore: il periodo deve essere 'day', 'week' o 'month'");
                            continue;
                        }

                        println!(
                            "Calculating stats for user {} in period {}...",
                            user_id, interval
                        );

                        match stats::calculate_user_stats(&db_pool, user_id, interval).await {
                            Ok(res) => {
                                println!("--------------------------------");
                                println!("User id:\t\t{}", user_id);
                                println!("Period:\t\t\t{}", res.period);
                                println!("Distance:\t\t{:.2} km", res.distance); // Aggiunta unità di misura
                                println!("Total time:\t\t{:.2} s", res.total_time);
                                println!("Total pause time:\t{:.2} s", res.total_pause);
                                println!("Average velocity:\t{:.2} km/h", res.avg_velocity);
                                println!("--------------------------------");
                            }
                            Err(e) => println!("Error in db data extraction: {}", e),
                        }
                    } else {
                        println!("Errore: user_id deve essere un numero intero.");
                    }
                } else {
                    println!("Usage: stats <user_id> <day|week|month>");
                }
            }

            "users" => {
                let active_users = state.get_connected_users().await;

                if active_users.is_empty() {
                    println!("Nessun utente attualmente connesso.");
                } else {
                    println!("--------------------------------");
                    println!("Utenti attualmente connessi ({}): ", active_users.len());
                    for id in active_users {
                        println!("  - User ID: {id}");
                    }
                    println!("--------------------------------");
                }
            }   

            "logs" => {
                if parts.len() == 2 {
                    if let Ok(user_id) = parts[1].parse::<i64>() {
                        println!("Estrazione degli ultimi log per l'utente {user_id}...");
                        
                        let query_result = sqlx::query(
                            r#"
                            SELECT kind, content, created_at_ms 
                            FROM messages 
                            WHERE sender_id = ?1 OR recipient_id = ?1 
                            ORDER BY created_at_ms DESC 
                            LIMIT 10
                            "#
                        )
                        .bind(user_id)
                        .fetch_all(&db_pool)
                        .await;

                        match query_result {
                            Ok(messages) if messages.is_empty() => {
                                println!("Nessun messaggio trovato per l'utente {user_id}.");
                            }
                            Ok(messages) => {
                                println!("--------------------------------");
                                for row in messages {
                                    let kind: String = row.get("kind");
                                    let content: String = row.get("content");
                                    let timestamp_ms: i64 = row.get("created_at_ms");
                                    
                                    // converte i millisecondi in una data formattata
                                    let time_str = match chrono::DateTime::from_timestamp_millis(timestamp_ms) {
                                        Some(dt) => dt.with_timezone(&chrono::Local).format("%d/%m/%Y %H:%M:%S").to_string(),
                                        None => timestamp_ms.to_string(),
                                    };

                                    println!("[{}] [{}] {}", time_str, kind.to_uppercase(), content);
                                }
                                println!("--------------------------------");
                            }
                            Err(e) => println!("Errore nel recupero dei log: {}", e),
                        }
                    } else {
                        println!("Errore: user_id deve essere un numero intero.");
                    }
                } else {
                    println!("Usage: logs <user_id>");
                }
            }

            "msg" => {
                // Gestione dei sotto-comandi per "msg" (send o broadcast)
                if parts.len() >= 2 {
                    let sub_command = parts[1];
                    match sub_command {
                        "send" => {
                            if parts.len() >= 4 {
                                if let Ok(user_id) = parts[2].parse::<i64>() {
                                    // Ricostruiamo il messaggio unendo le parole rimanenti
                                    let text = parts[3..].join(" ");

                                    let time_str = chrono::Utc::now().format("%H:%M:%S");
                                
                                    let msg_id = state.message_service.send_admin_direct_message(user_id, &text).await;
                                    match msg_id {
                                        Ok(id) => println!("OK #{} [Direct -> {} {}] {}", id, user_id, time_str, text),
                                        Err(MessageError::NotFound(reason)) => {
                                            println!("Invio fallito. Utente #{user_id} non trovato: {reason}");
                                        }
                                        Err(MessageError::DatabaseError(e)) => {
                                            tracing::error!("Errore DB console admin: {e}");
                                            println!("Invio fallito. Si è verificato un errore imprevisto.");
                                        }
                                        Err(MessageError::ValidationError(msg)) => {
                                            println!("Invio fallito. Messaggio non valido: {msg}");
                                        }
                                    }
                                } else {
                                    println!("Errore: user_id deve essere un intero.");
                                }
                            } else {
                                println!("Usage: msg send <user_id> <testo del messaggio>");
                            }
                        }
                        "broadcast" => {
                            if parts.len() >= 3 {
                                // Ricostruiamo il messaggio
                                let text = parts[2..].join(" ");
                                
                                let time_str = chrono::Utc::now().format("%H:%M:%S");
                                
                                let msg_id = state.message_service.send_admin_broadcast_message(&text).await;
                                match msg_id {
                                    Ok(id) => println!("OK #{} [Broadcast {}] {}", id, time_str, text),
                                    Err(MessageError::DatabaseError(e)) => {
                                        tracing::error!("Errore DB console admin: {e}");
                                        println!("Invio fallito. Si è verificato un errore imprevisto.");
                                    }
                                    Err(MessageError::ValidationError(msg)) => {
                                        println!("Invio fallito. Messaggio non valido: {msg}");
                                    }
                                    _ => {
                                        println!("Invio fallito. Si è verificato un errore imprevisto.");
                                    }
                                }
                            } else {
                                println!("Usage: msg broadcast <testo del messaggio>");
                            }
                        }
                        _ => println!("Sotto-comando non valido. Usa 'send' o 'broadcast'."),
                    }
                } else {
                    println!("Usage: msg <send|broadcast> ...");
                }
            }

            // ---------------------------------------------------------
            // UTILITIES
            // ---------------------------------------------------------
            "help" => print_help(),

            "clear" => {
                // Sequenza ANSI per pulire lo schermo e riportare il cursore in alto a sinistra
                print!("\x1B[2J\x1B[1;1H");
                println!("Console pulita. Digita 'help' per i comandi.");
            }

            _ => {
                println!("Comando non riconosciuto. Digita 'help' per vedere la lista dei comandi.")
            }
        }
    }
}
