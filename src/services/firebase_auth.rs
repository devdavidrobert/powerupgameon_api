use crate::config::Config;
use crate::services::firestore::FirestoreService;
use anyhow::{bail, Context, Result};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use tokio::sync::Mutex;

static HTTP: Lazy<Client> = Lazy::new(Client::new);
static JWKS_CACHE: Lazy<RwLock<HashMap<String, DecodingKey>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

#[derive(Clone)]
pub struct FirebaseAuth {
    project_id: String,
    service_account: ServiceAccount,
    access_token: Arc<Mutex<Option<(String, chrono::DateTime<chrono::Utc>)>>>,
}

#[derive(Clone, Deserialize)]
struct ServiceAccount {
    project_id: String,
    client_email: String,
    private_key: String,
    token_uri: String,
}

#[derive(Debug, Deserialize)]
struct FirebaseClaims {
    sub: String,
    email: Option<String>,
    email_verified: Option<bool>,
    admin: Option<bool>,
}

#[derive(Deserialize)]
struct GoogleTokenResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Deserialize)]
struct JwksResponse {
    keys: Vec<JwkKey>,
}

#[derive(Deserialize)]
struct JwkKey {
    kid: String,
    n: String,
    e: String,
}

impl FirebaseAuth {
    pub async fn new(config: &Config, _db: &FirestoreService) -> Result<Self> {
        let json = config
            .firebase_service_account_json
            .as_ref()
            .context("FIREBASE_SERVICE_ACCOUNT_JSON is required")?;
        let service_account: ServiceAccount =
            serde_json::from_str(json).context("Invalid service account JSON")?;
        let project_id = config.project_id(Some(&service_account.project_id))?;

        Ok(Self {
            project_id,
            service_account,
            access_token: Arc::new(Mutex::new(None)),
        })
    }

    pub async fn verify_id_token(&self, id_token: &str) -> Result<VerifiedUser> {
        let header = decode_header(id_token).context("Invalid token header")?;
        let kid = header.kid.context("Token missing kid")?;

        let key = self.get_decoding_key(&kid).await?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&[self.project_id.as_str()]);
        validation.set_issuer(&[format!(
            "https://securetoken.google.com/{}",
            self.project_id
        )]);

        let token = decode::<FirebaseClaims>(id_token, &key, &validation)
            .context("Invalid or expired token")?;

        Ok(VerifiedUser {
            uid: token.claims.sub,
            email: token.claims.email,
            admin: token.claims.admin.unwrap_or(false),
            email_verified: token.claims.email_verified.unwrap_or(false),
        })
    }

    async fn get_decoding_key(&self, kid: &str) -> Result<DecodingKey> {
        if let Some(key) = JWKS_CACHE.read().unwrap().get(kid).cloned() {
            return Ok(key);
        }

        let url = "https://www.googleapis.com/service_accounts/v1/jwk/securetoken@system.gserviceaccount.com";
        let resp: JwksResponse = HTTP.get(url).send().await?.json().await?;

        let mut cache = JWKS_CACHE.write().unwrap();
        for key in resp.keys {
            let decoding = DecodingKey::from_rsa_components(&key.n, &key.e)?;
            cache.insert(key.kid.clone(), decoding);
        }

        cache
            .get(kid)
            .cloned()
            .context("Unable to find matching JWK for token")
    }

    pub async fn create_session_cookie(
        &self,
        id_token: &str,
        expires_in_ms: u64,
    ) -> Result<String> {
        let access_token = self.get_access_token().await?;
        let url = format!(
            "https://identitytoolkit.googleapis.com/v1/projects/{}/accounts:sessionCookie",
            self.project_id
        );

        let resp = HTTP
            .post(&url)
            .bearer_auth(access_token)
            .json(&json!({
                "idToken": id_token,
                "validDuration": expires_in_ms / 1000,
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("Failed to create session cookie: {body}");
        }

        #[derive(Deserialize)]
        struct SessionResponse {
            #[serde(rename = "sessionCookie")]
            session_cookie: String,
        }

        let data: SessionResponse = resp.json().await?;
        Ok(data.session_cookie)
    }

    pub async fn set_admin_claim(&self, uid: &str, grant: bool) -> Result<()> {
        let access_token = self.get_access_token().await?;
        let user = self.get_user(uid, &access_token).await?;
        let mut claims = user
            .get("customAttributes")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .unwrap_or_else(|| json!({}));

        if grant {
            claims["admin"] = json!(true);
        } else {
            if let Some(obj) = claims.as_object_mut() {
                obj.remove("admin");
            }
        }

        let claims_str = if claims.as_object().map(|o| o.is_empty()).unwrap_or(true) {
            None
        } else {
            Some(claims.to_string())
        };

        self.patch_user(uid, &access_token, claims_str).await
    }

    pub async fn update_user_email(
        &self,
        uid: &str,
        email: &str,
        email_verified: bool,
    ) -> Result<Value> {
        let access_token = self.get_access_token().await?;
        let url = format!(
            "https://identitytoolkit.googleapis.com/v1/projects/{}/accounts:update",
            self.project_id
        );

        let resp = HTTP
            .post(&url)
            .bearer_auth(&access_token)
            .json(&json!({
                "localId": uid,
                "email": email,
                "emailVerified": email_verified,
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("Failed to update user email: {body}");
        }

        Ok(resp.json().await?)
    }

    async fn get_user(&self, uid: &str, access_token: &str) -> Result<Value> {
        let url = format!(
            "https://identitytoolkit.googleapis.com/v1/projects/{}/accounts:lookup",
            self.project_id
        );
        let resp = HTTP
            .post(&url)
            .bearer_auth(access_token)
            .json(&json!({ "localId": [uid] }))
            .send()
            .await?;
        if !resp.status().is_success() {
            bail!("Failed to lookup user");
        }
        let data: Value = resp.json().await?;
        data["users"]
            .as_array()
            .and_then(|a| a.first().cloned())
            .context("User not found")
    }

    async fn patch_user(
        &self,
        uid: &str,
        access_token: &str,
        custom_attributes: Option<String>,
    ) -> Result<()> {
        let url = format!(
            "https://identitytoolkit.googleapis.com/v1/projects/{}/accounts:update",
            self.project_id
        );
        let mut body = json!({ "localId": uid });
        if let Some(attrs) = custom_attributes {
            body["customAttributes"] = json!(attrs);
        } else {
            body["customAttributes"] = json!(null);
        }

        let resp = HTTP
            .post(&url)
            .bearer_auth(access_token)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("Failed to update custom claims: {text}");
        }
        Ok(())
    }

    async fn get_access_token(&self) -> Result<String> {
        let mut guard = self.access_token.lock().await;
        let now = chrono::Utc::now();
        if let Some((token, exp)) = guard.as_ref() {
            if *exp > now + chrono::Duration::seconds(60) {
                return Ok(token.clone());
            }
        }

        let jwt = self.create_service_account_jwt()?;
        let resp = HTTP
            .post(&self.service_account.token_uri)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", &jwt),
            ])
            .send()
            .await?;

        let data: GoogleTokenResponse = resp.json().await?;
        let exp = now + chrono::Duration::seconds(data.expires_in as i64);
        *guard = Some((data.access_token.clone(), exp));
        Ok(data.access_token)
    }

    fn create_service_account_jwt(&self) -> Result<String> {
        use jsonwebtoken::{encode, Header};

        #[derive(Serialize)]
        struct Claims<'a> {
            iss: &'a str,
            sub: &'a str,
            aud: &'a str,
            iat: i64,
            exp: i64,
            scope: &'static str,
        }

        let now = chrono::Utc::now().timestamp();
        let claims = Claims {
            iss: &self.service_account.client_email,
            sub: &self.service_account.client_email,
            aud: &self.service_account.token_uri,
            iat: now,
            exp: now + 3600,
            scope: "https://www.googleapis.com/auth/identitytoolkit https://www.googleapis.com/auth/firebase.database https://www.googleapis.com/auth/cloud-platform",
        };

        let encoding_key =
            jsonwebtoken::EncodingKey::from_rsa_pem(self.service_account.private_key.as_bytes())?;
        encode(&Header::new(Algorithm::RS256), &claims, &encoding_key)
            .context("Failed to sign service account JWT")
    }
}

#[derive(Debug, Clone)]
pub struct VerifiedUser {
    pub uid: String,
    pub email: Option<String>,
    pub admin: bool,
    pub email_verified: bool,
}
