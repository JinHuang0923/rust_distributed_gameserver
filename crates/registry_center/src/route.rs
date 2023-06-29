use std::sync::Arc;

use axum::{routing::get, Router};
use tokio::sync::Mutex;

use crate::{
    app_context::AppContext,
    handler::{get_load_banlance_connection, get_master, nodes, node_states},
};

pub async fn init_router(context: Arc<Mutex<AppContext>>) -> Router {
    Router::new()
        .route("/nodes", get(nodes))
        .route("/connection", get(get_load_banlance_connection))
        .route("/master", get(get_master))
        .route("/state", get(node_states))
        .with_state(context)
}
