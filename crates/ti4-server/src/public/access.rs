//! Shared-password access cookie: `<expiry>.<hmac>` signed with a server secret.

use sha2::{Digest, Sha256};

pub const COOKIE_NAME: &str = "ti4_access";
pub const COOKIE_MAX_AGE_SECS: u64 = 30 * 24 * 3600;

fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut k = [0u8; BLOCK];
    if key.len() > BLOCK {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(k.map(|b| b ^ 0x36));
    inner.update(msg);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(k.map(|b| b ^ 0x5c));
    outer.update(inner);
    outer.finalize().into()
}

/// Constant-time equality of two byte strings (via fixed-size digests).
#[must_use]
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let da = Sha256::digest(a);
    let db = Sha256::digest(b);
    let mut diff = 0u8;
    for (x, y) in da.iter().zip(db.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Signs and checks access cookies. Changing the password invalidates every cookie.
pub struct AccessGate {
    password: String,
    key: Vec<u8>,
}

impl AccessGate {
    #[must_use]
    pub fn new(password: String, secret: &[u8]) -> Self {
        let mut key = secret.to_vec();
        key.extend_from_slice(&Sha256::digest(password.as_bytes()));
        Self { password, key }
    }

    #[must_use]
    pub fn password_matches(&self, attempt: &str) -> bool {
        constant_time_eq(self.password.as_bytes(), attempt.as_bytes())
    }

    #[must_use]
    pub fn issue(&self, now_unix: u64) -> String {
        let expiry = now_unix + COOKIE_MAX_AGE_SECS;
        let mac = hmac_sha256(&self.key, expiry.to_string().as_bytes());
        format!("{expiry}.{}", hex::encode(mac))
    }

    #[must_use]
    pub fn verify(&self, value: &str, now_unix: u64) -> bool {
        let Some((expiry, mac)) = value.split_once('.') else {
            return false;
        };
        let Ok(expiry_secs) = expiry.parse::<u64>() else {
            return false;
        };
        let expected = hex::encode(hmac_sha256(&self.key, expiry.as_bytes()));
        constant_time_eq(expected.as_bytes(), mac.as_bytes()) && expiry_secs > now_unix
    }
}

/// Extracts the access cookie value from a `Cookie` header.
#[must_use]
pub fn cookie_value(header: &str) -> Option<&str> {
    header.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        (name == COOKIE_NAME).then_some(value)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_matches_rfc4231_case_2() {
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            hex::encode(mac),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn cookie_roundtrip_expiry_tamper_and_password_change() {
        let gate = AccessGate::new("pw".into(), b"secret");
        let c = gate.issue(1000);
        assert!(gate.verify(&c, 1001));
        assert!(!gate.verify(&c, 1000 + COOKIE_MAX_AGE_SECS + 1));
        assert!(!gate.verify(&format!("{c}0"), 1001));
        assert!(!gate.verify("garbage", 1001));
        assert!(!AccessGate::new("other".into(), b"secret").verify(&c, 1001));
        assert!(gate.password_matches("pw"));
        assert!(!gate.password_matches("pW"));
    }
}
