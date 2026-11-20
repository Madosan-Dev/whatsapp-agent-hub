use crate::config::AppState;
use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use base64::{engine::general_purpose::STANDARD, Engine as _};

/// Constant-time byte comparison to mitigate timing attacks on credentials.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn unauthorized_response() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(
            header::WWW_AUTHENTICATE,
            "Basic realm=\"WhatsApp Agent Hub Admin\"",
        )],
        "Autenticação obrigatória para acessar o painel administrativo.",
    )
        .into_response()
}

/// Axum middleware enforcing HTTP Basic Auth (`ADMIN_USER` / `ADMIN_PASS`)
/// on `/` and `/admin/*` routes.
pub async fn basic_auth_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let auth_header = match request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        Some(value) => value,
        None => return unauthorized_response(),
    };

    let encoded = match auth_header.strip_prefix("Basic ") {
        Some(token) => token.trim(),
        None => return unauthorized_response(),
    };

    let decoded_bytes = match STANDARD.decode(encoded) {
        Ok(bytes) => bytes,
        Err(_) => return unauthorized_response(),
    };

    let decoded = match String::from_utf8(decoded_bytes) {
        Ok(text) => text,
        Err(_) => return unauthorized_response(),
    };

    let (user, pass) = match decoded.split_once(':') {
        Some(pair) => pair,
        None => return unauthorized_response(),
    };

    let user_ok = constant_time_eq(user.as_bytes(), state.config.admin_user.as_bytes());
    let pass_ok = constant_time_eq(pass.as_bytes(), state.config.admin_pass.as_bytes());

    if user_ok && pass_ok {
        next.run(request).await
    } else {
        tracing::warn!(attempted_user = %user, "Rejected invalid Basic Auth login attempt");
        unauthorized_response()
    }
}
