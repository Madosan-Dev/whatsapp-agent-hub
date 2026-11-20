# Changelog

Todas as mudanças relevantes do **WhatsApp Agent Hub** (Madosan Dev) são documentadas neste arquivo seguindo [SemVer](https://semver.org/).

## [1.0.0] - 2026-11-20

### Added
- **Multi-Tenant Core (Rust / Axum / SQLite):** binário único compilado com auto-migração SQLite WAL (`agents`, `chat_history`, `paused_chats`).
- **Webhook Evolution API (`POST /webhook/evolution`):** parsing flexível `serde_json::Value`, filtros contra `fromMe`, grupos `@g.us` e trava de transbordo humano (`paused_chats`), com resposta assíncrona imediata HTTP `200 OK`.
- **Motor LLM Zero-Trust (`gemini-2.5-flash`):** validação em 2 camadas (Camada 1 na declaração de tools; Camada 2 em runtime contra Prompt Injection) e catálogo de ferramentas (`consultar_agenda`, `criar_agendamento`, `transferir_para_humano`).
- **Painel Admin Dark Mode:** HTML5 + CSS3 puro sem dependências externas, protegido por HTTP Basic Auth com comparação em tempo constante e proteção contra Stored XSS.
- **Empacotamento & QA:** Dockerfile multi-stage hardened (`appuser`), `docker-compose.yml`, `.env.example`, 7 testes de integração Rust e suíte E2E `test_suite.ps1` (17 verificações).

### Fixed
- Corrigido o caminho relativo da folha de estilo em `templates/dashboard.html` (`../static/style.css`).
- Adicionado fallback embutido para `/static/style.css` diretamente no binário release, garantindo renderização do CSS independentemente do diretório de execução.
