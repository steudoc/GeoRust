use std::sync::Arc;

use crate::state::AppState;


type UserId = i64;

/**
 * A simple console for interacting with the server.
 */
pub struct SimpleConsole {
    state: Arc<AppState>,
}

impl SimpleConsole {
    
    /**
     * Creates a new instance of SimpleConsole with the given AppState.
     */
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /**
     * Runs the console, reading commands from standard input and executing them.
     */
    pub async fn run(&self) -> anyhow::Result<()> {
        // Loop until the user types "exit"
        loop {
            println!("Simple Console - Type 'help' for commands");
            println!("> ");

            // Read input from the user
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let input = input.trim().split_whitespace().collect::<Vec<&str>>();

            if input.is_empty() {
                continue;
            }

            // Match the input command and execute the corresponding action
            match input[0] {
                "help" => {
                    println!("Available commands:");
                    println!("help - Show this help message");
                    println!("list_users - List all connected users");
                    println!("msg <user_id> <message> - Send a message to a user");
                    println!("broadcast <message> - Send a message to all connected users");
                    println!("exit - Exit the console");
                },
                "list_users" => {
                    println!("Unimplemented");
                },
                "msg" => {
                    if input.len() < 3 {
                        println!("Usage: msg <user_id> <message>");
                        continue;
                    }

                    let user_id = input[1].parse::<UserId>();
                    if let Err(_) = user_id {
                        println!("Id utente non valido: {}", input[1]);
                        continue;
                    }
                    let message = input[2..].join(" ");
                    let user_id = user_id.unwrap();
                    if let Err(e) = self.state.send_direct_message(&user_id, message).await {
                        println!("Errore: {e}");
                    }
                },
                "broadcast" => {
                    if input.len() < 2 {
                        println!("Usage: broadcast <message>");
                        continue;
                    }

                    let message = input[1..].join(" ");
                    if let Err(e) = self.state.send_broadcast_message(message) {
                        println!("Errore: {e}");
                    }
                },
                "exit" => {
                    println!("Exiting console...");
                    break;
                },
                _ => {
                    println!("Unknown command: {}", input.join(" "));
                }
            }
        }
        Ok(())
    }
}