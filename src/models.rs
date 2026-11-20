use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Agent {
    pub id: i64,
    pub tenant_slug: String,
    pub evolution_instance: String,
    pub system_prompt: String,
    pub allowed_tools: String,
    pub active: bool,
    pub created_at: String,
}

impl Agent {
    /// Parses the JSON array of allowed tool names safely without panicking.
    pub fn parsed_allowed_tools(&self) -> Vec<String> {
        serde_json::from_str::<Vec<String>>(&self.allowed_tools).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChatMessage {
    pub id: i64,
    pub tenant_slug: String,
    pub customer_phone: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateAgentForm {
    pub tenant_slug: String,
    pub evolution_instance: String,
    pub system_prompt: String,
    pub tool_consultar_agenda: Option<String>,
    pub tool_criar_agendamento: Option<String>,
    pub tool_transferir_para_humano: Option<String>,
}

impl CreateAgentForm {
    /// Builds a strict allow-list of enabled tools from submitted checkboxes.
    pub fn extract_allowed_tools(&self) -> Vec<String> {
        let mut tools = Vec::new();
        if self.tool_consultar_agenda.is_some() {
            tools.push("consultar_agenda".to_string());
        }
        if self.tool_criar_agendamento.is_some() {
            tools.push("criar_agendamento".to_string());
        }
        if self.tool_transferir_para_humano.is_some() {
            tools.push("transferir_para_humano".to_string());
        }
        tools
    }
}
