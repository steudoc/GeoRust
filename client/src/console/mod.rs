mod app;
mod auth;
mod runtime;
mod ui;

pub async fn run() -> anyhow::Result<()> {
    runtime::run().await
}
