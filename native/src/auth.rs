use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
pub fn random() -> String {
    let mut bytes = [0; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}
pub fn digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}
pub fn equal(a: &str, b: &str) -> bool {
    !a.is_empty()
        && !b.is_empty()
        && bool::from(Sha256::digest(a.as_bytes()).ct_eq(&Sha256::digest(b.as_bytes())))
}
#[derive(Clone)]
pub struct Auth {
    secret: String,
    pub epoch: String,
}
impl Auth {
    pub fn new(secret: &str, password: &str, token: Option<&str>) -> Self {
        Self {
            secret: secret.into(),
            epoch: digest(&format!(
                "{}:{}:{}",
                digest(secret),
                digest(password),
                digest(token.unwrap_or_default())
            )),
        }
    }
    fn mac(&self, context: &str, data: &str) -> HmacSha256 {
        let mut mac =
            HmacSha256::new_from_slice(self.secret.as_bytes()).expect("HMAC supports any key size");
        mac.update(self.epoch.as_bytes());
        mac.update(&[0]);
        mac.update(context.as_bytes());
        mac.update(&[0]);
        mac.update(data.as_bytes());
        mac
    }
    pub fn issue(&self, context: &str) -> String {
        self.issue_at(context, now())
    }
    pub fn issue_at(&self, context: &str, time: i64) -> String {
        let data = format!("{}.{}", time + 3600, random());
        format!(
            "{}.{}",
            data,
            hex::encode(self.mac(context, &data).finalize().into_bytes())
        )
    }
    pub fn verify(&self, context: &str, value: &str) -> bool {
        if value.len() > 160 {
            return false;
        }
        let Some((data, signature)) = value.rsplit_once('.') else {
            return false;
        };
        let Some((expires, nonce)) = data.split_once('.') else {
            return false;
        };
        let Ok(expiry) = expires.parse::<i64>() else {
            return false;
        };
        if nonce.len() != 64
            || !nonce.bytes().all(|c| c.is_ascii_hexdigit())
            || expiry < now()
            || expiry > now() + 3600
        {
            return false;
        }
        let Ok(signature) = hex::decode(signature) else {
            return false;
        };
        self.mac(context, data).verify_slice(&signature).is_ok()
    }
}
