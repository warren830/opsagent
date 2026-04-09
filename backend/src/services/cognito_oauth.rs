use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;

use crate::config::CognitoOAuthConfig;
use crate::error::{AppError, AppResult};

#[derive(Debug, Deserialize)]
pub struct CognitoTokenResponse {
    pub access_token: String,
    pub id_token: Option<String>,
    pub token_type: String,
    pub expires_in: u64,
}

#[derive(Debug, Deserialize)]
pub struct CognitoUserInfo {
    pub sub: String,
    pub email: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "given_name")]
    pub given_name: Option<String>,
    #[serde(rename = "family_name")]
    pub family_name: Option<String>,
}

impl CognitoUserInfo {
    pub fn display_name_or_email(&self) -> String {
        self.name
            .clone()
            .or_else(|| match (&self.given_name, &self.family_name) {
                (Some(g), Some(f)) => Some(format!("{} {}", g, f)),
                (Some(g), None) => Some(g.clone()),
                _ => None,
            })
            .or_else(|| self.email.clone())
            .unwrap_or_else(|| "Unknown".to_string())
    }
}

/// Build the Cognito base URL
fn base_url(config: &CognitoOAuthConfig) -> String {
    format!("https://{}.auth.{}.amazoncognito.com", config.domain, config.region)
}

/// Build the Cognito authorization URL
pub fn get_authorization_url(
    config: &CognitoOAuthConfig,
    redirect_uri: &str,
    state: &str,
    code_challenge: &str,
) -> String {
    format!(
        "{}/oauth2/authorize?\
         client_id={}&response_type=code&redirect_uri={}&scope={}&state={}\
         &code_challenge={}&code_challenge_method=S256",
        base_url(config),
        config.client_id,
        urlencoding::encode(redirect_uri),
        urlencoding::encode("openid profile email"),
        state,
        code_challenge,
    )
}

/// Exchange authorization code for tokens (Cognito uses Basic Auth header)
pub async fn exchange_code_for_token(
    config: &CognitoOAuthConfig,
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> AppResult<CognitoTokenResponse> {
    let client = reqwest::Client::new();
    let url = format!("{}/oauth2/token", base_url(config));

    // Cognito requires Basic Auth: base64(client_id:client_secret)
    let basic_auth = STANDARD.encode(format!("{}:{}", config.client_id, config.client_secret));

    let response = client
        .post(&url)
        .header("Authorization", format!("Basic {}", basic_auth))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("code_verifier", code_verifier),
        ])
        .send()
        .await
        .map_err(|e| AppError::HttpClient(e.to_string()))?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(AppError::OAuth(format!(
            "Cognito token exchange failed: {}",
            error_text
        )));
    }

    response
        .json::<CognitoTokenResponse>()
        .await
        .map_err(|e| AppError::HttpClient(e.to_string()))
}

/// Get user info from Cognito userInfo endpoint
pub async fn get_user_info(config: &CognitoOAuthConfig, access_token: &str) -> AppResult<CognitoUserInfo> {
    let client = reqwest::Client::new();
    let url = format!("{}/oauth2/userInfo", base_url(config));

    let response = client
        .get(&url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| AppError::HttpClient(e.to_string()))?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(AppError::OAuth(format!("Cognito user info failed: {}", error_text)));
    }

    response
        .json::<CognitoUserInfo>()
        .await
        .map_err(|e| AppError::HttpClient(e.to_string()))
}
