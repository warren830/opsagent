use std::env;

/// Application environment
#[derive(Debug, Clone, PartialEq)]
pub enum Environment {
    Local,
    Dev,
    Prod,
}

impl Environment {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "prod" | "production" => Self::Prod,
            "dev" | "development" => Self::Dev,
            _ => Self::Local,
        }
    }

    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }

    pub fn is_prod(&self) -> bool {
        matches!(self, Self::Prod)
    }
}

/// Application configuration loaded from environment variables (12-factor)
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub env: Environment,
    pub backend_port: u16,
    pub database_url: String,
    pub db_max_connections: u32,
    pub db_min_connections: u32,
    pub jwt_secret: String,
    pub allowed_origins: Vec<String>,
    pub claude_bin: String,
    pub claude_timeout_ms: u64,
    pub claude_model: String,
    pub claude_work_dir: String,
    pub aws_region: String,
    pub redis_url: Option<String>,
}

impl AppConfig {
    /// Load configuration from environment variables.
    /// Panics on missing required values in production.
    pub fn from_env() -> Self {
        let env = Environment::from_str(
            &env::var("OPENOPS_ENV").unwrap_or_else(|_| "local".to_string()),
        );

        let jwt_secret = env::var("JWT_SECRET").unwrap_or_else(|_| {
            if env.is_prod() {
                panic!("JWT_SECRET must be set in production");
            }
            "dev-secret-minimum-32-characters-long-change-in-prod".to_string()
        });

        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://openops:openops_dev@localhost:5432/openops".to_string()
        });

        let allowed_origins: Vec<String> = env::var("ALLOWED_ORIGINS")
            .unwrap_or_else(|_| "http://localhost:3000".to_string())
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        // Validate: wildcard only in local mode
        if !env.is_local() && allowed_origins.iter().any(|o| o == "*") {
            panic!("Wildcard CORS origin (*) is only allowed in local mode");
        }

        Self {
            env,
            backend_port: env::var("BACKEND_PORT")
                .unwrap_or_else(|_| "3080".to_string())
                .parse()
                .expect("BACKEND_PORT must be a valid port number"),
            database_url,
            db_max_connections: env::var("DB_MAX_CONNECTIONS")
                .unwrap_or_else(|_| "20".to_string())
                .parse()
                .unwrap_or(20),
            db_min_connections: env::var("DB_MIN_CONNECTIONS")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .unwrap_or(5),
            jwt_secret,
            allowed_origins,
            claude_bin: env::var("CLAUDE_BIN")
                .unwrap_or_else(|_| "claude".to_string()),
            claude_timeout_ms: env::var("CLAUDE_TIMEOUT_MS")
                .unwrap_or_else(|_| "300000".to_string())
                .parse()
                .unwrap_or(300_000),
            claude_model: env::var("CLAUDE_MODEL")
                .unwrap_or_else(|_| "opus".to_string()),
            claude_work_dir: env::var("CLAUDE_WORK_DIR")
                .unwrap_or_else(|_| "./workspace".to_string()),
            aws_region: env::var("AWS_REGION")
                .unwrap_or_else(|_| "us-east-1".to_string()),
            redis_url: env::var("REDIS_URL").ok(),
        }
    }
}
