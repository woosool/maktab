//! Passwords remain in the OS credential store or, if it is unavailable, process memory.
use crate::model::Account;
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};
use zeroize::Zeroizing;
pub const UNAVAILABLE: &str = "Saved password unavailable.";
struct Password {
    secret: Zeroizing<String>,
    session_only: bool,
}
#[derive(Default)]
struct Passwords {
    values: HashMap<String, Password>,
}
impl Passwords {
    fn remember(
        &mut self,
        id: &str,
        secret: &str,
        persist: impl FnOnce() -> Result<String, String>,
    ) -> Result<(), String> {
        if id.is_empty() || secret.is_empty() {
            return Err("Enter an account password before saving.".into());
        }
        // A write alone is insufficient: some platform stores accept it but deny the later read.
        let persistent = persist().is_ok_and(|saved| saved == secret);
        self.values.insert(
            id.into(),
            Password {
                secret: Zeroizing::new(secret.into()),
                session_only: !persistent,
            },
        );
        Ok(())
    }
    fn get(&self, id: &str) -> Option<String> {
        self.values.get(id).map(|p| p.secret.to_string())
    }
}
fn session() -> &'static Mutex<Passwords> {
    static SESSION: OnceLock<Mutex<Passwords>> = OnceLock::new();
    SESSION.get_or_init(|| Mutex::new(Passwords::default()))
}
pub fn password(a: &Account) -> Result<String, String> {
    if let Some(secret) = session()
        .lock()
        .map_err(|_| "Password storage is busy")?
        .get(&a.id)
    {
        return Ok(secret);
    }
    let secret=keyring::Entry::new("com.barid.mail",&a.id).and_then(|entry|entry.get_password()).map_err(|_|format!("{UNAVAILABLE} Your operating system could not unlock this account's password. Open Settings → Email accounts → Edit and enter the app password again. If a keychain prompt appears, allow Barid access."))?;
    if secret.is_empty() {
        return Err(format!(
            "{UNAVAILABLE} Enter the app password again in Settings → Email accounts → Edit."
        ));
    }
    session()
        .lock()
        .map_err(|_| "Password storage is busy")?
        .values
        .insert(
            a.id.clone(),
            Password {
                secret: Zeroizing::new(secret.clone()),
                session_only: false,
            },
        );
    Ok(secret)
}
pub fn save_password(a: &Account, secret: &str) -> Result<(), String> {
    if a.id.is_empty() || secret.is_empty() {
        return Err("Enter an account password before saving.".into());
    }
    // Do not hold the session lock while the OS may be showing an authorization prompt.
    let persisted = keyring::Entry::new("com.barid.mail", &a.id)
        .and_then(|entry| {
            entry.set_password(secret)?;
            entry.get_password()
        })
        .map_err(|_| "System credential store unavailable".into());
    session()
        .lock()
        .map_err(|_| "Password storage is busy")?
        .remember(&a.id, secret, || persisted)
}
pub fn session_only(id: &str) -> bool {
    session()
        .lock()
        .ok()
        .and_then(|s| s.values.get(id).map(|p| p.session_only))
        .unwrap_or(false)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn denied_keychain_read_keeps_the_entered_password_for_this_session() {
        let mut s = Passwords::default();
        s.remember("a", "new-password", || Err("access denied".into()))
            .unwrap();
        assert_eq!(s.get("a").as_deref(), Some("new-password"));
        assert!(s.values["a"].session_only);
        assert!(Passwords::default().get("a").is_none());
    }
    #[test]
    fn successful_readback_is_persistent_and_accounts_remain_isolated() {
        let mut s = Passwords::default();
        s.remember("a", "one", || Ok("one".into())).unwrap();
        s.remember("b", "two", || Ok("two".into())).unwrap();
        assert!(!s.values["a"].session_only);
        assert_eq!(s.get("a").as_deref(), Some("one"));
        assert_eq!(s.get("b").as_deref(), Some("two"));
    }
    #[test]
    fn stale_readback_and_password_updates_use_the_latest_entered_secret() {
        let mut s = Passwords::default();
        s.remember("a", "new", || Ok("old".into())).unwrap();
        assert!(s.values["a"].session_only);
        assert_eq!(s.get("a").as_deref(), Some("new"));
        s.remember("a", "updated", || Ok("updated".into())).unwrap();
        assert!(!s.values["a"].session_only);
        assert_eq!(s.get("a").as_deref(), Some("updated"));
        assert!(s.remember("a", "", || Ok("".into())).is_err());
        assert_eq!(s.get("a").as_deref(), Some("updated"));
    }
}
