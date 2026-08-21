use std::{sync::Arc};
use std::io;

use crossterm::{
    event::{Event, KeyCode, EventStream},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt; // necessario per consumare l'eventStream
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};
use crossterm::event::KeyEventKind;

use crate::stats;
use crate::AppState;
use crate::messaging::MessageError;

// STSTO INTERFACCIA
struct TuiState {
    input: String,
    logs: Vec<String>,
    should_quit: bool,
    app_state: Arc<AppState>,
}
impl TuiState {
    fn new(app_state: Arc<AppState>) -> Self {
        Self { 
            input: String::new(), 
            logs: vec![
                "Benvenuto nella Admin Console.".to_string(),
                "Digita 'help' per i comandi o 'exit' (o ESC) per uscire.".to_string(),
            ], 
            should_quit: false, 
            app_state 
        }
    }    

    async fn process_command(&mut self) {
        let cmd = self.input.trim().to_string();
        self.input.clear();

        if cmd.is_empty() { return; }

        // stampa il comando a schermo come feedback
        self.logs.push(format!("> {cmd}"));

        let parts: Vec<&str> = cmd.split_whitespace().collect();
        match parts[0] {
            "help" => {
                self.logs.push("Comandi disponibili: stats, users, msg, logs, clear, exit".to_string());
            }

            "clear" => {
                self.logs.clear();
            }

            "exit" | "quit" => {
                self.should_quit = true;
            }

            // USERS
            "users" => {
                let active_users = self.app_state.get_connected_users().await;
                if active_users.is_empty() {
                    self.logs.push("Nessun utente attualmente connesso.".to_string());
                } else {
                    self.logs.push(format!("Utenti connessi ({}):", active_users.len()));
                    for id in active_users {
                        self.logs.push(format!("  - User ID: {}", id));
                    }
                }
            }

            // STATS
            "stats" => {
                if parts.len() == 3 {
                    if let Ok(user_id) = parts[1].parse::<i64>() {
                        let interval = parts[2];
                        if !["day", "week", "month"].contains(&interval) {
                            self.logs.push("Errore: il periodo deve essere 'day', 'week' o 'month'".to_string());
                            return;
                        }

                        self.logs.push(format!("Calcolo statistiche per utente {} in {}...", user_id, interval));

                        match stats::calculate_user_stats(&self.app_state.db, user_id, interval).await {
                            Ok(res) => {
                                self.logs.push("--------------------------------".to_string());
                                self.logs.push(format!("User id:          {}", user_id));
                                self.logs.push(format!("Period:           {}", res.period));
                                self.logs.push(format!("Distance:         {:.2} km", res.distance));
                                self.logs.push(format!("Total time:       {:.2} s", res.total_time));
                                self.logs.push(format!("Total pause time: {:.2} s", res.total_pause));
                                self.logs.push(format!("Average velocity: {:.2} km/h", res.avg_velocity));
                                self.logs.push("--------------------------------".to_string());
                            }
                            Err(e) => self.logs.push(format!("Errore db: {}", e)),
                        }
                    } else {
                        self.logs.push("Errore: user_id deve essere un numero intero.".to_string());
                    }
                } else {
                    self.logs.push("Usage: stats <user_id> <day|week|month>".to_string());
                }
            }

            // LOGS
            "logs" => {
                if parts.len() == 2 {
                    if let Ok(user_id) = parts[1].parse::<i64>() {
                        self.logs.push(format!("Estrazione ultimi log per l'utente {}...", user_id));
                        
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
                        .fetch_all(&self.app_state.db)
                        .await;

                        match query_result {
                            Ok(messages) if messages.is_empty() => {
                                self.logs.push(format!("Nessun messaggio trovato per l'utente {}.", user_id));
                            }
                            Ok(messages) => {
                                use sqlx::Row;
                                self.logs.push("--------------------------------".to_string());
                                for row in messages {
                                    let kind: String = row.get("kind");
                                    let content: String = row.get("content");
                                    let timestamp_ms: i64 = row.get("created_at_ms");
                                    
                                    let time_str = match chrono::DateTime::from_timestamp_millis(timestamp_ms) {
                                        Some(dt) => dt.with_timezone(&chrono::Local).format("%d/%m/%Y %H:%M:%S").to_string(),
                                        None => timestamp_ms.to_string(),
                                    };

                                    self.logs.push(format!("[{}] [{}] {}", time_str, kind.to_uppercase(), content));
                                }
                                self.logs.push("--------------------------------".to_string());
                            }
                            Err(e) => self.logs.push(format!("Errore log: {}", e)),
                        }
                    } else {
                        self.logs.push("Errore: user_id deve essere intero.".to_string());
                    }
                } else {
                    self.logs.push("Usage: logs <user_id>".to_string());
                }
            }

            // MSG
            "msg" => {
                if parts.len() >= 2 {
                    match parts[1] {
                        "send" => {
                            if parts.len() >= 4 {
                                if let Ok(user_id) = parts[2].parse::<i64>() {
                                    let text = parts[3..].join(" ");
                                    let time_str = chrono::Utc::now().format("%H:%M:%S");
                                
                                    let msg_id = self.app_state.message_service.send_admin_direct_message(user_id, &text).await;
                                    match msg_id {
                                        Ok(id) => self.logs.push(format!("OK #{} [Direct -> {} {}] {}", id, user_id, time_str, text)),
                                        Err(MessageError::NotFound(reason)) => {
                                            self.logs.push(format!("Invio fallito. Utente #{user_id} non trovato: {reason}"));
                                        }
                                        Err(MessageError::DatabaseError(e)) => {
                                            tracing::error!("Errore DB: {e}");
                                            self.logs.push("Invio fallito. Errore imprevisto.".to_string());
                                        }
                                        Err(MessageError::ValidationError(msg)) => {
                                            self.logs.push(format!("Invio fallito. Msg non valido: {msg}"));
                                        }
                                    }
                                } else {
                                    self.logs.push("Errore: user_id deve essere un intero.".to_string());
                                }
                            } else {
                                self.logs.push("Usage: msg send <user_id> <testo>".to_string());
                            }
                        }
                        "broadcast" => {
                            if parts.len() >= 3 {
                                let text = parts[2..].join(" ");
                                let time_str = chrono::Utc::now().format("%H:%M:%S");
                                
                                let msg_id = self.app_state.message_service.send_admin_broadcast_message(&text).await;
                                match msg_id {
                                    Ok(id) => self.logs.push(format!("OK #{} [Broadcast {}] {}", id, time_str, text)),
                                    Err(MessageError::DatabaseError(e)) => {
                                        tracing::error!("Errore DB: {e}");
                                        self.logs.push("Invio fallito. Errore imprevisto.".to_string());
                                    }
                                    Err(MessageError::ValidationError(msg)) => {
                                        self.logs.push(format!("Invio fallito. Msg non valido: {msg}"));
                                    }
                                    _ => {
                                        self.logs.push("Invio fallito. Errore imprevisto.".to_string());
                                    }
                                }
                            } else {
                                self.logs.push("Usage: msg broadcast <testo>".to_string());
                            }
                        }
                        _ => self.logs.push("Sotto-comando non valido. Usa 'send' o 'broadcast'.".to_string()),
                    }
                } else {
                    self.logs.push("Usage: msg <send|broadcast> ...".to_string());
                }
            }

            _ => {
                self.logs.push("Comando sconosciuto. Digita 'help'.".to_string());
            }
        }

        // autoscroll rudimentale
        if self.logs.len() > 50 {
            let overflow = self.logs.len() - 50;
            self.logs.drain(0..overflow);
        }
    }
}
/* 
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
}*/

// CORE ASINCRONO
pub async fn start_admin_console(state: Arc<AppState>) {
    // setup iniziale del terminale
    enable_raw_mode().expect("Impossibile abilitare Raw Mode");
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).expect("Impossibile entrare nello schermo alternativo");
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).expect("Impossibile creare il terminale");

    // inizializzazione dello stato
    let mut tui_state = TuiState::new(state);
    let mut reader = EventStream::new();
    let mut tick_rate = tokio::time::interval(std::time::Duration::from_millis(500)); // timer  per animazioni e refresh dati in background

    // event loop
    loop {
        //disenga l'interfaccia ad ogni ciclo
        terminal.draw(|f| draw_ui(f, &tui_state)).expect("Errore di rendering");

        tokio::select! {
            // risveglio periodico
            _ = tick_rate.tick() => {
                // TODO: aggiornamento dati in real time
            }
            // risveglio immediato alla pressione di un tasto
            Some(Ok(event)) = reader.next() => {
                if let Event::Key(key) = event {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char(c) => {
                                tui_state.input.push(c);
                            }
                            KeyCode::Backspace => {
                                tui_state.input.pop(); // Rimuove l'ultimo carattere
                            }
                            KeyCode::Enter => {
                                tui_state.process_command().await;
                            }
                            KeyCode::Esc => {
                                tui_state.should_quit = true;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        if tui_state.should_quit {
            break;
        }
    }

    // ripristino essenziale 
    disable_raw_mode().expect("Impossibile disabilitare Raw Mode");
    execute!(terminal.backend_mut(), LeaveAlternateScreen).expect("Impossibile uscire dallo schermo alternativo");
    terminal.show_cursor().expect("Impossibile mostrare il cursore");
}

// RENDER INTERFACCIA
fn draw_ui(f: &mut ratatui::Frame, state: &TuiState) {
    // divide lo schermo in due aree: 80% log, 20% input
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Min(5),    // Area Logs
                Constraint::Length(3), // Area Input (sempre fissa a 3 righe di altezza)
            ]
            .as_ref(),
        )
        .split(f.size());

    // BLOCCO SUPERIORE: cronologia log
    let logs_text = state.logs.join("\n");
    let line_count = state.logs.len() as u16;
    
    // calcoliamo lo spazio effettivo: l'altezza del blocco meno 2 righe (bordo sopra e sotto)
    let area_height = chunks[0].height.saturating_sub(2);

    // Se abbiamo più righe dell'altezza dello schermo, calcoliamo l'offset
    let scroll_offset = if line_count > area_height {
        line_count - area_height
    } else {
        0
    };

    let logs_block = Paragraph::new(logs_text)
        .block(Block::default().title(" Risultati Console ").borders(Borders::ALL))
        // Diciamo a Ratatui di scrollare verso il basso dell'offset calcolato
        .scroll((scroll_offset, 0)); 

    f.render_widget(logs_block, chunks[0]);

    // BLOCCO INFERIORE: riga di comando interattiva
    let input_text = format!("> {}", state.input);
    let input_block = Paragraph::new(input_text)
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().title(" Inserisci Comando ").borders(Borders::ALL));
    f.render_widget(input_block, chunks[1]);
}

/* 
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
*/