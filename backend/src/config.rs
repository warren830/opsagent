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
            "dev" | "development" | "non-prod" | "staging" => Self::Dev,
            "local" | "" => Self::Local,
            // Any unrecognized non-empty value is treated as non-local (cloud)
            _ => Self::Dev,
        }
    }

    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }

    pub fn is_prod(&self) -> bool {
        matches!(self, Self::Prod)
    }
}

/// Microsoft Entra ID (Azure AD) OAuth configuration
#[derive(Debug, Clone)]
pub struct MicrosoftOAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub tenant_id: String,
    pub redirect_uris: Vec<String>,
}

/// AWS Cognito OAuth configuration
#[derive(Debug, Clone)]
pub struct CognitoOAuthConfig {
    pub user_pool_id: String,
    pub client_id: String,
    pub client_secret: String,
    pub region: String,
    pub domain: String,
    pub redirect_uris: Vec<String>,
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
    pub jwt_access_token_expire_minutes: u64,
    pub jwt_refresh_token_expire_days: u64,
    pub allowed_origins: Vec<String>,
    pub claude_bin: String,
    pub claude_timeout_ms: u64,
    pub claude_model: String,
    pub claude_work_dir: String,
    pub aws_region: String,
    // OAuth providers (None = not configured)
    pub microsoft_oauth: Option<MicrosoftOAuthConfig>,
    pub cognito_oauth: Option<CognitoOAuthConfig>,
}

impl AppConfig {
    /// Load configuration from environment variables.
    /// Panics on missing required values in production.
    pub fn from_env() -> Self {
        let env = Environment::from_str(&env::var("OPS_ENV").unwrap_or_else(|_| "local".to_string()));

        let jwt_secret = env::var("JWT_SECRET").unwrap_or_else(|_| {
            if env.is_prod() {
                panic!("JWT_SECRET must be set in production");
            }
            "dev-secret-minimum-32-characters-long-change-in-prod".to_string()
        });

        let database_url = env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://ops:ops_dev@localhost:5432/ops".to_string());

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

        // Microsoft OAuth (optional)
        let microsoft_oauth = Self::load_microsoft_oauth();

        // Cognito OAuth (optional)
        let cognito_oauth = Self::load_cognito_oauth();

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
            jwt_access_token_expire_minutes: env::var("JWT_ACCESS_TOKEN_EXPIRE_MINUTES")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .unwrap_or(30),
            jwt_refresh_token_expire_days: env::var("JWT_REFRESH_TOKEN_EXPIRE_DAYS")
                .unwrap_or_else(|_| "7".to_string())
                .parse()
                .unwrap_or(7),
            allowed_origins,
            claude_bin: env::var("CLAUDE_BIN").unwrap_or_else(|_| "claude".to_string()),
            claude_timeout_ms: env::var("CLAUDE_TIMEOUT_MS")
                .unwrap_or_else(|_| "300000".to_string())
                .parse()
                .unwrap_or(300_000),
            claude_model: env::var("CLAUDE_MODEL").unwrap_or_else(|_| "opus".to_string()),
            claude_work_dir: env::var("CLAUDE_WORK_DIR").unwrap_or_else(|_| "./workspace".to_string()),
            aws_region: env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
            microsoft_oauth,
            cognito_oauth,
        }
    }

    /// Load Microsoft Entra ID OAuth config from env vars (returns None if not configured)
    fn load_microsoft_oauth() -> Option<MicrosoftOAuthConfig> {
        let client_id = env::var("MICROSOFT_CLIENT_ID").ok()?;
        let client_secret = env::var("MICROSOFT_CLIENT_SECRET").ok()?;
        let tenant_id = env::var("MICROSOFT_TENANT_ID").ok()?;
        let redirect_uris: Vec<String> = env::var("MICROSOFT_REDIRECT_URIS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Some(MicrosoftOAuthConfig {
            client_id,
            client_secret,
            tenant_id,
            redirect_uris,
        })
    }

    /// Load AWS Cognito OAuth config from env vars (returns None if not configured)
    fn load_cognito_oauth() -> Option<CognitoOAuthConfig> {
        let user_pool_id = env::var("COGNITO_USER_POOL_ID").ok()?;
        let client_id = env::var("COGNITO_CLIENT_ID").ok()?;
        let client_secret = env::var("COGNITO_CLIENT_SECRET").ok()?;
        let region = env::var("COGNITO_REGION").ok()?;
        let domain = env::var("COGNITO_DOMAIN").ok()?;
        let redirect_uris: Vec<String> = env::var("COGNITO_REDIRECT_URIS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Some(CognitoOAuthConfig {
            user_pool_id,
            client_id,
            client_secret,
            region,
            domain,
            redirect_uris,
        })
    }

    /// Check if Microsoft OAuth is configured
    pub fn microsoft_is_configured(&self) -> bool {
        self.microsoft_oauth.is_some()
    }

    /// Check if Cognito OAuth is configured
    pub fn cognito_is_configured(&self) -> bool {
        self.cognito_oauth.is_some()
    }

    /// Get Cognito base URL (e.g. https://domain.auth.us-east-1.amazoncognito.com)
    pub fn cognito_base_url(&self) -> Option<String> {
        self.cognito_oauth
            .as_ref()
            .map(|c| format!("https://{}.auth.{}.amazoncognito.com", c.domain, c.region))
    }
}
