use std::{fmt::format, io::{self, Write}, time::Duration};

use common::{LoginRequest, LoginResponse, RegisterRequest, RegisterResponse};

const SERVER_HTTP: &str = "http://127.0.0.1:3000";

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

    Ok(())
}
