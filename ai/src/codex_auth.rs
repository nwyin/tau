//! Codex OAuth token management.
//!
//! Reads the access token from `~/.codex/auth.json` (written by `codex login`)
//! and refreshes it when expired. This lets tau use a ChatGPT subscription
//! instead of a pay-per-token API key for OpenAI models.

use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// auth.json schema (matches Codex CLI's persisted format)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexAuthFile {
    #[serde(default)]
    pub auth_mode: Option<String>,
    #[serde(default, rename = "OPENAI_API_KEY")]
    pub openai_api_key: Option<String>,
    #[serde(default)]
    pub tokens: Option<CodexTokens>,
    #[serde(default)]
    pub last_refresh: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexTokens {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub id_token: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
}

// ---------------------------------------------------------------------------
// JWT expiry extraction
// ---------------------------------------------------------------------------

/// Extract the `exp` claim from a JWT without full validation.
/// Returns the Unix timestamp, or None if parsing fails.
fn jwt_exp(token: &str) -> Option<i64> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    // Base64url-decode the payload (2nd segment)
    let payload = base64url_decode(parts[1])?;
    let claims: serde_json::Value = serde_json::from_slice(&payload).ok()?;
    claims.get("exp")?.as_i64()
}

fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    // Add padding if needed
    let padded = match input.len() % 4 {
        2 => format!("{}==", input),
        3 => format!("{}=", input),
        _ => input.to_string(),
    };
    // base64url uses - and _ instead of + and /
    let standard: String = padded
        .chars()
        .map(|c| match c {
            '-' => '+',
            '_' => '/',
            other => other,
        })
        .collect();
    // Use a simple decoder — we only need this for JWT payload
    base64_decode_simple(&standard)
}

/// Minimal base64 decoder (avoids adding the `base64` crate dependency).
fn base64_decode_simple(input: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    fn val(c: u8) -> Option<u8> {
        TABLE.iter().position(|&b| b == c).map(|p| p as u8)
    }

    let bytes: Vec<u8> = input.bytes().filter(|&b| b != b'=').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let vals: Vec<u8> = chunk.iter().filter_map(|&b| val(b)).collect();
        if vals.len() < 2 {
            return None;
        }
        out.push((vals[0] << 2) | (vals[1] >> 4));
        if vals.len() > 2 {
            out.push((vals[1] << 4) | (vals[2] >> 2));
        }
        if vals.len() > 3 {
            out.push((vals[2] << 6) | vals[3]);
        }
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Token refresh
// ---------------------------------------------------------------------------

/// Codex CLI's public OAuth client ID.
const CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const TOKEN_ENDPOINT: &str = "https://auth.openai.com/oauth/token";

/// Buffer before expiry to trigger refresh (5 minutes).
const REFRESH_BUFFER_SECS: i64 = 300;

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
}

async fn refresh_token(client: &reqwest::Client, refresh_token: &str) -> Result<TokenResponse> {
    let resp = client
        .post(TOKEN_ENDPOINT)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", CODEX_CLIENT_ID),
        ])
        .send()
        .await
        .context("failed to reach OpenAI token endpoint")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("token refresh failed (HTTP {}): {}", status, body));
    }

    resp.json::<TokenResponse>()
        .await
        .context("failed to parse token refresh response")
}

// ---------------------------------------------------------------------------
// CodexAuth — the public interface
// ---------------------------------------------------------------------------

/// Base URL for the ChatGPT backend (Codex OAuth).
/// ChatGPT OAuth tokens cannot hit api.openai.com/v1 — they use a separate
/// endpoint on chatgpt.com that accepts the same Responses API format.
pub const CHATGPT_BACKEND_URL: &str = "https://chatgpt.com/backend-api/codex";

/// Manages Codex OAuth tokens with automatic refresh.
pub struct CodexAuth {
    auth_file_path: PathBuf,
    client: reqwest::Client,
    state: Mutex<CodexAuthFile>,
}

impl CodexAuth {
    /// Load from the default path (`~/.codex/auth.json`).
    pub fn load() -> Result<Self> {
        let home = std::env::var("HOME").context("HOME not set")?;
        let path = PathBuf::from(home).join(".codex").join("auth.json");
        Self::load_from(path)
    }

    /// Load from a specific path.
    pub fn load_from(path: PathBuf) -> Result<Self> {
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let auth: CodexAuthFile = serde_json::from_str(&contents)
            .with_context(|| format!("failed to parse {}", path.display()))?;

        // Validate we have tokens
        if auth.tokens.is_none() {
            return Err(anyhow!(
                "no tokens in {}. Run `codex login` first.",
                path.display()
            ));
        }

        Ok(Self {
            auth_file_path: path,
            client: reqwest::Client::new(),
            state: Mutex::new(auth),
        })
    }

    /// Get a valid access token, refreshing if expired.
    pub async fn access_token(&self) -> Result<String> {
        let mut state = self.state.lock().await;
        let tokens = state
            .tokens
            .as_ref()
            .ok_or_else(|| anyhow!("no tokens available"))?;

        let now = chrono::Utc::now().timestamp();

        // Check if token is expired (or will expire within the buffer)
        let needs_refresh = match jwt_exp(&tokens.access_token) {
            Some(exp) => now >= (exp - REFRESH_BUFFER_SECS),
            None => false, // Can't determine expiry — try using it
        };

        if !needs_refresh {
            return Ok(tokens.access_token.clone());
        }

        // Refresh
        let rt = tokens
            .refresh_token
            .as_deref()
            .ok_or_else(|| anyhow!("token expired but no refresh_token available"))?;

        let response = refresh_token(&self.client, rt).await?;

        // Update in-memory state
        let new_tokens = CodexTokens {
            access_token: response.access_token.clone(),
            refresh_token: response
                .refresh_token
                .or_else(|| tokens.refresh_token.clone()),
            id_token: response.id_token.or_else(|| tokens.id_token.clone()),
            account_id: tokens.account_id.clone(),
        };
        state.tokens = Some(new_tokens);
        state.last_refresh = Some(chrono::Utc::now().to_rfc3339());

        // Persist updated tokens back to disk
        if let Ok(json) = serde_json::to_string_pretty(&*state) {
            let _ = std::fs::write(&self.auth_file_path, json);
        }

        Ok(response.access_token)
    }

    /// Get the account ID (needed as `ChatGPT-Account-ID` header).
    pub async fn account_id(&self) -> Option<String> {
        let state = self.state.lock().await;
        state.tokens.as_ref().and_then(|t| t.account_id.clone())
    }

    /// Check if a Codex auth file exists at the default path.
    pub fn is_available() -> bool {
        let home = match std::env::var("HOME") {
            Ok(h) => h,
            Err(_) => return false,
        };
        PathBuf::from(home)
            .join(".codex")
            .join("auth.json")
            .exists()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_auth_file() {
        let json = r#"{
            "auth_mode": "chatgpt",
            "OPENAI_API_KEY": null,
            "tokens": {
                "id_token": "eyJ.test.id",
                "access_token": "eyJ.test.access",
                "refresh_token": "rt_test123",
                "account_id": "org-test"
            },
            "last_refresh": "2026-03-13T19:24:14.586119Z"
        }"#;
        let auth: CodexAuthFile = serde_json::from_str(json).unwrap();
        assert_eq!(auth.auth_mode.as_deref(), Some("chatgpt"));
        let tokens = auth.tokens.unwrap();
        assert_eq!(tokens.access_token, "eyJ.test.access");
        assert_eq!(tokens.refresh_token.as_deref(), Some("rt_test123"));
        assert_eq!(tokens.account_id.as_deref(), Some("org-test"));
    }

    #[test]
    fn parse_minimal_auth_file() {
        let json = r#"{"tokens": {"access_token": "tok"}}"#;
        let auth: CodexAuthFile = serde_json::from_str(json).unwrap();
        let tokens = auth.tokens.unwrap();
        assert_eq!(tokens.access_token, "tok");
        assert!(tokens.refresh_token.is_none());
    }

    #[test]
    fn jwt_exp_extracts_expiry() {
        // Build a minimal JWT: header.payload.signature
        // Payload: {"exp": 1700000000, "sub": "test"}
        let payload = r#"{"exp":1700000000,"sub":"test"}"#;
        let encoded = base64url_encode(payload.as_bytes());
        let token = format!("eyJhbGciOiJSUzI1NiJ9.{}.fake_sig", encoded);
        assert_eq!(jwt_exp(&token), Some(1700000000));
    }

    #[test]
    fn jwt_exp_returns_none_for_invalid() {
        assert_eq!(jwt_exp("not-a-jwt"), None);
        assert_eq!(jwt_exp("a.b.c"), None); // b won't decode to valid JSON
    }

    #[test]
    fn base64url_roundtrip() {
        let input = b"hello world";
        let encoded = base64url_encode(input);
        let decoded = base64url_decode(&encoded).unwrap();
        assert_eq!(decoded, input);
    }

    fn base64url_encode(input: &[u8]) -> String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut out = String::new();
        for chunk in input.chunks(3) {
            let b0 = chunk[0] as usize;
            let b1 = if chunk.len() > 1 {
                chunk[1] as usize
            } else {
                0
            };
            let b2 = if chunk.len() > 2 {
                chunk[2] as usize
            } else {
                0
            };
            out.push(TABLE[b0 >> 2] as char);
            out.push(TABLE[((b0 & 3) << 4) | (b1 >> 4)] as char);
            if chunk.len() > 1 {
                out.push(TABLE[((b1 & 0xf) << 2) | (b2 >> 6)] as char);
            }
            if chunk.len() > 2 {
                out.push(TABLE[b2 & 0x3f] as char);
            }
        }
        out
    }
}
