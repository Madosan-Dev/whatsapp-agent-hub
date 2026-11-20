use crate::{
    config::AppState,
    db,
    error::AppError,
    models::{Agent, CreateAgentForm},
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    Form, Json,
};
use serde_json::json;

const DASHBOARD_TEMPLATE: &str = include_str!("../../templates/dashboard.html");

/// Escapes HTML special characters to prevent Stored XSS in rendered cards.
fn escape_html(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#x27;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn render_agent_cards(agents: &[Agent]) -> String {
    if agents.is_empty() {
        return r#"
        <div class="empty-state">
          <div class="empty-state-icon">🤖</div>
          <p><strong>Nenhum agente cadastrado ainda.</strong></p>
          <p>Utilize o formulário ao lado para criar o primeiro tenant multi-instância.</p>
        </div>
        "#
        .to_string();
    }

    let mut html = String::new();
    for agent in agents {
        let slug = escape_html(&agent.tenant_slug);
        let instance = escape_html(&agent.evolution_instance);
        let prompt = escape_html(&agent.system_prompt);
        let created = escape_html(&agent.created_at);

        let (status_badge, toggle_btn_class, toggle_label) = if agent.active {
            (
                r#"<span class="badge badge-active">● Ativo</span>"#,
                "btn btn-sm btn-toggle-active",
                "Pausar",
            )
        } else {
            (
                r#"<span class="badge badge-paused">● Pausado</span>"#,
                "btn btn-sm btn-toggle-paused",
                "Ativar",
            )
        };

        let tools = agent.parsed_allowed_tools();
        let tools_html = if tools.is_empty() {
            r#"<span class="badge badge-paused">Sem ferramentas habilitadas</span>"#.to_string()
        } else {
            tools
                .iter()
                .map(|t| format!(r#"<span class="badge badge-tool">{}</span>"#, escape_html(t)))
                .collect::<Vec<String>>()
                .join("\n")
        };

        html.push_str(&format!(
            r#"
            <article class="agent-card">
              <div class="agent-card-header">
                <div class="agent-title-group">
                  <span class="agent-name">{slug}</span>
                  <span class="agent-slug">Instância: {instance}</span>
                </div>
                <div class="agent-actions">
                  {status_badge}
                  <form method="POST" action="/admin/agents/{id}/toggle">
                    <button type="submit" class="{toggle_btn_class}">{toggle_label}</button>
                  </form>
                  <form method="POST" action="/admin/agents/{id}/delete">
                    <button type="submit" class="btn btn-sm btn-delete" title="Remover agente">Excluir</button>
                  </form>
                </div>
              </div>

              <div class="agent-tools-container">
                {tools_html}
              </div>

              <div class="agent-prompt-preview">{prompt}</div>

              <div class="agent-footer">
                <span>ID #{id}</span>
                <span>Criado em: {created}</span>
              </div>
            </article>
            "#,
            id = agent.id,
            slug = slug,
            instance = instance,
            status_badge = status_badge,
            toggle_btn_class = toggle_btn_class,
            toggle_label = toggle_label,
            tools_html = tools_html,
            prompt = prompt,
            created = created,
        ));
    }

    html
}

/// Public health check endpoint (`GET /health`).
pub async fn health_check() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({
            "status": "ok",
            "service": "whatsapp-agent-hub"
        })),
    )
}

/// Renders the Dark Mode Multi-Tenant Admin Dashboard (`GET /`).
pub async fn dashboard(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    let agents = db::list_agents(&state.db).await?;
    let cards_html = render_agent_cards(&agents);

    let rendered = DASHBOARD_TEMPLATE
        .replace("{{AGENT_COUNT}}", &agents.len().to_string())
        .replace("{{AGENTS_CARDS}}", &cards_html);

    Ok(Html(rendered))
}

/// Creates or updates a tenant agent (`POST /admin/agents`).
pub async fn upsert_agent_handler(
    State(state): State<AppState>,
    Form(form): Form<CreateAgentForm>,
) -> Result<Redirect, AppError> {
    let tenant_slug = form.tenant_slug.trim();
    let evolution_instance = form.evolution_instance.trim();
    let system_prompt = form.system_prompt.trim();

    if tenant_slug.is_empty()
        || !tenant_slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::BadRequest(
            "tenant_slug inválido: utilize apenas letras, números, hífens ou underlines."
                .to_string(),
        ));
    }

    if evolution_instance.is_empty() {
        return Err(AppError::BadRequest(
            "evolution_instance é obrigatório.".to_string(),
        ));
    }

    if system_prompt.is_empty() {
        return Err(AppError::BadRequest(
            "system_prompt é obrigatório.".to_string(),
        ));
    }

    let allowed_tools = form.extract_allowed_tools();
    let allowed_tools_json = serde_json::to_string(&allowed_tools)
        .map_err(|e| AppError::Internal(format!("Failed to serialize allowed_tools: {}", e)))?;

    db::upsert_agent(
        &state.db,
        tenant_slug,
        evolution_instance,
        system_prompt,
        &allowed_tools_json,
    )
    .await?;

    tracing::info!(
        tenant = %tenant_slug,
        instance = %evolution_instance,
        tools = %allowed_tools_json,
        "Saved tenant agent via admin panel"
    );

    Ok(Redirect::to("/"))
}

/// Toggles active/paused state of an agent (`POST /admin/agents/:id/toggle`).
pub async fn toggle_agent_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Redirect, AppError> {
    let affected = db::toggle_agent_status(&state.db, id).await?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("Agente #{} não encontrado.", id)));
    }

    tracing::info!(agent_id = id, "Toggled agent active status");
    Ok(Redirect::to("/"))
}

/// Deletes an agent by ID (`POST /admin/agents/:id/delete`).
pub async fn delete_agent_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Redirect, AppError> {
    let affected = db::delete_agent(&state.db, id).await?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("Agente #{} não encontrado.", id)));
    }

    tracing::info!(agent_id = id, "Deleted tenant agent");
    Ok(Redirect::to("/"))
}
