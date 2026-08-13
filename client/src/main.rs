mod movement;

use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};
use common::{LoginRequest, LoginResponse, RegisterRequest, WsClientMessage, WsServerMessage};
use futures_util::{
    Sink, SinkExt, Stream, StreamExt,
    stream::{SplitSink, SplitStream},
};
use movement::RouteSimulator;
use tokio::net::TcpStream;
use tokio::sync::mpsc;  // Canale usato per mettere in comunicazione il simulatore RouteSimulator e la websocket
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};

const SERVER_HTTP: &str = "http://127.0.0.1:3000";
const SERVER_WS: &str = "ws://127.0.0.1:3000/ws";




// Nota: pwd letta e mostrata in chiaro. Per nascondere input vedi crate 'rpassword'
fn read_line(message: &str) -> anyhow::Result<String> {
    print!("{message}");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let http = reqwest::Client::new();

    let login_resp: LoginResponse = loop {
        let choice = loop {
            let answer = read_line("Register [r] - Login [l] ? ")?;
            match answer.to_lowercase().as_str() {
                "r" | "register" => break "register",
                "l" | "login" => break "login",
                _ => println!("Invalid command, valid command: 'r', 'l'\n"),
            }
        };

        let username = read_line("Username: ")?;
        let password = read_line("Password: ")?;

        if choice == "register" {
            match http
                .post(format!("{SERVER_HTTP}/register"))
                .json(&RegisterRequest {
                    username: username.clone(),
                    password: password.clone(),
                })
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    println!("Registration: OK");
                }
                Ok(resp) if resp.status() == reqwest::StatusCode::CONFLICT => {
                    println!("Registration: FAILED. \nUsername already registered, trying login...\n");
                }
                Ok(resp) => {
                    println!("Registration: FAILED ({}).\n", resp.status());
                    continue; // torna all'inizio del loop
                }
                Err(e) => {
                    println!("Errore during registration: {e}. Please retry.\n");
                    continue;
                }
            }
        }

        let resp = match http
            .post(format!("{SERVER_HTTP}/login"))
            .json(&LoginRequest {
                username: username.clone(), 
                password: password.clone()
            })
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                println!("Error during login: {e}. Please retry\n");
                continue;
            }
        };

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            println!("Login: FAILED. Wrong credentials, retry.\n");
            continue;
        }
        if !resp.status().is_success() {
            println!("Login: FAILED ({}). Please retry.\n", resp.status());
            continue;
        }

        match resp.json::<LoginResponse>().await {
            Ok(login_resp) => break login_resp,
            Err(e) => {
                println!("Invalid answer from the server: {e}. Please retry.\n");
                continue;
            }
        }
    };

    println!("Login: OK, user_id = {}", login_resp.user_id);

    let default_csv_path = Path::new("G8/data/Torino-Asti.csv");
    let csv_path = read_csv_path(&default_csv_path)?;
    let speed_factor = read_speed_factor()?;
    let route = movement::load_route(&csv_path)?;
    let simulator = RouteSimulator::new(route, speed_factor)?;

    println!("Percorso caricato da {}", csv_path.display());
    println!("Velocita simulazione: {speed_factor}x");

    let (mut write, mut read) = open_client_side_socket(&login_resp.token).await?;
    do_client_side_socket_operations(&mut write, &mut read, simulator).await?;

    Ok(())
}

/// Permette all'utente di scegliere il file CSV, fornendone uno di default
fn read_csv_path(default_path: &Path) -> anyhow::Result<PathBuf> {
    let input = read_line(
        &format!("File CSV del percorso [{}]: ", default_path.display()
    ))?;

    if input.is_empty() {
        Ok(default_path.to_path_buf())
    } else {
        Ok(PathBuf::from(input))
    }
}

/// Impostare quanto velocemente riprodurre il percorso simulato
fn read_speed_factor() -> anyhow::Result<u32> {
    let input = read_line("Fattore di velocita [1]: ")?;

    if input.is_empty() {
        return Ok(1);   // Riprodotto in tempo reale
    }

    input
        .parse::<u32>()
        .map_err(|_| anyhow::anyhow!("Il fattore di velocita deve essere un numero intero"))
}


async fn open_client_side_socket(
    token: &str,
) -> anyhow::Result<(
    SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
    SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
)> {
    let mut request = SERVER_WS.into_client_request()?;
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse()?);

    let (ws_stream, _response) = connect_async(request).await?;
    println!("Connesso a {SERVER_WS}");

    Ok(ws_stream.split())
}


async fn do_client_side_socket_operations<S, R>(
    write: &mut S,
    read: &mut R,
    simulator: RouteSimulator,
) -> anyhow::Result<()>
where
    S: Sink<Message> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
    R: Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let (route_sender, mut route_receiver) = mpsc::channel(16); // Creazione del canale tra simulatore e WebSocket di al massimo 16 punti non letti
    let simulator_task = tokio::spawn(simulator.start(route_sender));   // Avvio simulatore
    let mut route_finished = false; // tag per fine simulazione e inviare il messaggio di fine TripCompleted

    loop {
        tokio::select! {

            // Ricezione dal server
            msg = read.next() => {
                match msg {
                    /*
                    msg è del tipo Option<Result<Message, Error>> con
                        Option: per la connessione aperta o chiusa,
                        Result: lettura riuscita o no,
                        Message: quale frame WebSocket è arrivato
                    */
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<WsServerMessage>(&text) {
                            Ok(WsServerMessage::PositionAccepted { stato, coord_ricevute }) => {
                                println!(
                                    "Posizione {coord_ricevute} accettata; stato utente: {stato:?}"
                                );
                            }
                            Ok(WsServerMessage::TripCompleted {
                                numero_coord,
                                tempo_movimento,
                                tempo_fermo,
                            }) => {
                                println!(
                                    "Tragitto completato: {numero_coord} punti, \
                                     movimento {tempo_movimento}s, fermo {tempo_fermo}s"
                                );
                                break;  // Termine loop websocket perchè il client ha finito il lavoro -> UNICO punto di uscita
                            }
                            Ok(WsServerMessage::Error { code, message }) => {
                                // Non interrompiamo il loop in caso di errore, restiamo collegati così da rifiutare una posizione errata senza abbattere la connesione
                                eprintln!("Errore dal server [{code}]: {message}");
                            }
                            Ok(other) => println!("Aggiornamento ricevuto: {other:?}"), // Prr BroadcastText e DirectText
                            Err(error) => {
                                // Messaggio che non si può trasformare in WsServerMessage
                                eprintln!("Risposta non valida dal server: {error}");
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        println!("Server ha chiuso la connessione");
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        println!("Errore: {e}");
                        break;
                    }
                    None => {
                        println!("Connessione chiusa");
                        break;
                    }
                }
            }

            // Ricezione dal simulatore e invio al server
            point = route_receiver.recv(), if !route_finished => {
                match point {
                    Some(point) => {
                        // Simulatore ha emmesso una nuova coordinata
                        let message = WsClientMessage::PositionUpdate {
                            coordinata: point.coordinates,
                        };
                        let json = serde_json::to_string(&message)?;

                        write.send(Message::Text(json)).await?;
                        println!("Inviata posizione a t={}s", point.elapsed.as_secs()); // Mostriamo il tempo del CSV, non quello registrato dal server
                    }
                    None => {
                        // Simulatore ha terminato: distrutto Sender del canale
                        let json = serde_json::to_string(&WsClientMessage::TripCompleted)?;
                        write.send(Message::Text(json)).await?;
                        route_finished = true;
                        println!("Tutte le posizioni sono state inviate");
                    }
                }
            }
        }
    }

    drop(route_receiver); // Chiusura esplicita lato ricevente così il simulatore non continuare a produrre punti
    let emitted_points = simulator_task.await?;
    println!("Il simulatore ha emesso {emitted_points} punti");

    Ok(())
}
