use super::{
    RemoteAction,
    oauth::{access, client, response},
};
use crate::model::*;
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use reqwest::{Method, blocking::Client};
use serde_json::{Value, json};
pub(super) const GMAIL: &str = "https://gmail.googleapis.com/gmail/v1/users/me";
pub(super) const GRAPH: &str = "https://graph.microsoft.com/v1.0/me";
pub(super) fn segment(id: &str) -> String {
    url::form_urlencoded::byte_serialize(id.as_bytes()).collect()
}
pub(super) fn request(
    http: &Client,
    method: Method,
    url: &str,
    token: &str,
    body: Option<Value>,
) -> Result<Value, String> {
    let mut request = http
        .request(method, url)
        .bearer_auth(token)
        .header("Prefer", "IdType=\"ImmutableId\"");
    if let Some(body) = body {
        request = request.json(&body);
    }
    let r = response(request.send().map_err(|e| e.to_string())?)?;
    if r.status() == 204 || r.status() == 202 {
        return Ok(Value::Null);
    }
    r.json().map_err(|_| "Invalid mail service response".into())
}
fn get(http: &Client, url: &str, token: &str) -> Result<Value, String> {
    request(http, Method::GET, url, token, None)
}
pub(super) fn next_graph(value: &Value) -> Result<Option<String>, String> {
    let Some(next) = value["@odata.nextLink"].as_str() else {
        return Ok(None);
    };
    let u = url::Url::parse(next).map_err(|_| "Invalid mail page")?;
    if u.scheme() != "https"
        || u.host_str() != Some("graph.microsoft.com")
        || u.port_or_known_default() != Some(443)
        || !u.username().is_empty()
        || u.password().is_some()
    {
        return Err("Invalid mail pagination destination".into());
    }
    Ok(Some(next.into()))
}
fn gmail_folder(labels: &[String]) -> &'static str {
    for (label, folder) in [
        ("TRASH", "Trash"),
        ("SPAM", "Junk"),
        ("DRAFT", "Drafts"),
        ("SENT", "Sent"),
        ("INBOX", "Inbox"),
    ] {
        if labels.iter().any(|s| s == label) {
            return folder;
        }
    }
    "Archive"
}
fn gmail_folders(a: &Account) -> Vec<Folder> {
    [
        ("INBOX", "Inbox"),
        ("SENT", "Sent"),
        ("DRAFT", "Drafts"),
        ("SPAM", "Junk"),
        ("TRASH", "Trash"),
        ("ARCHIVE", "Archive"),
    ]
    .into_iter()
    .map(|(path, name)| Folder {
        is_label: false,
        account: a.id.clone(),
        path: path.into(),
        name: name.into(),
    })
    .collect()
}
pub fn sync(a: &Account) -> Result<SyncBatch, String> {
    let token = access(a)?;
    let http = client()?;
    if a.connection == Connection::Gmail {
        gmail_sync(a, &http, &token)
    } else {
        outlook_sync(a, &http, &token)
    }
}
fn gmail_sync(a: &Account, http: &Client, token: &str) -> Result<SyncBatch, String> {
    let label_page = get(http, &format!("{GMAIL}/labels"), token)?;
    let labels: Vec<(String, String)> = label_page["labels"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|l| l["type"] == "user")
        .filter_map(|l| Some((l["id"].as_str()?.into(), l["name"].as_str()?.into())))
        .collect();
    let mut folders = gmail_folders(a);
    folders.extend(labels.iter().map(|(id, name)| Folder {
        is_label: true,
        account: a.id.clone(),
        path: id.clone(),
        name: name.clone(),
    }));
    let mut messages = Vec::new();
    let mut next = String::new();
    loop {
        let mut url = url::Url::parse(&format!("{GMAIL}/messages")).map_err(|e| e.to_string())?;
        url.query_pairs_mut()
            .extend_pairs([("maxResults", "100"), ("includeSpamTrash", "true")]);
        if !next.is_empty() {
            url.query_pairs_mut().append_pair("pageToken", &next);
        }
        let page = get(http, url.as_str(), token)?;
        for item in page["messages"].as_array().into_iter().flatten() {
            let id = item["id"].as_str().ok_or("Missing message ID")?;
            let value = get(
                http,
                &format!("{GMAIL}/messages/{}?format=raw", segment(id)),
                token,
            )?;
            let raw = URL_SAFE_NO_PAD
                .decode(
                    value["raw"]
                        .as_str()
                        .ok_or("Missing message source")?
                        .trim_end_matches('='),
                )
                .map_err(|_| "Invalid message source")?;
            let labels: Vec<String> = value["labelIds"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|l| l.as_str().map(String::from))
                .collect();
            let mut mail = super::parse_remote(a, id, gmail_folder(&labels), &raw)?;
            mail.thread_id = value["threadId"].as_str().unwrap_or("").into();
            mail.remote_labels = value["labelIds"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|l| {
                    let id = l.as_str()?;
                    folders
                        .iter()
                        .find(|f| f.is_label && f.path == id)
                        .map(|f| f.name.clone())
                })
                .collect();
            mail.unread = labels.iter().any(|l| l == "UNREAD");
            mail.starred = labels.iter().any(|l| l == "STARRED");
            messages.push(mail);
        }
        next = page["nextPageToken"].as_str().unwrap_or("").into();
        if next.is_empty() {
            break;
        }
    }
    Ok(SyncBatch {
        failed_folders: Vec::new(),
        warnings: Vec::new(),
        messages,
        folders,
    })
}
fn outlook_sync(a: &Account, http: &Client, token: &str) -> Result<SyncBatch, String> {
    let mut folders = Vec::new();
    for (key, name) in [
        ("inbox", "Inbox"),
        ("sentitems", "Sent"),
        ("drafts", "Drafts"),
        ("archive", "Archive"),
        ("junkemail", "Junk"),
        ("deleteditems", "Trash"),
    ] {
        let folder = get(
            http,
            &format!("{GRAPH}/mailFolders/{key}?$select=id"),
            token,
        )?;
        folders.push(Folder {
            is_label: false,
            account: a.id.clone(),
            path: folder["id"].as_str().ok_or("Missing folder ID")?.into(),
            name: name.into(),
        });
    }
    if let Ok(page) = get(
        http,
        &format!("{GRAPH}/outlook/masterCategories?$select=displayName"),
        token,
    ) {
        for category in page["value"].as_array().into_iter().flatten() {
            if let Some(name) = category["displayName"].as_str() {
                folders.push(Folder {
                    is_label: true,
                    account: a.id.clone(),
                    path: name.into(),
                    name: name.into(),
                });
            }
        }
    }
    let mut pages = vec![format!(
        "{GRAPH}/mailFolders?$top=100&$select=id,displayName,childFolderCount"
    )];
    let mut visited = std::collections::HashSet::new();
    while let Some(url) = pages.pop() {
        let page = get(http, &url, token)?;
        if let Some(next) = next_graph(&page)? {
            pages.push(next);
        }
        for value in page["value"].as_array().into_iter().flatten() {
            let id = value["id"].as_str().ok_or("Missing folder ID")?;
            if !visited.insert(id.to_string()) {
                continue;
            }
            if !folders.iter().any(|f| f.path == id) {
                folders.push(Folder {
                    is_label: false,
                    account: a.id.clone(),
                    path: id.into(),
                    name: value["displayName"].as_str().unwrap_or("Folder").into(),
                });
            }
            if value["childFolderCount"].as_u64().unwrap_or(0) > 0 {
                pages.push(format!("{GRAPH}/mailFolders/{}/childFolders?$top=100&$select=id,displayName,childFolderCount",segment(id)));
            }
        }
    }
    let mut messages = Vec::new();
    let mut next = Some(format!(
        "{GRAPH}/messages?$top=100&$select=id,parentFolderId,isRead,flag,conversationId,categories"
    ));
    while let Some(url) = next {
        let page = get(http, &url, token)?;
        for item in page["value"].as_array().into_iter().flatten() {
            let id = item["id"].as_str().ok_or("Missing message ID")?;
            let raw = response(
                http.get(format!("{GRAPH}/messages/{}/$value", segment(id)))
                    .bearer_auth(token)
                    .header("Prefer", "IdType=\"ImmutableId\"")
                    .send()
                    .map_err(|e| e.to_string())?,
            )?
            .bytes()
            .map_err(|e| e.to_string())?;
            let parent = item["parentFolderId"].as_str().unwrap_or("");
            let folder = folders
                .iter()
                .find(|f| f.path == parent)
                .map(|f| f.name.as_str())
                .unwrap_or("Archive");
            let mut mail = super::parse_remote(a, id, folder, &raw)?;
            mail.thread_id = item["conversationId"].as_str().unwrap_or("").into();
            mail.remote_labels = item["categories"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|c| c.as_str().map(String::from))
                .collect();
            mail.remote_folder = parent.into();
            mail.unread = !item["isRead"].as_bool().unwrap_or(false);
            mail.starred = item["flag"]["flagStatus"] == "flagged";
            messages.push(mail);
        }
        next = next_graph(&page)?;
    }
    Ok(SyncBatch {
        failed_folders: Vec::new(),
        warnings: Vec::new(),
        messages,
        folders,
    })
}
pub fn action(a: &Account, mail: &Mail, action: RemoteAction) -> Result<(), String> {
    let token = access(a)?;
    let http = client()?;
    let id = segment(&mail.remote_id);
    if a.connection == Connection::Gmail {
        let (mut add, mut remove) = (Vec::new(), Vec::new());
        match action {
            RemoteAction::Label(label, apply) => {
                if apply {
                    add.push(label);
                } else {
                    remove.push(label);
                }
            }
            RemoteAction::Read(read) => {
                if read {
                    remove.push("UNREAD".to_owned())
                } else {
                    add.push("UNREAD".to_owned())
                }
            }
            RemoteAction::Star(star) => {
                if star {
                    add.push("STARRED".to_owned())
                } else {
                    remove.push("STARRED".to_owned())
                }
            }
            RemoteAction::Move(folder) => {
                remove.extend(["INBOX", "TRASH", "SPAM"].map(String::from));
                if folder != "ARCHIVE" {
                    add.push(folder.clone());
                    remove.retain(|v| v != &folder);
                }
            }
        }
        request(
            &http,
            Method::POST,
            &format!("{GMAIL}/messages/{id}/modify"),
            &token,
            Some(json!({"addLabelIds":add,"removeLabelIds":remove})),
        )?;
    } else {
        let (method, suffix, body) = match action {
            RemoteAction::Label(label, apply) => {
                let mut labels = mail.remote_labels.clone();
                labels.retain(|s| s != &label);
                if apply {
                    labels.push(label);
                }
                (Method::PATCH, "", json!({"categories":labels}))
            }
            RemoteAction::Read(read) => (Method::PATCH, "", json!({"isRead":read})),
            RemoteAction::Star(star) => (
                Method::PATCH,
                "",
                json!({"flag":{"flagStatus":if star{"flagged"}else{"notFlagged"}}}),
            ),
            RemoteAction::Move(folder) => (Method::POST, "/move", json!({"destinationId":folder})),
        };
        request(
            &http,
            method,
            &format!("{GRAPH}/messages/{id}{suffix}"),
            &token,
            Some(body),
        )?;
    }
    Ok(())
}
pub fn send(a: &Account, draft: &Draft) -> Result<Vec<u8>, String> {
    let message = super::build_message(a, draft)?;
    let raw = message.formatted();
    let payload = api_mime(&raw, &draft.bcc)?;
    let token = access(a)?;
    let http = client()?;
    if a.connection == Connection::Gmail {
        request(
            &http,
            Method::POST,
            &format!("{GMAIL}/messages/send"),
            &token,
            Some(json!({"raw":URL_SAFE_NO_PAD.encode(payload)})),
        )?;
    } else {
        response(
            http.post(format!("{GRAPH}/sendMail"))
                .bearer_auth(token)
                .header("Content-Type", "text/plain")
                .body(STANDARD.encode(payload))
                .send()
                .map_err(|e| e.to_string())?,
        )?;
    }
    Ok(raw)
}
fn api_mime(raw: &[u8], bcc: &str) -> Result<Vec<u8>, String> {
    let bcc = super::parse_addresses(bcc)?;
    if bcc.is_empty() {
        return Ok(raw.to_vec());
    }
    let header = format!(
        "Bcc: {}\r\n",
        bcc.iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut payload = header.into_bytes();
    payload.extend_from_slice(raw);
    Ok(payload)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pagination_never_forwards_token_to_other_hosts() {
        assert!(next_graph(&json!({"@odata.nextLink":"https://evil.example/messages"})).is_err());
        assert!(
            next_graph(
                &json!({"@odata.nextLink":"https://graph.microsoft.com/v1.0/me/messages?$skip=10"})
            )
            .unwrap()
            .is_some()
        );
    }
    #[test]
    fn folders_prioritize_trash_over_sent() {
        assert_eq!(gmail_folder(&["SENT".into(), "TRASH".into()]), "Trash");
    }
    #[test]
    fn bcc_is_only_in_transport_payload() {
        let raw = b"Subject: Test\r\n\r\nBody";
        let payload = api_mime(raw, "private@example.com").unwrap();
        assert!(payload.starts_with(b"Bcc: private@example.com\r\n"));
        assert!(!raw.starts_with(b"Bcc:"));
        assert!(api_mime(raw, "bad\r\nInjected: yes").is_err());
    }
}
