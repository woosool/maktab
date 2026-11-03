use super::{
    api::{GMAIL, GRAPH, request, segment},
    oauth::{access, client},
};
use crate::model::*;
use reqwest::Method;
use serde_json::json;
#[derive(Clone)]
pub enum Change {
    Create(String),
    Rename { path: String, name: String },
    Delete(String),
}
pub fn change(a: &Account, change: Change) -> Result<(), String> {
    let name = match &change {
        Change::Create(n) | Change::Rename { name: n, .. } => Some(n.as_str()),
        _ => None,
    };
    if name.is_some_and(|s| s.trim().is_empty() || s.contains(['\r', '\n', '\0']) || s.len() > 200)
    {
        return Err("Enter a folder name of 1–200 bytes without control characters.".into());
    }
    match a.connection {
        Connection::Pop3 => {
            return Err("POP3 folders are local. Use IMAP to manage server folders.".into());
        }
        Connection::Imap => {
            let mut s = super::connect(a)?;
            match change {
                Change::Create(name) => s.create(encode_mailbox(&name)),
                Change::Rename { path, name } => s.rename(&path, encode_mailbox(&name)),
                Change::Delete(path) => s.delete(&path),
            }
            .map_err(|e| e.to_string())?;
            let _ = s.logout();
        }
        Connection::Gmail | Connection::Outlook => {
            let token = access(a)?;
            let http = client()?;
            let gmail = a.connection == Connection::Gmail;
            let base = if gmail {
                format!("{GMAIL}/labels")
            } else {
                format!("{GRAPH}/mailFolders")
            };
            let (method, url, body) = match change {
                Change::Create(name) => (
                    Method::POST,
                    base,
                    Some(if gmail {
                        json!({"name":name,"labelListVisibility":"labelShow","messageListVisibility":"show"})
                    } else {
                        json!({"displayName":name})
                    }),
                ),
                Change::Rename { path, name } => (
                    Method::PATCH,
                    format!("{base}/{}", segment(&path)),
                    Some(if gmail {
                        json!({"name":name})
                    } else {
                        json!({"displayName":name})
                    }),
                ),
                Change::Delete(path) => {
                    (Method::DELETE, format!("{base}/{}", segment(&path)), None)
                }
            };
            request(&http, method, &url, &token, body)?;
        }
    }
    Ok(())
}
pub fn protected(folder: &Folder) -> bool {
    !folder.is_label
        && ["Inbox", "Sent", "Drafts", "Archive", "Junk", "Trash"].contains(&folder.name.as_str())
}

fn encode_mailbox(name: &str) -> String {
    use base64::{Engine, engine::general_purpose::STANDARD_NO_PAD};
    let mut output = String::new();
    let mut non_ascii = String::new();
    let flush = |output: &mut String, buffer: &mut String| {
        if !buffer.is_empty() {
            let bytes: Vec<u8> = buffer.encode_utf16().flat_map(u16::to_be_bytes).collect();
            output.push('&');
            output.push_str(&STANDARD_NO_PAD.encode(bytes).replace('/', ","));
            output.push('-');
            buffer.clear();
        }
    };
    for c in name.chars() {
        if (' '..='~').contains(&c) {
            flush(&mut output, &mut non_ascii);
            if c == '&' {
                output.push_str("&-");
            } else {
                output.push(c);
            }
        } else {
            non_ascii.push(c);
        }
    }
    flush(&mut output, &mut non_ascii);
    output
}
pub(super) fn decode_mailbox(name: &str) -> String {
    use base64::{Engine, engine::general_purpose::STANDARD_NO_PAD};
    let mut output = String::new();
    let mut remaining = name;
    while let Some(start) = remaining.find('&') {
        output.push_str(&remaining[..start]);
        let part = &remaining[start + 1..];
        let Some(end) = part.find('-') else {
            return name.into();
        };
        let encoded = &part[..end];
        if encoded.is_empty() {
            output.push('&');
        } else {
            let Ok(bytes) = STANDARD_NO_PAD.decode(encoded.replace(',', "/")) else {
                return name.into();
            };
            if bytes.len() % 2 != 0 {
                return name.into();
            }
            let units: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|b| u16::from_be_bytes([b[0], b[1]]))
                .collect();
            let Ok(value) = String::from_utf16(&units) else {
                return name.into();
            };
            output.push_str(&value);
        }
        remaining = &part[end + 1..];
    }
    output.push_str(remaining);
    output
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_folder_names_roundtrip_modified_utf7() {
        for name in ["Work & family", "العمل", "日本語", "Folder/مرحبا"] {
            assert_eq!(decode_mailbox(&encode_mailbox(name)), name);
        }
        assert_eq!(encode_mailbox("日本語"), "&ZeVnLIqe-");
    }
    #[test]
    fn builtin_folders_are_protected_but_custom_labels_can_be_managed() {
        assert!(protected(&Folder {
            name: "Inbox".into(),
            ..Default::default()
        }));
        assert!(!protected(&Folder {
            is_label: true,
            name: "Inbox".into(),
            ..Default::default()
        }));
    }
}
