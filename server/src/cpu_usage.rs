use chrono::Local;
use sysinfo::System;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::time::{self, Duration};

const LOG_INTERVAL_SECS: u64 = 120;
const LOG_FILENAME: &str = "logs/cpu_usage_log.txt";

pub async fn start_cpu_logger() {
    let mut sys = System::new_all();
    let pid = sysinfo::get_current_pid().expect("Impossible to retrieve PID");
    let mut interval = time::interval(Duration::from_secs(LOG_INTERVAL_SECS));

    // Il primo tick di tokio::time::interval è immediato:
    // lo consumiamo, così il primo log arriva dopo 120 secondi.
    interval.tick().await;

    let mut total_cpu_time_sec = 0.0_f32;

    loop {
        interval.tick().await;
        sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]));

        if let Some(process) = sys.process(pid) {
            let cpu_usage_percent = process.cpu_usage();
            let run_time_sec = process.run_time();

            let interval_cpu_time_sec = cpu_usage_percent / 100.0 * LOG_INTERVAL_SECS as f32;

            total_cpu_time_sec += interval_cpu_time_sec;

            let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            let log_message = format!(
                "[{timestamp}] PID: {pid} | CPU Usage: {cpu_usage_percent:.2}% | Estimated CPU Time (last 120s): {interval_cpu_time_sec:.2}s | Estimated CPU Time (total): {total_cpu_time_sec:.2}s | Uptime process: {run_time_sec}s\n"
            );

            match OpenOptions::new()
                .create(true)
                .append(true)
                .open(LOG_FILENAME)
                .await
            {
                Ok(mut file) => {
                    if let Err(e) = file.write_all(log_message.as_bytes()).await {
                        eprintln!("Error in writing CPU log: {}", e);
                    }
                }
                Err(e) => eprintln!("Impossible opening CPU log: {}", e),
            }
        }
    }
}
