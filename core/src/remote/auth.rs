use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub fn valid_pin_format(pin: &str) -> bool {
    (4..=8).contains(&pin.len()) && pin.chars().all(|c| c.is_ascii_digit())
}

pub fn hash_pin(pin: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default().hash_password(pin.as_bytes(), &salt).expect("argon2 hash").to_string()
}

pub fn verify_pin(pin: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|h| Argon2::default().verify_password(pin.as_bytes(), &h).is_ok())
        .unwrap_or(false)
}

fn mac(secret: &str) -> HmacSha256 {
    HmacSha256::new_from_slice(secret.as_bytes()).expect("hmac accepts any key length")
}

/// `<nonce>.<hmac(nonce)>`, both base64url without padding.
pub fn issue_token(secret: &str) -> String {
    let nonce: [u8; 16] = rand::random();
    let mut m = mac(secret);
    m.update(&nonce);
    format!("{}.{}", URL_SAFE_NO_PAD.encode(nonce), URL_SAFE_NO_PAD.encode(m.finalize().into_bytes()))
}

pub fn verify_token(secret: &str, token: &str) -> bool {
    let Some((n, s)) = token.split_once('.') else { return false };
    let (Ok(nonce), Ok(sig)) = (URL_SAFE_NO_PAD.decode(n), URL_SAFE_NO_PAD.decode(s)) else { return false };
    let mut m = mac(secret);
    m.update(&nonce);
    m.verify_slice(&sig).is_ok()
}

#[derive(Default)]
struct Entry {
    failures: Vec<Instant>,
    blocked_until: Option<Instant>,
}

/// Per-IP failure counter. Time is injected so tests do not sleep.
pub struct Lockout {
    max_failures: u32,
    window: Duration,
    block: Duration,
    entries: HashMap<IpAddr, Entry>,
}

impl Lockout {
    pub fn new(max_failures: u32, window: Duration, block: Duration) -> Self {
        Self { max_failures, window, block, entries: HashMap::new() }
    }

    /// 5 failures within 10 minutes block for 60 seconds.
    pub fn standard() -> Self {
        Self::new(5, Duration::from_secs(600), Duration::from_secs(60))
    }

    pub fn is_blocked(&mut self, ip: IpAddr, now: Instant) -> bool {
        match self.entries.get(&ip).and_then(|e| e.blocked_until) {
            Some(until) if now < until => true,
            _ => false,
        }
    }

    pub fn record_failure(&mut self, ip: IpAddr, now: Instant) {
        let (window, block, max) = (self.window, self.block, self.max_failures);
        let e = self.entries.entry(ip).or_default();
        e.failures.retain(|t| now.saturating_duration_since(*t) < window);
        e.failures.push(now);
        if e.failures.len() as u32 >= max {
            e.blocked_until = Some(now + block);
            e.failures.clear();
        }
    }

    pub fn record_success(&mut self, ip: IpAddr) {
        self.entries.remove(&ip);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn ip() -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 7))
    }

    #[test]
    fn pin_format() {
        assert!(valid_pin_format("1234"));
        assert!(valid_pin_format("12345678"));
        assert!(!valid_pin_format("123"));
        assert!(!valid_pin_format("123456789"));
        assert!(!valid_pin_format("12a4"));
        assert!(!valid_pin_format(""));
    }

    #[test]
    fn pin_hash_round_trip() {
        let h = hash_pin("4321");
        assert!(verify_pin("4321", &h));
        assert!(!verify_pin("4322", &h));
        assert!(!verify_pin("4321", "garbage"));
    }

    #[test]
    fn token_verifies_with_same_secret_only() {
        let t = issue_token("secret-a");
        assert!(verify_token("secret-a", &t));
        assert!(!verify_token("secret-b", &t), "rotated secret invalidates");
        assert!(!verify_token("secret-a", "nonsense"));
        assert!(!verify_token("secret-a", ""));
        let mut tampered = t.clone();
        tampered.push('x');
        assert!(!verify_token("secret-a", &tampered));
    }

    #[test]
    fn lockout_blocks_after_max_failures_then_expires() {
        let mut l = Lockout::new(3, Duration::from_secs(600), Duration::from_secs(60));
        let t0 = Instant::now();
        l.record_failure(ip(), t0);
        l.record_failure(ip(), t0 + Duration::from_secs(1));
        assert!(!l.is_blocked(ip(), t0 + Duration::from_secs(2)));
        l.record_failure(ip(), t0 + Duration::from_secs(2));
        assert!(l.is_blocked(ip(), t0 + Duration::from_secs(3)));
        assert!(l.is_blocked(ip(), t0 + Duration::from_secs(61)));
        assert!(!l.is_blocked(ip(), t0 + Duration::from_secs(63)));
    }

    #[test]
    fn old_failures_fall_out_of_window() {
        let mut l = Lockout::new(3, Duration::from_secs(10), Duration::from_secs(60));
        let t0 = Instant::now();
        l.record_failure(ip(), t0);
        l.record_failure(ip(), t0 + Duration::from_secs(1));
        l.record_failure(ip(), t0 + Duration::from_secs(20)); // first two expired
        assert!(!l.is_blocked(ip(), t0 + Duration::from_secs(21)));
    }

    #[test]
    fn success_clears_failures_and_ips_are_independent() {
        let mut l = Lockout::new(2, Duration::from_secs(600), Duration::from_secs(60));
        let other = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let t0 = Instant::now();
        l.record_failure(ip(), t0);
        l.record_success(ip());
        l.record_failure(ip(), t0);
        assert!(!l.is_blocked(ip(), t0));
        l.record_failure(other, t0);
        l.record_failure(other, t0);
        assert!(l.is_blocked(other, t0));
        assert!(!l.is_blocked(ip(), t0));
    }
}
