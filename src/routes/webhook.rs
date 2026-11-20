use crate::{config::AppState, db, error::AppError, services::agent_engine};
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde_json::{json, Value};

/// Extracts incoming text from Evolution API message payload flexibly:
/// checks `data.message.conversation` first, then `data.message.extendedTextMessage.text`.
fn extract_message_text(payload: &Value) -> Option<String> {
    let text = payload
        .pointer("/data/message/conversation")
        .and_then(|v| v.as_str())
        .or_else(|| {
            payload
                .pointer("/data/message/extendedTextMessage/text")
                .and_then(|v| v.as_str())
        })?;

    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Evolution API webhook handler (`POST /webhook/evolution`).
/// Uses flexible `serde_json::Value` parsing, enforces multi-tenant isolation & human handoff lock,
/// and responds HTTP 200 immediately while delegating LLM processing asynchronously.
pub async fn handle_evolution_webhook(
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<impl IntoResponse, AppError> {
    let event = payload
        .get("event")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    if event != "messages.upsert" {
        return Ok((
            StatusCode::OK,
            Json(json!({ "status": "ignored", "reason": "event_not_messages_upsert" })),
        ));
    }

    let from_me = payload
        .pointer("/data/key/fromMe")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if from_me {
        return Ok((
            StatusCode::OK,
            Json(json!({ "status": "ignored", "reason": "from_me" })),
        ));
    }

    let remote_jid = payload
        .pointer("/data/key/remoteJid")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    if remote_jid.is_empty() || remote_jid.ends_with("@g.us") {
        return Ok((
            StatusCode::OK,
            Json(json!({ "status": "ignored", "reason": "group_or_invalid_jid" })),
        ));
    }

    let message_text = match extract_message_text(&payload) {
        Some(text) => text,
        None => {
            return Ok((
                StatusCode::OK,
                Json(json!({ "status": "ignored", "reason": "no_text_content" })),
            ));
        }
    };

    let instance = payload
        .get("instance")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .trim();

    if instance.is_empty() {
        return Ok((
            StatusCode::OK,
            Json(json!({ "status": "ignored", "reason": "missing_instance" })),
        ));
    }

    let agent = match db::find_active_agent_by_instance(&state.db, instance).await? {
        Some(agent) => agent,
        None => {
            tracing::debug!(instance = %instance, "No active agent found for Evolution instance");
            return Ok((
                StatusCode::OK,
                Json(json!({ "status": "ignored", "reason": "agent_not_found_or_inactive" })),
            ));
        }
    };

    let customer_phone = remote_jid
        .split('@')
        .next()
        .unwrap_or(remote_jid)
        .to_string();

    if db::is_chat_paused(&state.db, &agent.tenant_slug, &customer_phone).await? {
        tracing::info!(
            tenant = %agent.tenant_slug,
            customer = %customer_phone,
            "Chat is paused for human handoff — ignoring silently"
        );
        return Ok((
            StatusCode::OK,
            Json(json!({ "status": "ignored", "reason": "paused_for_human" })),
        ));
    }

    let tenant_slug = agent.tenant_slug.clone();
    let state_clone = state.clone();

    tokio::spawn(async move {
        agent_engine::process_incoming_message(state_clone, agent, customer_phone, message_text)
            .await;
    });

    Ok((
        StatusCode::OK,
        Json(json!({
            "status": "accepted",
            "tenant": tenant_slug
        })),
    ))
}
