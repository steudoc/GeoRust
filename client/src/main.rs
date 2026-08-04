use std::{fmt::format, io::{self, Write}, time::Duration};

use common::{LoginRequest, LoginResponse, RegisterRequest, RegisterResponse};

const SERVER_HTTP: &str = "http://127.0.0.1:3000";
use futures_util::StreamExt;
use tokio_tungstenite::{connect_async, tungstenite::{client::IntoClientRequest, Message}};

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
    const SERVER_WS: &str = "ws://127.0.0.1:3000/ws";

    let login_resp: LoginResponse = loop {
        // scelta registrazione/login da terminale
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
                    // username già esistente, proviamo il login
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

        // login - sempre eseguito
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
    //if login is okay, need open socket passing upgrade token:
    let mut request = SERVER_WS.into_client_request()?;
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {}", login_resp.token).parse()?);

    let (ws_stream, _response) = connect_async(request).await?;
    println!("Connesso a {SERVER_WS}");

    let (_write, mut read) = ws_stream.split();

    while let Some(msg) = read.next().await {
        //TODO: Here we receive messages from the server. We use this as receiving channel for both.
        //In the initial implementationwhen we receive a message we just print it to the console. In the future we will use this channel to receive messages from the server and update the state of the client accordingly. 
        match msg {
            Ok(Message::Text(text)) => {
                println!("Aggiornamento ricevuto: {text}");
            }
            Ok(Message::Close(_)) => {
                println!("Server ha chiuso la connessione");
                break;
            }
            Ok(_) => {}
            Err(e) => {
                println!("Errore: {e}");
                break;
            }
        }
    }

    Ok(())
}
