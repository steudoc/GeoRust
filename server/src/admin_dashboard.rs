/*use std::{sync::Arc};
use std::io;
use std::sync::Arc;

use crossterm::{
    event::{
        DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyCode, KeyEventKind,
        MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures_util::StreamExt; // necessario per consumare l'eventStream
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use sqlx::Row;

use crate::AppState;
use crate::messaging::MessageError;
use crate::stats;

// STATO INTERFACCIA
pub struct TuiState {
    pub input: String,
    pub logs: Vec<String>,
    pub should_quit: bool,
    pub app_state: Arc<AppState>,
    pub scroll_offset: u16,
    pub active_users: Vec<i64>,
    pub recent_messages: Vec<String>,
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
            app_state,
            scroll_offset: 0,
            active_users: Vec::new(),
            recent_messages: Vec::new(),
        }
    }

    async fn process_command(&mut self) {
        self.scroll_offset = 0;
        let cmd = self.input.trim().to_string();
        self.input.clear();

        if cmd.is_empty() {
            return;
        }

        // stampa il comando a schermo come feedback
        self.logs.push(format!("> {cmd}"));

        let parts: Vec<&str> = cmd.split_whitespace().collect();
        match parts[0] {
            "help" => {
                self.logs
                    .push("Comandi disponibili: stats, users, msg, logs, clear, exit".to_string());
            }
            "clear" => {
                self.logs.clear();
            }
            "exit" | "quit" => {
                self.should_quit = true;
            }

            // USERS
            "users" => {
                self.logs.push("Estrazione utenti...".to_string());

                let query_result = sqlx::query("SELECT id, username FROM users ORDER BY id ASC")
                    .fetch_all(&self.app_state.db)
                    .await;

                match query_result {
                    Ok(users) => {
                        if users.is_empty() {
                            self.logs.push("Nessun utente trovato.".to_string());
                        } else {
                            self.logs
                                .push("--------------------------------".to_string());
                            self.logs
                                .push(format!("Utenti registrati ({}):", users.len()));
                            for row in users {
                                let id: i64 = row.get("id");
                                let username: String = row.get("username");
                                self.logs
                                    .push(format!("  - User ID: {}, Username: {}", id, username));
                            }
                            self.logs
                                .push("--------------------------------".to_string());
                        }
                    }
                    Err(e) => {
                        tracing::error!("Errore DB: {}", e);
                        self.logs
                            .push("Errore durante l'estrazione degli utenti.".to_string());
                    }
                }
            }

            // STATS
            "stats" => {
                if parts.len() == 3 {
                    if let Ok(user_id) = parts[1].parse::<i64>() {
                        let interval = parts[2];
                        if !["day", "week", "month"].contains(&interval) {
                            self.logs.push(
                                "Errore: il periodo deve essere 'day', 'week' o 'month'"
                                    .to_string(),
                            );
                            return;
                        }

                        self.logs.push(format!(
                            "Calcolo statistiche per utente {} in {}...",
                            user_id, interval
                        ));

                        match stats::calculate_user_stats(&self.app_state.db, user_id, interval)
                            .await
                        {
                            Ok(res) => {
                                self.logs
                                    .push("--------------------------------".to_string());
                                self.logs.push(format!("User id:          {}", user_id));
                                self.logs.push(format!("Period:           {}", res.period));
                                self.logs
                                    .push(format!("Distance:         {:.2} km", res.distance));
                                self.logs
                                    .push(format!("Total time:       {:.2} s", res.total_time));
                                self.logs
                                    .push(format!("Total pause time: {:.2} s", res.total_pause));
                                self.logs.push(format!(
                                    "Average velocity: {:.2} km/h",
                                    res.avg_velocity
                                ));
                                self.logs
                                    .push("--------------------------------".to_string());
                            }
                            Err(e) => self.logs.push(format!("Errore db: {}", e)),
                        }
                    } else {
                        self.logs
                            .push("Errore: user_id deve essere un numero intero.".to_string());
                    }
                } else {
                    self.logs
                        .push("Usage: stats <user_id> <day|week|month>".to_string());
                }
            }

            // LOGS
            "logs" => {
                if parts.len() == 2 {
                    let username = parts[1];

                    self.logs.push(format!(
                        "Estrazione ultimi 10 log per l'utente '{}'...",
                        username
                    ));

                    let query_result = sqlx::query(
                        r#"
                        SELECT kind, content, created_at_ms, is_read
                        FROM messages m
                        LEFT JOIN users u ON m.sender_id = u.id OR m.recipient_id = u.id
                        WHERE u.username = ?1 AND (m.kind != 'broadcast')
                        ORDER BY created_at_ms DESC 
                        LIMIT 10
                        "#,
                    )
                    .bind(username)
                    .fetch_all(&self.app_state.db)
                    .await;

                    match query_result {
                        Ok(messages) if messages.is_empty() => {
                            self.logs.push(format!(
                                "Nessun messaggio trovato per l'utente '{}'.",
                                username
                            ));
                        }
                        Ok(messages) => {
                            self.logs
                                .push("--------------------------------".to_string());
                            for row in messages {
                                let kind: String = row.get("kind");
                                let content: String = row.get("content");
                                let timestamp_ms: i64 = row.get("created_at_ms");
                                let is_read: bool = row.get("is_read");

                                let time_str =
                                    match chrono::DateTime::from_timestamp_millis(timestamp_ms) {
                                        Some(dt) => dt
                                            .with_timezone(&chrono::Local)
                                            .format("%d/%m/%Y %H:%M:%S")
                                            .to_string(),
                                        None => timestamp_ms.to_string(),
                                    };

                                let kind = match kind.as_str() {
                                    "direct" => "OUTBOUND",
                                    "client_to_server" => "INBOUND",
                                    "broadcast" => "BROADCAST",
                                    _ => "UNKNOWN",
                                };

                                let read_status = match (kind, is_read) {
                                    ("OUTBOUND", true) => " (READ)",
                                    ("OUTBOUND", false) => " (PENDING)",
                                    _ => "",
                                };

                                self.logs.push(format!(
                                    "[{}] [{}] {}{}",
                                    time_str,
                                    kind.to_uppercase(),
                                    content,
                                    read_status
                                ));
                            }
                            self.logs
                                .push("--------------------------------".to_string());
                        }
                        Err(e) => self.logs.push(format!("Errore log: {}", e)),
                    }
                } else {
                    self.logs.push("Usage: logs <username>".to_string());
                }
            }

            // MSG
            "msg" => {
                if parts.len() >= 2 {
                    match parts[1] {
                        "send" => {
                            if parts.len() >= 4 {
                                let username = parts[2];
                                let text = parts[3..].join(" ");
                                let time_str = chrono::Utc::now().format("%H:%M:%S");

                                match self
                                    .app_state
                                    .message_service
                                    .send_admin_direct_message(username, &text)
                                    .await
                                {
                                    Ok(_) => self.logs.push(format!(
                                        "[{time_str}] [OUTBOUND -> {username}] {text} (PENDING)"
                                    )),
                                    Err(MessageError::NotFound(reason)) => {
                                        self.logs.push(format!("Invio fallito. {reason}"));
                                    }
                                    Err(MessageError::DatabaseError(e)) => {
                                        tracing::error!("Errore DB: {e}");
                                        self.logs
                                            .push("Invio fallito. Errore imprevisto.".to_string());
                                    }
                                    Err(MessageError::ValidationError(msg)) => {
                                        self.logs.push(format!(
                                            "Invio fallito. Messaggio non valido: {msg}"
                                        ));
                                    }
                                }
                            } else {
                                self.logs
                                    .push("Usage: msg send <username> <testo>".to_string());
                            }
                        }
                        "broadcast" => {
                            if parts.len() >= 3 {
                                let text = parts[2..].join(" ");
                                let time_str = chrono::Utc::now().format("%H:%M:%S");

                                match self
                                    .app_state
                                    .message_service
                                    .send_admin_broadcast_message(&text)
                                    .await
                                {
                                    Ok(_) => self.logs.push(format!(
                                        "[{time_str}] [OUTBOUND -> BROADCAST] {text}"
                                    )),
                                    Err(MessageError::DatabaseError(e)) => {
                                        tracing::error!("Errore DB: {e}");
                                        self.logs
                                            .push("Invio fallito. Errore imprevisto.".to_string());
                                    }
                                    Err(MessageError::ValidationError(msg)) => {
                                        self.logs.push(format!(
                                            "Invio fallito. Messaggio non valido: {msg}"
                                        ));
                                    }
                                    _ => {
                                        self.logs
                                            .push("Invio fallito. Errore imprevisto.".to_string());
                                    }
                                }
                            } else {
                                self.logs.push("Usage: msg broadcast <testo>".to_string());
                            }
                        }
                        _ => self.logs.push(
                            "Sotto-comando non valido. Usa 'send' o 'broadcast'.".to_string(),
                        ),
                    }
                } else {
                    self.logs
                        .push("Usage: msg <send|broadcast> ...".to_string());
                }
            }

            _ => {
                self.logs
                    .push("Comando sconosciuto. Digita 'help'.".to_string());
            }
        }

        // autoscroll rudimentale
        if self.logs.len() > 50 {
            let overflow = self.logs.len() - 50;
            self.logs.drain(0..overflow);
        }
    }
}

// CORE ASINCRONO
pub async fn start_admin_console(
    state: Arc<AppState>,
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
) {
    // setup iniziale del terminale
    enable_raw_mode().expect("Impossibile abilitare Raw Mode");
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).expect("Errore setup terminale");
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).expect("Impossibile creare il terminale");

    // inizializzazione dello stato
    let mut tui_state = TuiState::new(state.clone());
    let mut reader = EventStream::new();
    let mut tick_rate = tokio::time::interval(std::time::Duration::from_millis(1000)); // refresh ogni 1000 ms

    // event loop
    loop {
        //disenga l'interfaccia ad ogni ciclo
        terminal
            .draw(|f| draw_ui(f, &mut tui_state))
            .expect("Errore di rendering");

        tokio::select! {
            // risveglio periodico
            _ = tick_rate.tick() => {
                // estrazione utenti connessi
                tui_state.active_users = state.get_connected_users().await;

                // estrazione ultimi messaggi
                // aggiunti i join per ottenere i nomi degli utenti mittente e destinatario
                // aggiunto is_read per sapere se il messaggio è stato letto o meno
                let query_result = sqlx::query(
                    r#"
                    SELECT 
                        m.kind, 
                        m.content, 
                        m.created_at_ms, 
                        m.is_read, 
                        u_sender.username AS sender_name, 
                        u_recip.username AS recipient_name
                    FROM messages m
                    LEFT JOIN users u_sender ON m.sender_id = u_sender.id
                    LEFT JOIN users u_recip ON m.recipient_id = u_recip.id
                    ORDER BY m.created_at_ms DESC 
                    LIMIT 20
                    "#
                )
                .fetch_all(&state.db)
                .await;

                if let Ok(rows) = query_result {
                    use sqlx::Row;
                    let mut msgs = Vec::new();
                    for row in rows {
                        let kind: String = row.get("kind");
                        let content: String = row.get("content");
                        let timestamp_ms: i64 = row.get("created_at_ms");
                        let is_read: bool = row.get("is_read");
                        let sender: Option<String> = row.get("sender_name");
                        let recipient: Option<String> = row.get("recipient_name");

                        let time_str = match chrono::DateTime::from_timestamp_millis(timestamp_ms) {
                            Some(dt) => dt.with_timezone(&chrono::Local).format("%d/%m/%Y %H:%M:%S").to_string(),
                            None => "--/--/---- --:--:--".to_string(),
                        };

                        let display_str = if kind == "broadcast" {
                            format!("📢 [{time_str}] [BROADCAST] {content}")
                        } else {
                            // messaggio diretto
                            let status_icon = if is_read { "✔✔" } else { "✔" };

                            if sender.is_none() {
                                // Inviato dall'Admin verso un Utente (Outbound)
                                let recip_name = recipient.as_deref().unwrap_or("Sconosciuto");
                                format!("📤 [{time_str}] [TO {recip_name}] {content} {status_icon}")
                            } else {
                                // Inviato da un Utente verso l'Admin (Inbound)
                                let sender_name = sender.as_deref().unwrap_or("Sconosciuto");
                                format!("📥 [{time_str}] [FROM {sender_name}] {content}")
                            }
                        };

                        msgs.push(display_str);
                    }
                    tui_state.recent_messages = msgs;
                }
            }
            // risveglio immediato alla pressione di un tasto
            Some(Ok(event)) = reader.next() => {
                match event {
                    Event::Key(key) => {
                        if key.kind == KeyEventKind::Press {
                            match key.code {
                                KeyCode::Char(c) => { tui_state.input.push(c); }
                                KeyCode::Backspace => { tui_state.input.pop(); }// Rimuove l'ultimo carattere
                                KeyCode::Enter => { tui_state.process_command().await; }
                                KeyCode::Esc => { tui_state.should_quit = true; }
                                _ => {}
                            }
                        }
                    }
                    Event::Mouse(mouse) => {
                        match mouse.kind {
                            MouseEventKind::ScrollUp => {
                                tui_state.scroll_offset = tui_state.scroll_offset.saturating_add(3) // scorre 3 righe alla volta
                            }
                            MouseEventKind::ScrollDown => {
                                tui_state.scroll_offset = tui_state.scroll_offset.saturating_sub(3);
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }

        if tui_state.should_quit {
            // ripristino essenziale
            disable_raw_mode().expect("Impossibile disabilitare Raw Mode");
            execute!(
                terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            )
            .expect("Errore ripristino");
            terminal
                .show_cursor()
                .expect("Impossibile mostrare il cursore");

            // invio segnale di shutdown al server
            let _ = shutdown_tx.send(());
            return;
        }
    }
}

// RENDER INTERFACCIA
fn draw_ui(f: &mut ratatui::Frame, state: &mut TuiState) {
    // LAYOUT PRINCIPALE: header, centro, footer
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header
            Constraint::Min(0),    // Main content
            Constraint::Length(1), // Footer
        ])
        .split(f.size());

    // LAYOUT CENTRALE: Console (75%) | Sidebar (25%)
    let center_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(main_chunks[1]);

    // LAYOUT CONSOLE (SINISTRA): logs, input
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // Logs
            Constraint::Length(3), // Input
        ])
        .split(center_chunks[0]);

    // HEADER & FOOTER
    let header = Paragraph::new(" GeoRust Admin Dashboard ").style(
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    f.render_widget(header, main_chunks[0]);

    let footer = Paragraph::new(" [ESC] Esci | [help] Comandi ")
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(footer, main_chunks[2]);

    // SPLIT DELLA SIDEBAR DESTRA
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40), // 40% per gli utenti
            Constraint::Percentage(60), // 60% per i messaggi
        ])
        .split(center_chunks[1]);

    // UTENTI LIVE (alto destra)
    let mut user_items = Vec::new();
    if state.active_users.is_empty() {
        user_items
            .push(ListItem::new("Nessuno online").style(Style::default().fg(Color::DarkGray)));
    } else {
        for id in &state.active_users {
            user_items.push(
                ListItem::new(format!("🟢 User #{}", id)).style(
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
            );
        }
    }

    let users_list = List::new(user_items).block(
        Block::default()
            .title(format!(" Live Users ({}) ", state.active_users.len()))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(users_list, right_chunks[0]);

    // MESSAGGI RECENTI (basso destra) con word wrap
    let msg_text = if state.recent_messages.is_empty() {
        "Nessun messaggio".to_string()
    } else {
        state.recent_messages.join("\n")
    };

    let msg_paragraph = Paragraph::new(msg_text)
        .block(
            Block::default()
                .title(" Recent Messages ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta)),
        )
        .wrap(ratatui::widgets::Wrap { trim: true });
    f.render_widget(msg_paragraph, right_chunks[1]);

    // LOGS (Centro Sinistra)
    let logs_text = state.logs.join("\n");
    let line_count = state.logs.len() as u16;
    let area_height = left_chunks[0].height.saturating_sub(2);

    let max_base_offset = line_count.saturating_sub(area_height);
    state.scroll_offset = state.scroll_offset.min(max_base_offset);
    let actual_scroll = max_base_offset - state.scroll_offset;

    let title = " Console ".to_string();
    let logs_block = Paragraph::new(logs_text)
        .block(Block::default().title(title).borders(Borders::ALL))
        .scroll((actual_scroll, 0));
    f.render_widget(logs_block, left_chunks[0]);

    // INPUT (Basso Sinistra)
    let input_text = format!("> {}", state.input);
    let input_block = Paragraph::new(input_text)
        .style(Style::default().fg(Color::Yellow))
        .block(
            Block::default()
                .title(" Invia Comando ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        );
    f.render_widget(input_block, left_chunks[1]);
}
*/