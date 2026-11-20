use reqwest::Client;
use sqlx::SqlitePool;
use std::env;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub port: u16,
    pub database_url: String,
    pub admin_user: String,
    pub admin_pass: String,
    pub evolution_api_url: String,
    pub evolution_api_key: String,
    pub gemini_api_key: String,
    pub gemini_api_base_url: String,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let port = env::var("PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(3000);

        let database_url =
            env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:hub.db?mode=rwc".to_string());

        let admin_user = env::var("ADMIN_USER").unwrap_or_else(|_| "admin".to_string());
        let admin_pass = env::var("ADMIN_PASS").unwrap_or_else(|_| "changeme".to_string());

        let evolution_api_url = env::var("EVOLUTION_API_URL")
            .unwrap_or_else(|_| "http://localhost:8080".to_string())
            .trim_end_matches('/')
            .to_string();

        let evolution_api_key = env::var("EVOLUTION_API_KEY").unwrap_or_default();
        let gemini_api_key = env::var("GEMINI_API_KEY").unwrap_or_default();

        let gemini_api_base_url = env::var("GEMINI_API_BASE_URL")
            .unwrap_or_else(|_| "https://generativelanguage.googleapis.com/v1beta".to_string())
            .trim_end_matches('/')
            .to_string();

        Self {
            port,
            database_url,
            admin_user,
            admin_pass,
            evolution_api_url,
            evolution_api_key,
            gemini_api_key,
            gemini_api_base_url,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub http: Client,
    pub config: AppConfig,
}

impl AppState {
    pub fn new(db: SqlitePool, config: AppConfig) -> Result<Self, reqwest::Error> {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        Ok(Self { db, http, config })
    }
}
