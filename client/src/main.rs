use std::{fmt::format, time::Duration};

use common::{LoginRequest, LoginResponse, RegisterRequest, RegisterResponse};

const SERVER_HTTP: &str = "http://127.0.0.1:3000";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let http = reqwest::Client::new();

    // registrazione
    let username = "utente_demo".to_string();
    let password = "password".to_string();

    let register_resp = http
        .post(format!("{SERVER_HTTP}/register"))
        .json(&RegisterRequest {
            username: username.clone(),
            password: password.clone(),
        })
        .send()
        .await?;

    if register_resp.status().is_success() {
        println!("Registration: OK");
    } else {
        println!(
            "Registration: FAILED ({}) \nTrying login...",
            register_resp.status()
        )
    }

    // login
    let login_resp: LoginResponse = http
        .post(format!("{SERVER_HTTP}/login"))
        .json(&LoginRequest {username, password})
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    println!("Login: OK, user_id = {}", login_resp.user_id);

    Ok(())
}
