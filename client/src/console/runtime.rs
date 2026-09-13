use std::{future::pending, io, time::Duration};

use anyhow::Context;
use common::{WsClientMessage, WsServerMessage};
use crossterm::{
    event::{
        DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyCode, KeyEventKind,
        KeyModifiers, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures_util::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::{net::TcpStream, sync::mpsc, task::JoinHandle, time::MissedTickBehavior};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Error as WebSocketError, Message, client::IntoClientRequest},
};

use crate::movement::{RoutePoint, RouteSimulator};

use super::{
    app::{AppAction, ClientApp, ConnectionStatus, RouteOption, TripStatus},
    auth, ui,
};

const SERVER_WS: &str = "ws://127.0.0.1:3000/ws";
const RECONNECT_INTERVAL: Duration = Duration::from_secs(2);

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
type SocketWriter = SplitSink<Socket, Message>;
type SocketReader = SplitStream<Socket>;
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

struct ActiveSimulator {
    receiver: mpsc::Receiver<RoutePoint>,
    task: Option<JoinHandle<usize>>,
}

impl Drop for ActiveSimulator {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

enum RouteEvent {
    Point(RoutePoint),
    Finished,
}

pub(super) async fn run() -> anyhow::Result<()> {
    let mut terminal = TerminalSession::new()?;
    let mut events = EventStream::new();
    let http = reqwest::Client::new();

    let Some(user) = auth::authenticate(&mut terminal, &mut events, &http).await? else {
        return Ok(());
    };

    run_dashboard(&mut terminal, &mut events, user).await
}

async fn run_dashboard(
    terminal: &mut TerminalSession,
    events: &mut EventStream,
    user: auth::AuthenticatedUser,
) -> anyhow::Result<()> {
    let mut app = ClientApp::new(user.user_id, user.username);
    let token = user.token;
    let mut socket_writer = None;
    let mut socket_reader = None;
    let mut simulator: Option<ActiveSimulator> = None;

    match connect_socket(&token).await {
        Ok((writer, reader)) => {
            socket_writer = Some(writer);
            socket_reader = Some(reader);
            app.mark_connected();
        }
        Err(error) if is_user_already_connected(&error) => {
            anyhow::bail!("Accesso rifiutato: questo utente è già connesso da un altro client");
        }
        Err(error) => {
            app.mark_disconnected(format!("Connessione WebSocket fallita: {error}"));
        }
    }

    let mut reconnect_tick = tokio::time::interval(RECONNECT_INTERVAL);
    reconnect_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let mut should_quit = false;
    while !should_quit {
        terminal
            .terminal_mut()
            .draw(|frame| ui::draw_dashboard(frame, &mut app))?;

        let mut disconnect_reason: Option<String> = None;

        tokio::select! {
            event = events.next() => {
                let Some(event) = event else {
                    should_quit = true;
                    continue;
                };
                match event? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        if key.code == KeyCode::Char('c')
                            && key.modifiers.contains(KeyModifiers::CONTROL)
                        {
                            should_quit = true;
                            continue;
                        }

                        match key.code {
                            KeyCode::Char(character) => app.input.push(character),
                            KeyCode::Backspace => {
                                app.input.pop();
                            }
                            KeyCode::Enter => {
                                match app.submit_input() {
                                    AppAction::None => {}
                                    AppAction::Exit => should_quit = true,
                                    AppAction::SendText(text) => {
                                        let message = WsClientMessage::Text { text: text.clone() };
                                        match send_protocol_message(&mut socket_writer, &message).await {
                                            Ok(()) => app.add_sent_message(text),
                                            Err(error) => {
                                                disconnect_reason = Some(format!(
                                                    "Invio del messaggio fallito: {error}"
                                                ));
                                            }
                                        }
                                    }
                                    AppAction::StartTrip { route, speed_factor } => {
                                        if let Err(error) = start_trip(
                                            &mut app,
                                            &mut simulator,
                                            &mut socket_writer,
                                            route,
                                            speed_factor,
                                        )
                                        .await
                                        {
                                            disconnect_reason = Some(format!(
                                                "Avvio del trip fallito: {error}"
                                            ));
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

            server_event = next_server_event(&mut socket_reader) => {
                match server_event {
                    Some(Ok(Message::Text(text))) => {
                        if let Err(error) = handle_server_text(
                            &mut app,
                            &mut socket_writer,
                            &mut simulator,
                            &text,
                        )
                        .await
                        {
                            disconnect_reason = Some(format!("Errore WebSocket: {error}"));
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        disconnect_reason = Some("Il server ha chiuso la connessione.".to_string());
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if let Some(writer) = socket_writer.as_mut()
                            && writer.send(Message::Pong(payload)).await.is_err()
                        {
                            disconnect_reason = Some("Impossibile rispondere al server.".to_string());
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(error)) => {
                        disconnect_reason = Some(format!("Connessione WebSocket interrotta: {error}"));
                    }
                }
            }

            route_event = next_route_event(&mut simulator) => {
                match route_event {
                    RouteEvent::Point(point) => {
                        let elapsed_seconds = point.elapsed.as_secs();
                        let latitude = point.coordinates.get_latitudine();
                        let longitude = point.coordinates.get_longitudine();
                        let message = WsClientMessage::PositionUpdate {
                            coordinata: point.coordinates,
                            elapsed_seconds,
                        };

                        match send_protocol_message(&mut socket_writer, &message).await {
                            Ok(()) => {
                                app.record_sent_position(elapsed_seconds, latitude, longitude);
                            }
                            Err(error) => {
                                disconnect_reason = Some(format!(
                                    "Invio della posizione fallito: {error}"
                                ));
                            }
                        }
                    }
                    RouteEvent::Finished => {
                        if let Some(mut completed_simulator) = simulator.take()
                            && let Some(task) = completed_simulator.task.take()
                            && let Err(error) = task.await
                        {
                            app.push_console(format!("Errore del simulatore: {error}"));
                        }

                        match send_protocol_message(
                            &mut socket_writer,
                            &WsClientMessage::TripCompleted,
                        )
                        .await
                        {
                            Ok(()) => app.mark_route_finished(),
                            Err(error) => {
                                disconnect_reason = Some(format!(
                                    "Impossibile completare il trip: {error}"
                                ));
                            }
                        }
                    }
                }
            }

            _ = reconnect_tick.tick(), if socket_reader.is_none() => {
                match connect_socket(&token).await {
                    Ok((writer, reader)) => {
                        socket_writer = Some(writer);
                        socket_reader = Some(reader);
                        app.mark_connected();
                    }
                    Err(_) => {
                        app.connection_status = ConnectionStatus::Reconnecting;
                    }
                }
            }
        }

        if let Some(reason) = disconnect_reason {
            socket_writer = None;
            socket_reader = None;

            if matches!(
                app.trip_status,
                TripStatus::Running { .. } | TripStatus::AwaitingCompletion { .. }
            ) {
                simulator = None;
                app.invalidate_active_trip(
                    "Trip interrotto dalla perdita di connessione: non verrà completato dal client.",
                );
            }

            app.mark_disconnected(reason);
        }
    }

    // Uscita volontaria: il simulatore viene annullato e NON viene inviato TripCompleted.
    simulator = None;
    if let Some(mut writer) = socket_writer {
        let _ = tokio::time::timeout(
            Duration::from_millis(250),
            writer.send(Message::Close(None)),
        )
        .await;
    }

    drop(simulator);
    Ok(())
}

async fn start_trip(
    app: &mut ClientApp,
    simulator: &mut Option<ActiveSimulator>,
    socket_writer: &mut Option<SocketWriter>,
    route: RouteOption,
    speed_factor: u32,
) -> anyhow::Result<()> {
    let route_points = match crate::movement::load_route(&route.path) {
        Ok(route_points) => route_points,
        Err(error) => {
            app.push_console(format!("Percorso non valido: {error}"));
            app.invalidate_active_trip("Il trip non è stato avviato.");
            return Ok(());
        }
    };

    let route_simulator = match RouteSimulator::new(route_points, speed_factor) {
        Ok(route_simulator) => route_simulator,
        Err(error) => {
            app.push_console(format!("Impossibile avviare il simulatore: {error}"));
            app.invalidate_active_trip("Il trip non è stato avviato.");
            return Ok(());
        }
    };

    send_protocol_message(socket_writer, &WsClientMessage::StartTrip)
        .await
        .context("Impossibile richiedere al server l'avvio del trip")?;

    let (sender, receiver) = mpsc::channel(16);
    let task = tokio::spawn(route_simulator.start(sender));
    *simulator = Some(ActiveSimulator {
        receiver,
        task: Some(task),
    });
    app.mark_trip_started(route.name, speed_factor);
    Ok(())
}

async fn connect_socket(token: &str) -> anyhow::Result<(SocketWriter, SocketReader)> {
    let mut request = SERVER_WS.into_client_request()?;
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse()?);

    let (socket, _) = connect_async(request).await?;
    Ok(socket.split())
}

fn is_user_already_connected(error: &anyhow::Error) -> bool {
    matches!(
        error.downcast_ref::<WebSocketError>(),
        Some(WebSocketError::Http(response)) if response.status().as_u16() == 409
    )
}

async fn send_protocol_message(
    writer: &mut Option<SocketWriter>,
    message: &WsClientMessage,
) -> anyhow::Result<()> {
    let writer = writer
        .as_mut()
        .context("connessione al server non disponibile")?;
    let payload = serde_json::to_string(message)?;
    writer.send(Message::Text(payload)).await?;
    Ok(())
}

async fn handle_server_text(
    app: &mut ClientApp,
    writer: &mut Option<SocketWriter>,
    simulator: &mut Option<ActiveSimulator>,
    text: &str,
) -> anyhow::Result<()> {
    match serde_json::from_str::<WsServerMessage>(text) {
        Ok(WsServerMessage::TripStarted) => {
            app.push_console("Server: creazione del trip confermata.");
        }
        Ok(WsServerMessage::PositionAccepted {
            stato,
            coord_ricevute,
        }) => {
            app.push_console(format!(
                "Server: posizione {coord_ricevute} accettata, stato {stato:?}."
            ));
        }
        Ok(WsServerMessage::TripCompleted {
            numero_coord,
            tempo_movimento,
            tempo_fermo,
        }) => {
            app.mark_trip_completed(numero_coord, tempo_movimento, tempo_fermo);
        }
        Ok(WsServerMessage::Error { code, message }) => {
            app.push_console(format!("Errore server [{code}]: {message}"));

            if code == "trip_already_active" {
                *simulator = None;
                app.invalidate_active_trip(
                    "Il simulatore è stato arrestato perché il server ha rifiutato il nuovo trip.",
                );
            }
        }
        Ok(WsServerMessage::DirectText {
            id,
            text,
            timestamp,
        }) => {
            app.add_direct_message(text, timestamp);
            send_protocol_message(writer, &WsClientMessage::DirectTextAck { id }).await?;
        }
        Ok(WsServerMessage::BroadcastText { text, timestamp }) => {
            app.add_broadcast_message(text, timestamp);
        }
        Err(error) => {
            app.push_console(format!("Risposta non valida dal server: {error}"));
        }
    }

    Ok(())
}

async fn next_server_event(
    reader: &mut Option<SocketReader>,
) -> Option<Result<Message, tokio_tungstenite::tungstenite::Error>> {
    match reader {
        Some(reader) => reader.next().await,
        None => pending().await,
    }
}

async fn next_route_event(simulator: &mut Option<ActiveSimulator>) -> RouteEvent {
    match simulator {
        Some(simulator) => match simulator.receiver.recv().await {
            Some(point) => RouteEvent::Point(point),
            None => RouteEvent::Finished,
        },
        None => pending().await,
    }
}
