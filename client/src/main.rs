mod movement;

use std::io::{self, Write};

use common::{LoginRequest, LoginResponse, RegisterRequest};
use futures_util::{
    stream::{SplitSink, SplitStream},
    Sink, SinkExt, Stream, StreamExt,
};
use rand::Rng;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
    MaybeTlsStream, WebSocketStream,
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
    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(10));
    // Skip the immediate first tick if you don't want a send right at connection time:
    // ticker.tick().await;

    loop {
        tokio::select! {
            // Branch 1: incoming messages from server
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        println!("Aggiornamento ricevuto: {text}");
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
            
            //TODO: edit here to send
            // Current implementation for try: send a random number every 10 seconds
            _ = ticker.tick() => {
                let random_number: u32 = rand::thread_rng().gen_range(0..1000);
                let payload = random_number.to_string();
                write.send(Message::Text(payload.clone())).await?;
                println!("Inviato numero casuale: {payload}");
            }
        }
    }

    Ok(())
}
