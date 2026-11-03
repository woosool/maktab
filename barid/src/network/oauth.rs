use crate::model::{Account, Connection};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Serialize, Deserialize)]
struct Tokens {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
    #[serde(default)]
    client_secret: String,
}
#[derive(Deserialize)]
struct TokenReply {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    expires_in: i64,
}
pub(super) fn client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(40))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())
}
pub(super) fn response(
    response: reqwest::blocking::Response,
) -> Result<reqwest::blocking::Response, String> {
    if response.status().is_success() {
        Ok(response)
    } else {
        Err(format!(
            "Mail service returned HTTP {}. Check account access and try again.",
            response.status().as_u16()
        ))
    }
}
fn endpoints(connection: Connection) -> (&'static str, &'static str, &'static str) {
    match connection {
        Connection::Gmail => (
            "https://accounts.google.com/o/oauth2/v2/auth",
            "https://oauth2.googleapis.com/token",
            "https://www.googleapis.com/auth/gmail.modify https://www.googleapis.com/auth/contacts.readonly",
        ),
        _ => (
            "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
            "https://login.microsoftonline.com/common/oauth2/v2.0/token",
            "offline_access User.Read Mail.ReadWrite Mail.Send Contacts.Read MailboxSettings.Read",
        ),
    }
}
fn entry(a: &Account) -> Result<keyring::Entry, String> {
    keyring::Entry::new("com.barid.oauth", &a.id).map_err(|e| e.to_string())
}
fn save(a: &Account, t: &Tokens) -> Result<(), String> {
    entry(a)?
        .set_password(&serde_json::to_string(t).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
fn nonce() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}
fn callback(target: &str, state: &str) -> Result<Option<String>, String> {
    let url = url::Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|_| "Invalid sign-in callback")?;
    if url.path() != "/callback" {
        return Ok(None);
    }
    let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    if params.get("state").is_none_or(|v| v != state) {
        return Ok(None);
    }
    if params.contains_key("error") {
        return Err("Sign-in was declined. You can try again.".into());
    }
    Ok(params.get("code").filter(|s| !s.is_empty()).cloned())
}
pub fn authorize(
    mut a: Account,
    secret: String,
    cancel: Arc<AtomicBool>,
) -> Result<Account, String> {
    let secret = if secret.is_empty() && a.connection == Connection::Gmail {
        entry(&a)
            .and_then(|e| e.get_password().map_err(|e| e.to_string()))
            .ok()
            .and_then(|s| serde_json::from_str::<Tokens>(&s).ok())
            .map(|t| t.client_secret)
            .unwrap_or_default()
    } else {
        secret
    };
    if a.oauth_client_id.trim().is_empty() {
        return Err("An OAuth application client ID is required.".into());
    }
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let redirect = format!(
        "http://{}:{}/callback",
        if a.connection == Connection::Outlook {
            "localhost"
        } else {
            "127.0.0.1"
        },
        listener.local_addr().map_err(|e| e.to_string())?.port()
    );
    let verifier = nonce();
    let state = nonce();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let (auth, token, scope) = endpoints(a.connection);
    let mut url = url::Url::parse(auth).map_err(|e| e.to_string())?;
    url.query_pairs_mut().extend_pairs([
        ("client_id", a.oauth_client_id.as_str()),
        ("response_type", "code"),
        ("redirect_uri", &redirect),
        ("scope", scope),
        ("state", &state),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
        ("prompt", "consent"),
    ]);
    if a.connection == Connection::Gmail {
        url.query_pairs_mut().append_pair("access_type", "offline");
    }
    webbrowser::open(url.as_str()).map_err(|_| "Could not open your browser for sign-in")?;
    let started = Instant::now();
    let code = loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Sign-in cancelled".into());
        }
        if started.elapsed() > Duration::from_secs(180) {
            return Err("Sign-in timed out. Try again.".into());
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .map_err(|e| e.to_string())?;
                let mut data = [0; 8192];
                let count = stream.read(&mut data).unwrap_or(0);
                let request = String::from_utf8_lossy(&data[..count]);
                let target = request
                    .lines()
                    .next()
                    .and_then(|l| l.strip_prefix("GET "))
                    .and_then(|l| l.split_whitespace().next())
                    .unwrap_or("");
                let result = callback(target, &state);
                let valid = matches!(&result, Ok(Some(_)) | Err(_));
                let message = if valid {
                    "Sign-in received. You may return to Barid."
                } else {
                    "Invalid callback. Return to your sign-in tab."
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    message.len(),
                    message
                );
                if let Some(code) = result? {
                    break code;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100))
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    let http = client()?;
    let mut params = vec![
        ("client_id", a.oauth_client_id.as_str()),
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", &redirect),
        ("code_verifier", &verifier),
    ];
    if !secret.is_empty() {
        params.push(("client_secret", &secret));
    }
    let reply: TokenReply = response(
        http.post(token)
            .form(&params)
            .send()
            .map_err(|e| e.to_string())?,
    )?
    .json()
    .map_err(|_| "Invalid token response")?;
    if reply.refresh_token.is_empty() {
        return Err(
            "The provider did not grant offline access. Reconnect and approve offline access."
                .into(),
        );
    }
    let profile_url = if a.connection == Connection::Gmail {
        "https://gmail.googleapis.com/gmail/v1/users/me/profile"
    } else {
        "https://graph.microsoft.com/v1.0/me?$select=mail,userPrincipalName,displayName"
    };
    let profile: serde_json::Value = response(
        http.get(profile_url)
            .bearer_auth(&reply.access_token)
            .send()
            .map_err(|e| e.to_string())?,
    )?
    .json()
    .map_err(|_| "Invalid account profile")?;
    a.email = if a.connection == Connection::Gmail {
        profile["emailAddress"].as_str()
    } else {
        profile["mail"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| profile["userPrincipalName"].as_str())
    }
    .ok_or("The provider did not return an email address")?
    .into();
    if a.email.parse::<lettre::Address>().is_err() {
        return Err("The provider returned an invalid email address".into());
    }
    if a.name.trim().is_empty() {
        a.name = profile["displayName"].as_str().unwrap_or(&a.email).into();
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("Sign-in cancelled".into());
    }
    save(
        &a,
        &Tokens {
            access_token: reply.access_token,
            refresh_token: reply.refresh_token,
            expires_at: chrono::Utc::now().timestamp() + reply.expires_in,
            client_secret: secret,
        },
    )?;
    Ok(a)
}
pub(super) fn access(a: &Account) -> Result<String, String> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = LOCK.lock().map_err(|_| "Token storage is busy")?;
    let stored = entry(a)?
        .get_password()
        .map_err(|_| "Reconnect this account to sign in again")?;
    let mut t: Tokens =
        serde_json::from_str(&stored).map_err(|_| "Reconnect this account to sign in again")?;
    if t.expires_at > chrono::Utc::now().timestamp() + 60 {
        return Ok(t.access_token);
    }
    let mut params = vec![
        ("client_id", a.oauth_client_id.as_str()),
        ("grant_type", "refresh_token"),
        ("refresh_token", &t.refresh_token),
    ];
    if !t.client_secret.is_empty() {
        params.push(("client_secret", &t.client_secret));
    }
    let reply: TokenReply = response(
        client()?
            .post(endpoints(a.connection).1)
            .form(&params)
            .send()
            .map_err(|e| e.to_string())?,
    )?
    .json()
    .map_err(|_| "Invalid token response")?;
    t.access_token = reply.access_token;
    if !reply.refresh_token.is_empty() {
        t.refresh_token = reply.refresh_token;
    }
    t.expires_at = chrono::Utc::now().timestamp() + reply.expires_in;
    save(a, &t)?;
    Ok(t.access_token)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn callback_requires_matching_state_and_path() {
        assert_eq!(
            callback("/callback?code=ok&state=expected", "expected").unwrap(),
            Some("ok".into())
        );
        assert!(
            callback("/callback?code=bad&state=wrong", "expected")
                .unwrap()
                .is_none()
        );
        assert!(
            callback("/other?code=ok&state=expected", "expected")
                .unwrap()
                .is_none()
        );
        assert!(callback("/callback?error=access_denied&state=expected", "expected").is_err());
    }
    #[test]
    fn pkce_matches_rfc_vector() {
        assert_eq!(
            URL_SAFE_NO_PAD.encode(Sha256::digest(
                b"dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
            )),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }
}
