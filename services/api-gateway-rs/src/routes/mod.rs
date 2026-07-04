mod agent_chat;
mod browser;
mod rest;
mod share;
mod ws_multiplex;

use crate::state::AppState;
use axum::Router;

pub fn router(state: &AppState) -> Router<AppState> {
    Router::new()
        .merge(rest::router(state))
        .merge(share::router())
        .merge(browser::router())
        .merge(ws_multiplex::router())
        .merge(agent_chat::router(state))
}
