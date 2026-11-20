use crate::{
    config::AppState,
    db,
    models::Agent,
    services::tools,
};
use serde_json::{json, Value};

const MAX_TOOL_ROUNDS: usize = 3;
const HISTORY_LIMIT: i64 = 10;
const GEMINI_MODEL: &str = "gemini-2.5-flash";

/// Full multi-tenant message processing pipeline:
/// 1. Persist user message in isolated `chat_history`
/// 2. Load last 10 messages for `(tenant_slug, customer_phone)`
/// 3. Invoke Gemini 2.5 Flash with Zero-Trust Layer 1 (tool filtering) & Layer 2 (runtime tool re-validation)
/// 4. Persist assistant reply in `chat_history`
/// 5. Send reply via Evolution API REST endpoint
pub async fn process_incoming_message(
    state: AppState,
    agent: Agent,
    customer_phone: String,
    message_text: String,
) {
    if let Err(err) = db::save_chat_message(
        &state.db,
        &agent.tenant_slug,
        &customer_phone,
        "user",
        &message_text,
    )
    .await
    {
        tracing::error!(
            tenant = %agent.tenant_slug,
            customer = %customer_phone,
            error = %err,
            "Failed to save incoming user message"
        );
        return;
    }

    let reply_text = match generate_agent_reply(&state, &agent, &customer_phone).await {
        Ok(text) if !text.trim().is_empty() => text,
        Ok(_) => "Desculpe, não consegui gerar uma resposta no momento. Pode reformular sua mensagem?".to_string(),
        Err(err) => {
            tracing::error!(
                tenant = %agent.tenant_slug,
                customer = %customer_phone,
                error = %err,
                "Agent engine failed to generate reply"
            );
            "Desculpe, estamos com uma instabilidade temporária. Tente novamente em instantes.".to_string()
        }
    };

    if let Err(err) = db::save_chat_message(
        &state.db,
        &agent.tenant_slug,
        &customer_phone,
        "assistant",
        &reply_text,
    )
    .await
    {
        tracing::error!(
            tenant = %agent.tenant_slug,
            customer = %customer_phone,
            error = %err,
            "Failed to save assistant reply in chat_history"
        );
    }

    if let Err(err) = send_evolution_message(
        &state,
        &agent.evolution_instance,
        &customer_phone,
        &reply_text,
    )
    .await
    {
        tracing::error!(
            tenant = %agent.tenant_slug,
            instance = %agent.evolution_instance,
            customer = %customer_phone,
            error = %err,
            "Failed to dispatch WhatsApp message via Evolution API"
        );
    }
}

/// Calls Gemini 2.5 Flash with 2-layer Zero-Trust tool governance.
pub async fn generate_agent_reply(
    state: &AppState,
    agent: &Agent,
    customer_phone: &str,
) -> Result<String, String> {
    let history = db::get_recent_history(&state.db, &agent.tenant_slug, customer_phone, HISTORY_LIMIT)
        .await
        .map_err(|e| format!("Database history lookup failed: {}", e))?;

    let mut contents: Vec<Value> = history
        .into_iter()
        .map(|msg| {
            let gemini_role = if msg.role == "assistant" {
                "model"
            } else {
                "user"
            };
            json!({
                "role": gemini_role,
                "parts": [{ "text": msg.content }]
            })
        })
        .collect();

    let allowed_tools = agent.parsed_allowed_tools();
    // Layer 1 Zero-Trust: only declare tools permitted for this tenant
    let tool_declarations = tools::build_gemini_tool_declarations(&allowed_tools);

    for _round in 0..MAX_TOOL_ROUNDS {
        let mut body = json!({
            "system_instruction": {
                "parts": [{ "text": agent.system_prompt }]
            },
            "contents": contents
        });

        if let Some(ref decls) = tool_declarations {
            body["tools"] = json!([{
                "functionDeclarations": decls
            }]);
        }

        let response_json = call_gemini_api(state, &body).await?;

        let parts = response_json
            .pointer("/candidates/0/content/parts")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        // Check if Gemini requested a Function Call
        let function_call = parts
            .iter()
            .find_map(|part| part.get("functionCall").cloned());

        if let Some(fc) = function_call {
            let tool_name = fc
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let tool_args = fc.get("args").cloned().unwrap_or_else(|| json!({}));

            // Layer 2 Zero-Trust: strictly re-validate before executing ANY tool
            if !tools::is_tool_allowed(&allowed_tools, &tool_name) {
                tracing::warn!(
                    tenant = %agent.tenant_slug,
                    customer = %customer_phone,
                    attempted_tool = %tool_name,
                    "ZERO-TRUST SECURITY BLOCK: Discarding unauthorized functionCall from LLM"
                );
                return Ok(
                    "Desculpe, não tenho permissão para executar esta ação no momento.".to_string(),
                );
            }

            let tool_output = match tools::execute_tool(
                &state.db,
                &agent.tenant_slug,
                customer_phone,
                &allowed_tools,
                &tool_name,
                &tool_args,
            )
            .await
            {
                Ok(val) => val,
                Err(err_msg) => json!({ "error": err_msg }),
            };

            // Append model functionCall turn and tool functionResponse turn for next Gemini round
            contents.push(json!({
                "role": "model",
                "parts": [{ "functionCall": fc }]
            }));
            contents.push(json!({
                "role": "function",
                "parts": [{
                    "functionResponse": {
                        "name": tool_name,
                        "response": tool_output
                    }
                }]
            }));

            continue;
        }

        // Otherwise extract text parts and return final reply
        let text_reply = parts
            .iter()
            .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<&str>>()
            .join("\n")
            .trim()
            .to_string();

        return Ok(text_reply);
    }

    Err("Maximum function calling rounds exceeded".to_string())
}

async fn call_gemini_api(state: &AppState, payload: &Value) -> Result<Value, String> {
    if state.config.gemini_api_key.trim().is_empty() {
        return Err("GEMINI_API_KEY is not configured".to_string());
    }

    let url = format!(
        "{}/models/{}:generateContent?key={}",
        state.config.gemini_api_base_url, GEMINI_MODEL, state.config.gemini_api_key
    );

    let response = state
        .http
        .post(&url)
        .json(payload)
        .send()
        .await
        .map_err(|e| format!("HTTP error calling Gemini API: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let err_body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Gemini API returned HTTP {}: {}",
            status.as_u16(),
            err_body
        ));
    }

    response
        .json::<Value>()
        .await
        .map_err(|e| format!("Failed to decode Gemini API JSON response: {}", e))
}

/// Sends a WhatsApp text message via Evolution API (`POST /message/sendText/{instance}`).
pub async fn send_evolution_message(
    state: &AppState,
    instance: &str,
    customer_phone: &str,
    text: &str,
) -> Result<(), String> {
    if state.config.evolution_api_url.trim().is_empty() {
        return Err("EVOLUTION_API_URL is not configured".to_string());
    }

    let url = format!(
        "{}/message/sendText/{}",
        state.config.evolution_api_url, instance
    );

    let body = json!({
        "number": customer_phone,
        "text": text
    });

    let response = state
        .http
        .post(&url)
        .header("apikey", &state.config.evolution_api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("HTTP error calling Evolution API: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let err_body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Evolution API returned HTTP {}: {}",
            status.as_u16(),
            err_body
        ));
    }

    tracing::info!(
        instance = %instance,
        customer = %customer_phone,
        "Dispatched reply to WhatsApp via Evolution API"
    );
    Ok(())
}
