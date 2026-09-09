pub(crate) mod app;
mod runtime;
mod tui;

use std::sync::Arc;

use crate::state::AppState;

pub async fn run(state: Arc<AppState>) -> anyhow::Result<()> {
    runtime::run(state).await
}
