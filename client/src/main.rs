use std::io::{self, Write};

use chrono::Local;

use common::{LoginRequest, LoginResponse, RegisterRequest, WsClientMessage};
use futures_util::{
    stream::{SplitSink, SplitStream},
    Sink, SinkExt, Stream, StreamExt,
};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
    MaybeTlsStream, WebSocketStream,
};

use common::WsServerMessage::{self, BroadcastText, DirectText, Error};

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
    let (mut write, mut read) = open_client_side_socket(&login_resp.token).await?;
    do_client_side_socket_operations(&mut write, &mut read).await?;

    Ok(())
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
) -> anyhow::Result<()>
where
    S: Sink<Message> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
    R: Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let mut lines = BufReader::new(tokio::io::stdin()).lines();

    loop {
        println!("Simple Console - Type 'help' for commands");
        print!("> ");
        io::stdout().flush()?;

        tokio::select! {
            // Branch 1: incoming messages from server
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<WsServerMessage>(&text) {
                            Ok(DirectText { id, text, timestamp }) => {
                                let time_str = timestamp.with_timezone(&Local).format("%H:%M:%S");
                                print!("\r[Diretto #{id} {time_str}] {text}\n> ");
                                io::stdout().flush()?;

                                // Risposta automatica di ACK al server
                                let ack_payload = serde_json::to_string(&WsClientMessage::DirectTextAck { id })?;
                                let _ = write.send(Message::Text(ack_payload.into())).await;
                            },
                            Ok(BroadcastText { id, text, timestamp }) => {
                                let time_str = timestamp.with_timezone(&Local).format("%H:%M:%S");
                                print!("\r[Broadcast #{id} {time_str}] {text}\n> ");
                                io::stdout().flush()?;
                            },
                            Ok(Error { code, message }) => {
                                print!("\r[Errore {code}] {message}\n> ");
                                io::stdout().flush()?;
                            },
                            Err(e) => {
                                print!("\rErrore di deserializzazione dal server: {e}\n> ");
                                io::stdout().flush()?;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        println!("\rIl server ha chiuso la connessione.");
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        println!("\rErrore di rete: {e}");
                        break;
                    }
                    None => {
                        println!("\rConnessione terminata.");
                        break;
                    }
                }
            }

            line = lines.next_line() => {
                let Some(line) = line? else {
                    println!("Input chiuso");
                    break;
                };

                let mut parts = line.trim().splitn(2, char::is_whitespace);
                let command = parts.next().unwrap_or("");
                let argument = parts.next().unwrap_or("").trim();

                match command {
                    "help" => {
                        println!("Available commands:");
                        println!("help - Show this help message");
                        println!("msg <message> - Send a message to the server");
                        println!("exit - Exit the console");
                    }
                    "msg" => {
                        if argument.is_empty() {
                            println!("Usage: msg <message>");
                            continue;
                        }

                        let payload = serde_json::to_string(&WsClientMessage::Text {
                            text: argument.to_string(),
                        })?;

                        write.send(Message::Text(payload.into())).await?;
                        println!("Messaggio inviato");
                    }
                    "exit" => {
                        println!("Exiting console...");
                        break;
                    }
                    "" => {}
                    _ => {
                        println!("Unknown command: {command}");
                    }
                }

                print!("> ");
                io::stdout().flush()?;
            }
            
        }
    }

    Ok(())
}