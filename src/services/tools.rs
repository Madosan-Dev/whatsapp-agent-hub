use crate::db;
use serde_json::{json, Value};
use sqlx::SqlitePool;

/// Zero-Trust Layer 2 check: validates that `tool_name` is explicitly permitted for the tenant.
pub fn is_tool_allowed(allowed_tools: &[String], tool_name: &str) -> bool {
    allowed_tools.iter().any(|t| t == tool_name)
}

/// Zero-Trust Layer 1: builds Gemini `functionDeclarations` strictly limited to `allowed_tools`.
pub fn build_gemini_tool_declarations(allowed_tools: &[String]) -> Option<Vec<Value>> {
    let mut declarations = Vec::new();

    for tool in allowed_tools {
        match tool.as_str() {
            "consultar_agenda" => {
                declarations.push(json!({
                    "name": "consultar_agenda",
                    "description": "Consulta os horários disponíveis na agenda para atendimento.",
                    "parameters": {
                        "type": "OBJECT",
                        "properties": {
                            "data": {
                                "type": "STRING",
                                "description": "Data desejada no formato YYYY-MM-DD ou termo como 'hoje' ou 'amanhã'."
                            }
                        }
                    }
                }));
            }
            "criar_agendamento" => {
                declarations.push(json!({
                    "name": "criar_agendamento",
                    "description": "Confirma e registra um novo agendamento para o cliente.",
                    "parameters": {
                        "type": "OBJECT",
                        "properties": {
                            "data": {
                                "type": "STRING",
                                "description": "Data e horário do agendamento (ex: 2026-11-21 14:00)."
                            },
                            "servico": {
                                "type": "STRING",
                                "description": "Descrição do serviço solicitado pelo cliente."
                            },
                            "nome_cliente": {
                                "type": "STRING",
                                "description": "Nome do cliente, se informado."
                            }
                        },
                        "required": ["data", "servico"]
                    }
                }));
            }
            "transferir_para_humano" => {
                declarations.push(json!({
                    "name": "transferir_para_humano",
                    "description": "Transfere o atendimento para um especialista humano e pausa as respostas automáticas do robô por 2 horas.",
                    "parameters": {
                        "type": "OBJECT",
                        "properties": {
                            "motivo": {
                                "type": "STRING",
                                "description": "Resumo do motivo da transferência para o atendente humano."
                            }
                        }
                    }
                }));
            }
            unknown => {
                tracing::warn!(tool = %unknown, "Ignoring unrecognized tool in allowed_tools");
            }
        }
    }

    if declarations.is_empty() {
        None
    } else {
        Some(declarations)
    }
}

/// Executes a tool using a clean `match tool_name.as_str()` with Zero-Trust Layer 2 validation.
pub async fn execute_tool(
    pool: &SqlitePool,
    tenant_slug: &str,
    customer_phone: &str,
    allowed_tools: &[String],
    tool_name: &str,
    args: &Value,
) -> Result<Value, String> {
    if !is_tool_allowed(allowed_tools, tool_name) {
        tracing::warn!(
            tenant = %tenant_slug,
            customer = %customer_phone,
            tool = %tool_name,
            "ZERO-TRUST LAYER 2 VIOLATION: Blocked unauthorized tool call"
        );
        return Err(format!(
            "Acesso negado: a ferramenta '{}' não está autorizada para este agente.",
            tool_name
        ));
    }

    match tool_name {
        "consultar_agenda" => {
            let data = args
                .get("data")
                .and_then(|v| v.as_str())
                .unwrap_or("próximos dias úteis");

            tracing::info!(
                tenant = %tenant_slug,
                customer = %customer_phone,
                date = %data,
                "Executing tool consultar_agenda"
            );

            Ok(json!({
                "status": "ok",
                "data_consultada": data,
                "horarios_disponiveis": ["09:00", "11:00", "14:00", "16:30"],
                "observacao": "Horários sujeitos a confirmação imediata."
            }))
        }
        "criar_agendamento" => {
            let data = args
                .get("data")
                .and_then(|v| v.as_str())
                .unwrap_or("horário comercial a confirmar");
            let servico = args
                .get("servico")
                .and_then(|v| v.as_str())
                .unwrap_or("Atendimento geral");
            let nome = args
                .get("nome_cliente")
                .and_then(|v| v.as_str())
                .unwrap_or("Cliente");

            tracing::info!(
                tenant = %tenant_slug,
                customer = %customer_phone,
                date = %data,
                service = %servico,
                "Executing tool criar_agendamento"
            );

            Ok(json!({
                "status": "confirmado",
                "tenant": tenant_slug,
                "cliente": nome,
                "telefone": customer_phone,
                "data": data,
                "servico": servico,
                "mensagem": format!(
                    "Agendamento confirmado para {} — Serviço: {} (Cliente: {}).",
                    data, servico, nome
                )
            }))
        }
        "transferir_para_humano" => {
            let motivo = args
                .get("motivo")
                .and_then(|v| v.as_str())
                .unwrap_or("Solicitação de atendimento humano");

            if let Err(err) = db::pause_chat_for_two_hours(pool, tenant_slug, customer_phone).await
            {
                tracing::error!(
                    tenant = %tenant_slug,
                    customer = %customer_phone,
                    error = %err,
                    "Failed to record paused chat in SQLite"
                );
                return Err("Falha ao registrar pausa para transbordo humano.".to_string());
            }

            tracing::info!(
                tenant = %tenant_slug,
                customer = %customer_phone,
                reason = %motivo,
                "Chat paused for 2 hours due to human handoff"
            );

            Ok(json!({
                "status": "transferido",
                "pausado_por_horas": 2,
                "motivo": motivo,
                "mensagem": "Atendimento transferido com sucesso. Um atendente humano assumirá esta conversa em breve."
            }))
        }
        other => {
            tracing::warn!(
                tenant = %tenant_slug,
                tool = %other,
                "Unknown tool requested"
            );
            Err(format!("Ferramenta desconhecida: '{}'.", other))
        }
    }
}
