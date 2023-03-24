use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};

use derivative::Derivative;
use reqwest::{header, StatusCode};
use serde::Deserialize;

// const URL: &str = "https://api.helloasso.com/v5";
const OAUTH2_TOKEN_URL: &str = "https://api.helloasso.com/oauth2/token";
const OAUTH2_REFRESH_TOKEN_URL: &str = OAUTH2_TOKEN_URL;

#[derive(Debug)]
pub enum Error {
    ReqwestErr(reqwest::Error),
    AuthErr(AuthenticationError),
}

#[derive(Debug, Deserialize)]
pub struct AuthenticationError {
    pub error: String,
    pub error_description: String,
}

#[derive(Clone, Derivative)]
#[derivative(Debug, PartialEq)]
pub struct HelloAsso {
    pub client_id: String,
    client_secret: String,
    access_token: String,
    refresh_token: String,
    token_outdated_after: SystemTime,
    #[derivative(PartialEq = "ignore")]
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct RefreshToken {
    access_token: String,
    refresh_token: String,
    /* token_type: String, */
    expires_in: u64,
}

impl HelloAsso {
    pub async fn new(client_id: String, client_secret: String) -> Result<Self, Error> {
        let client = HelloAsso::builder(client_id, client_secret)
            .get_token()
            .await?
            .config_client()?
            .build();

        Ok(client)
    }

    pub fn builder(client_id: String, client_secret: String) -> HelloAssoBuilder {
        HelloAssoBuilder {
            client_id,
            client_secret,
            access_token: None,
            refresh_token: None,
            token_type: None,
            token_outdated_after: None,
            client: None,
        }
    }

    pub async fn refresh_token(&mut self) -> Result<&mut Self, reqwest::Error> {
        // Prepare request body
        let mut tokens = HashMap::new();
        tokens.insert("client_id", self.client_id.clone());
        tokens.insert("refresh_token", self.refresh_token.clone());
        tokens.insert("grant_type", "refresh_token".to_string());

        // Get access and refresh token
        let answer_client = reqwest::Client::new();
        let token = answer_client
            .post(OAUTH2_REFRESH_TOKEN_URL)
            .form(&tokens)
            .send()
            .await?
            .json::<RefreshToken>()
            .await?;

        // Fill data
        self.access_token = token.access_token;
        self.refresh_token = token.refresh_token;
        self.token_outdated_after = SystemTime::now() + Duration::from_secs(token.expires_in);

        Ok(self)
    }
}

#[derive(Debug, Deserialize)]
pub struct HelloAssoBuilder {
    pub client_id: String,
    client_secret: String,
    access_token: Option<String>,
    refresh_token: Option<String>,
    token_type: Option<String>,
    token_outdated_after: Option<SystemTime>,
    #[serde(skip)]
    client: Option<reqwest::Client>,
}

#[derive(Debug, Deserialize)]
struct AccessTokenResponse {
    access_token: String,
    refresh_token: String,
    token_type: String,
    expires_in: u64,
}

impl HelloAssoBuilder {
    async fn get_token(&mut self) -> Result<&mut Self, Error> {
        // Prepare request body
        let mut tokens = HashMap::new();
        tokens.insert("client_id", self.client_id.clone());
        tokens.insert("client_secret", self.client_secret.clone());
        tokens.insert("grant_type", "client_credentials".to_string());

        // Get access and refresh token
        let answer_client = reqwest::Client::new();
        let response = answer_client
            .post(OAUTH2_TOKEN_URL)
            .form(&tokens)
            .send()
            .await
            .map_err(Error::ReqwestErr)?;

        match response.status() {
            StatusCode::OK => {
                let token = response
                    .json::<AccessTokenResponse>()
                    .await
                    .expect("Can't deserialize AccessTokenResponse");

                // Fill data
                self.access_token = Some(token.access_token);
                self.refresh_token = Some(token.refresh_token);
                self.token_type = Some(token.token_type);
                self.token_outdated_after =
                    Some(SystemTime::now() + Duration::from_secs(token.expires_in));
                Ok(self)
            }
            StatusCode::BAD_REQUEST => {
                let error = response
                    .json::<AuthenticationError>()
                    .await
                    .expect("Can't deserialize AuthenticationError");

                Err(Error::AuthErr(error))
            }
            status => {
                panic!(
                    "Unknown status code while fetching the access_token, {}",
                    status
                )
            }
        }
    }

    fn config_client(&mut self) -> Result<&mut Self, Error> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            format!(
                "Bearer {}",
                self.access_token
                    .clone()
                    .expect("Can't get the access_token, use get_token")
            )
            .parse()
            .expect("Can't parse formatted token into a HeaderName"),
        );
        self.client = Some(
            reqwest::Client::builder()
                .default_headers(headers)
                .build()
                .map_err(Error::ReqwestErr)?,
        );

        Ok(self)
    }

    fn build(&mut self) -> HelloAsso {
        HelloAsso {
            client_id: self.client_id.clone(),
            client_secret: self.client_secret.clone(),
            access_token: self.access_token.clone().unwrap_or_default(),
            refresh_token: self.refresh_token.clone().unwrap_or_default(),
            token_outdated_after: self.token_outdated_after.unwrap_or(SystemTime::UNIX_EPOCH),
            client: self.client.clone().unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Error, HelloAsso};
    use dotenv::dotenv;
    use log::{info, warn};
    use std::env;

    fn get_env_variables() -> (String, String) {
        if let Err(err) = dotenv() {
            warn!("Can't load .env file, {}", err);
        } else {
            info!(".env file loaded");
        }

        let client_id = env::var("CLIENT_ID").unwrap();
        let client_secret = env::var("CLIENT_SECRET").unwrap();

        (client_id, client_secret)
    }

    #[tokio::test]
    async fn new_client() {
        let (client_id, client_secret) = get_env_variables();

        HelloAsso::new(client_id, client_secret)
            .await
            .expect("Test failed");
    }

    #[tokio::test]
    async fn invalid_client_id() {
        let (_, client_secret) = get_env_variables();
        let client_id = "abc".to_string();

        let client = HelloAsso::new(client_id, client_secret).await;

        assert!(matches!(client, Err(Error::AuthErr(_))))
    }

    #[tokio::test]
    async fn invalid_client_secret() {
        let (client_id, _) = get_env_variables();
        let client_secret = "abc".to_string();

        let client = HelloAsso::new(client_id, client_secret).await;

        assert!(matches!(client, Err(Error::AuthErr(_))))
    }

    #[tokio::test]
    async fn refresh_token() {
        let (client_id, client_secret) = get_env_variables();

        let mut client = HelloAsso::new(client_id, client_secret)
            .await
            .expect("Can't create the client");

        client
            .refresh_token()
            .await
            .expect("Could not refresh token");
    }
}
