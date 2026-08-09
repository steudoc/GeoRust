use tokio::io::{self, AsyncBufReadExt, BufReader};
use sqlx::SqlitePool;

// Importiamo il modulo stats (usando crate:: per indicare che parte dalla root del server)
use crate::stats;

pub async fn start_admin_console(db_pool: SqlitePool) {
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin).lines();

    println!("Admin console ready. Press 'help' for command menu.");

    while let Ok(Some(line)) = reader.next_line().await {
        let input = line.trim();
        if input.is_empty() { continue; }
        
        let parts: Vec<&str> = input.split_whitespace().collect();
        match parts[0] {
            "stats" => {
                if parts.len() == 3 {
                    if let Ok(user_id) = parts[1].parse::<i64>() {
                        let interval = parts[2];
                        println!("Calculating stats for user {} in period {}...", user_id, interval);

                        match stats::calculate_user_stats(&db_pool, user_id, interval).await {
                            Ok(res) => {
                                println!("--------------------------------");
                                println!("User id:\t\t{}", user_id);
                                println!("Period:\t\t\t{}", res.period);
                                println!("Distance:\t\t{:.2}", res.distance);
                                println!("Total time:\t\t{:.2}", res.total_time);
                                println!("Total pause time:\t{:.2}", res.total_pause);
                                println!("Average velocity:\t{:.2}", res.avg_velocity);
                                println!("--------------------------------");
                            }
                            Err(e) => {
                                println!("Error in db data extraction: {}", e);
                            }
                        }
                    }
                } else {
                    println!("Usage: stats <user_id> <day|week|month>");
                }
            },
            "help" => println!("Commands available: \n\tstats <user_id> <day|week|month>\n\thelp"),
            _ => println!("Invalid command"),
        }
    }
}