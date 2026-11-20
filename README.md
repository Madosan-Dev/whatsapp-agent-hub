# WhatsApp Agent Hub — Multi-Tenant SaaS (Madosan Dev)

Plataforma SaaS multi-tenant em **Rust (Edition 2021)** para operação de agentes autônomos de WhatsApp em um único binário compilado de alta performance, com isolamento estrito por tenant em **SQLite**, motor de IA **Google Gemini (`gemini-2.5-flash`)** com Function Calling **Zero-Trust em 2 camadas**, integração nativa via **Evolution API** e painel administrativo Dark Mode servido diretamente pelo **Axum** (sem Node.js, React ou CDNs externos).

---

## Arquitetura do Sistema

- **Back-end:** Rust (`axum 0.7`, `tokio`, `sqlx` SQLite WAL, `reqwest`, `tower-http`, `tracing`).
- **Banco de Dados:** SQLite local (`sqlite:hub.db?mode=rwc`) criado e migrado automaticamente no boot com 3 tabelas isoladas:
  - `agents` — cadastro e permissões Zero-Trust por tenant/instância.
  - `chat_history` — memória isolada por `(tenant_slug, customer_phone)` (últimas 10 mensagens).
  - `paused_chats` — trava de transbordo humano (pausa automática por 2 horas).
- **Front-end Admin:** HTML5 + CSS3 puro em Dark Mode (`templates/dashboard.html`, `static/style.css`) sob autenticação **HTTP Basic Auth** com comparação em tempo constante.
- **Zero-Trust Function Calling (2 Camadas):**
  1. **Camada 1 (Declaração):** Somente as ferramentas explicitamente marcadas em `allowed_tools` do tenant são enviadas ao Gemini.
  2. **Camada 2 (Execução Runtime):** Mesmo sob tentativa de *Prompt Injection*, toda `functionCall` retornada pelo LLM é revalidada contra `allowed_tools` antes da execução. Chamadas não autorizadas são descartadas e logadas.

---

## Endpoints HTTP

| Método | Rota | Acesso | Descrição |
| :--- | :--- | :--- | :--- |
| `GET` | `/health` | Público | Health check do serviço (`200 OK`) |
| `POST` | `/webhook/evolution` | Público | Webhook da Evolution API (`messages.upsert`), filtra `fromMe`, grupos `@g.us` e chats pausados |
| `GET` | `/` | Basic Auth | Painel Administrativo Dark Mode multi-tenant |
| `POST` | `/admin/agents` | Basic Auth | Cadastro ou edição (upsert) de agente por `tenant_slug` |
| `POST` | `/admin/agents/:id/toggle` | Basic Auth | Ativa ou pausa um agente imediatamente |
| `POST` | `/admin/agents/:id/delete` | Basic Auth | Remove um agente cadastrado |
| `GET` | `/static/*` | Público | Assets estáticos CSS (zero dependências externas) |

---

## Execução Local

1. Copie o template de variáveis de ambiente:
   ```bash
   cp .env.example .env
   ```
2. Configure `ADMIN_USER`, `ADMIN_PASS`, `EVOLUTION_API_URL`, `EVOLUTION_API_KEY` e `GEMINI_API_KEY` no arquivo `.env`.
3. Compile e execute:
   ```bash
   cargo build --release
   ./target/release/whatsapp-agent-hub
   ```
4. Acesse o painel administrativo em `http://localhost:3000` utilizando as credenciais Basic Auth definidas no `.env`.

---

## Deploy com Docker

```bash
docker compose up -d --build
```

O banco SQLite é persistido automaticamente em `./data/hub.db` rodando sob usuário não-root (`appuser`) em imagem hardened `debian:bookworm-slim`.

---

## Suíte de Testes & QA

- **Testes de integração Rust (Zero-Trust, Isolamento Multi-Tenant, XSS, Basic Auth, Mock Gemini/Evolution):**
  ```bash
  cargo test
  ```
- **Suíte E2E PowerShell (sobe binário real e valida todos os endpoints HTTP):**
  ```powershell
  powershell -ExecutionPolicy Bypass -File .\test_suite.ps1
  ```
