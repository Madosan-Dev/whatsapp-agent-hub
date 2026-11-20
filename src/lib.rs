pub mod config;
pub mod db;
pub mod error;
pub mod middleware;
pub mod models;
pub mod routes;
pub mod services;

use axum::{
    middleware as axum_middleware,
    routing::{get, post},
    Router,
};
use config::AppState;
use tower_http::{services::ServeDir, trace::TraceLayer};

/// Builds the complete Axum router with Basic Auth isolation on Admin routes,
/// public webhook & health endpoints, and static CSS serving.
pub fn build_router(state: AppState) -> Router {
    let admin_routes = Router::new()
        .route("/", get(routes::admin::dashboard))
        .route("/admin/agents", post(routes::admin::upsert_agent_handler))
        .route(
            "/admin/agents/:id/toggle",
            post(routes::admin::toggle_agent_handler),
        )
        .route(
            "/admin/agents/:id/delete",
            post(routes::admin::delete_agent_handler),
        )
        .route_layer(axum_middleware::from_fn_with_state(
            state.clone(),
            middleware::auth::basic_auth_middleware,
        ));

    let public_routes = Router::new()
        .route("/health", get(routes::admin::health_check))
        .route(
            "/webhook/evolution",
            post(routes::webhook::handle_evolution_webhook),
        )
        .nest_service("/static", ServeDir::new("static"));

    Router::new()
        .merge(admin_routes)
        .merge(public_routes)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
