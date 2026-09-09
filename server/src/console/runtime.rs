use std::{io, sync::Arc, time::Duration};

use anyhow::Context;
use crossterm::{
    event::{
        DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyCode, KeyEventKind,
        KeyModifiers, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures_util::StreamExt;
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::time::MissedTickBehavior;

use super::{
    app::{AppAction, ServerApp},
    tui,
};
use crate::AppState;
use crate::stats;

type AppTerminal = Terminal<CrosstermBackend<io::Stdout>>;

pub(super) struct TerminalSession {
    terminal: AppTerminal,
}

impl TerminalSession {
    fn new() -> anyhow::Result<Self> {
        enable_raw_mode().context("Impossibile abilitare la modalità raw")?;
        let mut stdout = io::stdout();

        if let Err(error) = execute!(stdout, EnterAlternateScreen, EnableMouseCapture) {
            let _ = disable_raw_mode();
            return Err(error).context("Impossibile aprire la schermata della console");
        }

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = match Terminal::new(backend) {
            Ok(terminal) => terminal,
            Err(error) => {
                let mut stdout = io::stdout();
                let _ = execute!(stdout, LeaveAlternateScreen, DisableMouseCapture);
                let _ = disable_raw_mode();
                return Err(error).context("Impossibile inizializzare Ratatui");
            }
        };

        terminal.hide_cursor()?;
        terminal.clear()?;
        Ok(Self { terminal })
    }

    pub fn terminal_mut(&mut self) -> &mut AppTerminal {
        &mut self.terminal
    }

    fn restore(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        );
        let _ = self.terminal.show_cursor();
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        self.restore();
    }
}

pub async fn run(app_state: Arc<AppState>) -> anyhow::Result<()> {
    let mut terminal = TerminalSession::new()?;
    let mut events = EventStream::new();

    run_dashboard(&mut terminal, &mut events, app_state).await
}

async fn run_dashboard(
    terminal: &mut TerminalSession,
    events: &mut EventStream,
    app_state: Arc<AppState>,
) -> anyhow::Result<()> {
    let mut app = ServerApp::new();

    app.push_console("Server avviato con successo. In ascolto sulla porta 3000.");
    app.push_console("Digita 'help' per vedere la lista dei comandi.");

    let mut background_tick = tokio::time::interval(Duration::from_secs(1));
    background_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut should_quit = false;

    while !should_quit {
        // Disegna l'interfaccia usando la vista tui.rs
        terminal
            .terminal_mut()
            .draw(|frame| tui::draw_dashboard(frame, &mut app))?;

        tokio::select! {
            event = events.next() => {
                let Some(event) = event else {
                    should_quit = true;
                    continue;
                };
                match event? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                            should_quit = true;
                            continue;
                        }

                        match key.code {
                            KeyCode::Char(character) => app.input.push(character),
                            KeyCode::Backspace => { app.input.pop(); }
                            KeyCode::Enter => {
                                match app.submit_input() {
                                    AppAction::None => {}
                                    AppAction::Exit => should_quit = true,
                                    
                                    // GESTIONE DEI COMANDI DEL SERVER:
                                    AppAction::SeeUsers => {
                                        let users = app_state.get_registered_users().await;
                                        if users.is_empty() {
                                            app.push_console("Nessun utente registrato o errore nel database.");
                                        } else {
                                            app.push_console(format!("Utenti registrati ({}):", users.len()));
                                            for (id, username) in users {
                                                app.push_console(format!("  [#{}] {}", id, username));
                                            }
                                        }
                                    }
                                    AppAction::SeeLogs { username } => {
                                        app.push_console(format!("Estrazione ultimi 10 logs per utente {}...", username));
                                        
                                        match app_state.get_user_logs(&username).await {
                                            Ok(messages) if messages.is_empty() => {
                                                app.push_console(format!("Nessun messaggio trovato per l'utente '{}'.", username));
                                            }
                                            Ok(messages) => {
                                                app.push_console("--------------------------------");
                                                
                                                for msg in messages {
                                                    // Formattazione della data
                                                    let time_str = match chrono::DateTime::from_timestamp_millis(msg.timestamp_ms) {
                                                        Some(dt) => dt.with_timezone(&chrono::Local).format("%d/%m/%Y %H:%M:%S").to_string(),
                                                        None => msg.timestamp_ms.to_string(),
                                                    };

                                                    // Mappatura della tipologia
                                                    let kind_str = match msg.kind.as_str() {
                                                        "direct" => "SERVER → USER",
                                                        "client_to_server" => "USER → SERVER", // se usi questo nel DB
                                                        "broadcast" => "BROADCAST",
                                                        _ => "UNKNOWN",
                                                    };

                                                    // Stato di lettura
                                                    let read_status = match (kind_str, msg.is_read) {
                                                        ("SERVER → USER", true) => " (READ)",
                                                        ("SERVER → USER", false) => " (PENDING)",
                                                        _ => "",
                                                    };

                                                    // Push effettivo nella grafica
                                                    app.push_console(format!(
                                                        "[{}] [{}] {}{}", 
                                                        time_str, 
                                                        kind_str, 
                                                        msg.content, 
                                                        read_status
                                                    ));
                                                }
                                                app.push_console("--------------------------------");
                                            }
                                            Err(e) => {
                                                app.push_console(format!("Errore log: {}", e));
                                                tracing::error!("Errore DB in get_user_logs: {}", e);
                                            }
                                        }
                                    }
                                    AppAction::SendBroadcast { text } => {
                                        match app_state.message_service.send_admin_broadcast_message(&text).await {
                                            Ok(_) => app.push_console("Broadcast inviato con successo!"),
                                            Err(e) => app.push_console(format!("Errore broadcast: {}", e)),
                                        }
                                    }
                                    AppAction::SendDirect { username, text } => {
                                        match app_state.message_service.send_admin_direct_message(&username, &text).await {
                                            Ok(_) => app.push_console(format!("Messaggio inviato all'utente {}", username)),
                                            Err(e) => app.push_console(format!("Errore: {}", e)),
                                        }
                                    }
                                    AppAction::CalculateStats { username, interval } => {
                                        if let Some(user_id) = app_state.get_user_id(&username).await {
                                            app.push_console(format!("Calcolo statistiche per #{} {} ({})", user_id, username, interval));
                                            
                                            match stats::calculate_user_stats(&app_state.db, user_id, &interval).await {
                                                Ok(res) => {
                                                    app.push_console("--------------------------------".to_string());
                                                    app.push_console(format!("Username:         {}", username));
                                                    app.push_console(format!("Period:           {}", res.period));
                                                    app.push_console(format!("Distance:         {:.2} km", res.distance));
                                                    app.push_console(format!("Total time:       {:.2} s", res.total_time));
                                                    app.push_console(format!("Total pause time: {:.2} s", res.total_pause));
                                                    app.push_console(format!("Average velocity: {:.2} km/h", res.avg_velocity));
                                                    app.push_console("--------------------------------".to_string());
                                                }
                                                Err(e) => app.push_console(format!("Errore db: {}", e)),
                                            }
                                        } else {
                                            app.push_console(format!("Errore: l'utente '{}' non esiste.", username));
                                        }                               
                                    }
                                }
                            }
                            KeyCode::PageUp => app.scroll_up(),
                            KeyCode::PageDown => app.scroll_down(),
                            _ => {}
                        }
                    }
                    Event::Mouse(mouse) => match mouse.kind {
                        MouseEventKind::ScrollUp => app.scroll_up(),
                        MouseEventKind::ScrollDown => app.scroll_down(),
                        _ => {}
                    },
                    _ => {}
                }
            }

            _ = background_tick.tick() => {
                app.active_users = app_state.get_connected_users().await;

                app.messages = app_state.get_msg().await;
            }
        }
    }

    Ok(())
}
