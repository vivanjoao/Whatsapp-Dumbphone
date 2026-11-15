use anyhow::{Result, ensure};
use std::{collections::HashMap, net::SocketAddr, path::PathBuf};

#[derive(Clone)]
pub struct Config {
    pub password: String,
    pub secret: String,
    pub token: Option<String>,
    pub legacy_token_urls: bool,
    pub secure_cookie: bool,
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
    pub max_requests: usize,
    pub max_sends: usize,
}
impl Config {
    pub fn from_env() -> Result<Self> {
        Self::from_values(std::env::vars().collect())
    }
    pub fn from_values(values: HashMap<String, String>) -> Result<Self> {
        let get = |key: &str| values.get(key).cloned().unwrap_or_default();
        let password = get("PASSWORD");
        let secret = get("SESSION_SECRET");
        ensure!(
            password.chars().count() >= 12
                && !password.starts_with("REPLACE_")
                && password != "secretpassword",
            "PASSWORD must be unique and at least 12 characters"
        );
        ensure!(
            secret.len() >= 32
                && !secret.starts_with("REPLACE_")
                && secret != "fallback-secret-key",
            "SESSION_SECRET must contain at least 32 random characters"
        );
        let token = get("ACCESS_TOKEN");
        ensure!(
            token.is_empty()
                || ((32..=128).contains(&token.len())
                    && token.bytes().all(|b| b.is_ascii_hexdigit())),
            "ACCESS_TOKEN must be empty or 32–128 hexadecimal characters"
        );
        let boolean = |key: &str, default| -> Result<bool> {
            match values.get(key).map(String::as_str) {
                None | Some("") => Ok(default),
                Some("true") => Ok(true),
                Some("false") => Ok(false),
                _ => anyhow::bail!("{key} must be true or false"),
            }
        };
        let number = |key: &str, default: usize, max: usize| -> Result<usize> {
            let value = values
                .get(key)
                .map(|s| s.parse())
                .transpose()?
                .unwrap_or(default);
            ensure!((1..=max).contains(&value), "{key} out of range");
            Ok(value)
        };
        Ok(Self {
            password,
            secret,
            token: (!token.is_empty()).then_some(token),
            legacy_token_urls: boolean("LEGACY_TOKEN_URLS", true)?,
            secure_cookie: boolean("COOKIE_SECURE", false)?,
            bind: values
                .get("BIND_ADDR")
                .map(String::as_str)
                .unwrap_or("0.0.0.0:3000")
                .parse()?,
            data_dir: values
                .get("DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("./data")),
            max_requests: number("MAX_REQUESTS", 64, 1024)?,
            max_sends: number("MAX_SENDS", 4, 32)?,
        })
    }
}
