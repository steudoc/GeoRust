use tokio::io::{self, AsyncBufReadExt, BufReader};
use sqlx::SqlitePool;

// Importiamo il modulo stats
use crate::stats;

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

pub async fn start_admin_console(db_pool: SqlitePool) {
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin).lines();

    // stampa del menu
    print!("\x1B[2J\x1B[1;1H"); 
    println!("Admin console ready.");
    print_help();

    while let Ok(Some(line)) = reader.next_line().await {
        let input = line.trim();
        if input.is_empty() { continue; }

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

                        println!("Calculating stats for user {} in period {}...", user_id, interval);

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
            },

            // ---------------------------------------------------------
            // COMANDI DA IMPLEMENTARE (MOCK)
            // ---------------------------------------------------------
            "users" => {
                println!("[TODO] Elenco utenti connessi.");
                println!("Suggerimento: qui dovrai accedere ad AppState per leggere la mappa degli utenti attivi.");
            },
            
            "logs" => {
                if parts.len() == 2 {
                    if let Ok(user_id) = parts[1].parse::<i64>() {
                        println!("[TODO] Estrazione degli ultimi log per l'utente {}.", user_id);
                        println!("Suggerimento: query su SQLite ordinata per timestamp decrescente (LIMIT 10).");
                    } else {
                        println!("Errore: user_id deve essere un numero intero.");
                    }
                } else {
                    println!("Usage: logs <user_id>");
                }
            },

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
                                    println!("[TODO] Invio messaggio '{}' all'utente {}.", text, user_id);
                                    println!("Suggerimento: usa il canale mpsc specifico dell'utente salvato in AppState.");
                                } else {
                                    println!("Errore: user_id deve essere un intero.");
                                }
                            } else {
                                println!("Usage: msg send <user_id> <testo del messaggio>");
                            }
                        },
                        "broadcast" => {
                            if parts.len() >= 3 {
                                // Ricostruiamo il messaggio
                                let text = parts[2..].join(" ");
                                println!("[TODO] Invio broadcast: '{}'", text);
                                println!("Suggerimento: usa il canale broadcast (tx) globale di AppState.");
                            } else {
                                println!("Usage: msg broadcast <testo del messaggio>");
                            }
                        },
                        _ => println!("Sotto-comando non valido. Usa 'send' o 'broadcast'."),
                    }
                } else {
                    println!("Usage: msg <send|broadcast> ...");
                }
            },

            // ---------------------------------------------------------
            // UTILITIES
            // ---------------------------------------------------------
            "help" => print_help(),
            
            "clear" => {
                // Sequenza ANSI per pulire lo schermo e riportare il cursore in alto a sinistra
                print!("\x1B[2J\x1B[1;1H"); 
                println!("Console pulita. Digita 'help' per i comandi.");
            },
            
            _ => println!("Comando non riconosciuto. Digita 'help' per vedere la lista dei comandi."),
        }
    }
}