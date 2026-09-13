mod console;
mod movement;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    console::run().await
}
