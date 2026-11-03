mod api;
pub mod contacts;
mod credentials;
pub mod folders;
pub mod message_html;
pub mod oauth;
mod pop;
use crate::model::*;
use lettre::{
    Message, SmtpTransport, Transport,
    message::{
        Attachment as MimeAttachment, Mailbox, Mailboxes, MultiPart, SinglePart,
        header::ContentType,
    },
    transport::smtp::authentication::Credentials,
};
use mailparse::MailHeaderMap;

type Session = imap::Session<native_tls::TlsStream<std::net::TcpStream>>;
pub use credentials::{UNAVAILABLE as PASSWORD_UNAVAILABLE, session_only as password_session_only};
pub use credentials::{password, save_password};
fn connect(a: &Account) -> Result<Session, String> {
    connect_with_password(a, &password(a)?)
}
fn connect_with_password(a: &Account, secret: &str) -> Result<Session, String> {
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::Duration;
    let addr = (a.imap_host.as_str(), a.imap_port)
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
        .next()
        .ok_or("IMAP host has no address")?;
    let stream =
        TcpStream::connect_timeout(&addr, Duration::from_secs(20)).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| e.to_string())?;
    let tls = native_tls::TlsConnector::new()
        .map_err(|e| e.to_string())?
        .connect(&a.imap_host, stream)
        .map_err(|e| e.to_string())?;
    let mut client = imap::Client::new(tls);
    client.read_greeting().map_err(|e| e.to_string())?;
    client
        .login(&a.email, secret)
        .map_err(|(e, _)| e.to_string())
}
pub fn sync(a: &Account) -> Result<SyncBatch, String> {
    if a.connection.api() {
        return api::sync(a);
    }
    if a.connection == Connection::Pop3 {
        return pop::sync(a);
    }
    let mut session = connect(a)?;
    let names: Vec<(String, String)> = session
        .list(None, Some("*"))
        .map_err(|e| e.to_string())?
        .iter()
        .filter(|n| {
            !format!("{:?}", n.attributes())
                .to_lowercase()
                .contains("noselect")
        })
        .map(|n| {
            let attrs = format!("{:?}", n.attributes()).to_lowercase();
            let lower = n.name().to_lowercase();
            let folder = if lower == "inbox" {
                "Inbox"
            } else if attrs.contains("sent")
                || lower.ends_with("sent")
                || lower.ends_with("sent mail")
            {
                "Sent"
            } else if attrs.contains("trash") || lower.ends_with("trash") {
                "Trash"
            } else if attrs.contains("junk") || lower.ends_with("spam") {
                "Junk"
            } else if attrs.contains("draft") || lower.ends_with("drafts") {
                "Drafts"
            } else if attrs.contains("archive") || lower.ends_with("archive") {
                "Archive"
            } else {
                n.name()
            };
            (n.name().to_string(), folders::decode_mailbox(folder))
        })
        .collect();
    // Folder enumeration must not leave a long-lived selected session carrying the whole sync.
    let _ = session.logout();
    let mut names = names;
    names.sort_by_key(|(path, name)| (name != "Inbox", path.clone()));
    let mut batch = SyncBatch {
        folders: names
            .iter()
            .map(|(path, name)| Folder {
                is_label: false,
                account: a.id.clone(),
                path: path.clone(),
                name: name.clone(),
            })
            .collect(),
        ..Default::default()
    };
    for (remote, folder) in &names {
        match retry_folder(|| download_folder(a, remote, folder)) {
            Ok(messages) => batch.messages.extend(messages),
            Err(error) => {
                batch.failed_folders.push(remote.clone());
                batch.warnings.push(format!("{folder}: {error}"));
            }
        }
    }
    let labels: std::collections::BTreeSet<_> = batch
        .messages
        .iter()
        .flat_map(|m| m.remote_labels.iter())
        .cloned()
        .collect();
    batch.folders.extend(labels.into_iter().map(|name| Folder {
        is_label: true,
        account: a.id.clone(),
        path: name.clone(),
        name,
    }));
    if !names.is_empty() && batch.failed_folders.len() == names.len() {
        return Err(format!(
            "No folders could be downloaded. {}",
            batch.warnings.join("; ")
        ));
    }
    Ok(batch)
}
fn retry_folder<T>(mut attempt: impl FnMut() -> Result<T, String>) -> Result<T, String> {
    for index in 0..3 {
        match attempt() {
            Ok(value) => return Ok(value),
            Err(error) => {
                let lower = error.to_lowercase();
                let transient = [
                    "connection lost",
                    "connection reset",
                    "connection aborted",
                    "broken pipe",
                    "timed out",
                    "timeout",
                    "unexpected eof",
                    "end of file",
                    "connection closed",
                    "bye",
                ]
                .iter()
                .any(|needle| lower.contains(needle));
                if !transient || index == 2 {
                    return Err(error);
                }
                std::thread::sleep(std::time::Duration::from_millis(200 * (index + 1)));
            }
        }
    }
    unreachable!()
}
fn download_folder(a: &Account, remote: &str, folder: &str) -> Result<Vec<Mail>, String> {
    let mut session = connect(a)?;
    let mailbox = session
        .examine(remote)
        .map_err(|e| format!("Cannot open {remote}: {e}"))?;
    let validity = mailbox
        .uid_validity
        .ok_or("Server did not return UIDVALIDITY; previous folder cache retained")?;
    let mut uids: Vec<_> = session
        .uid_search("ALL")
        .map_err(|e| e.to_string())?
        .into_iter()
        .collect();
    uids.sort_unstable();
    let mut result = Vec::new();
    // UID snapshots stay stable when other clients delete messages during sync. Small batches
    // avoid downloading an entire archive in one command and exceeding server response limits.
    for chunk in uids.chunks(25) {
        let set = chunk
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let messages = session
            .uid_fetch(set, "(UID FLAGS BODY.PEEK[])")
            .map_err(|e| e.to_string())?;
        for fetch in messages.iter() {
            let uid = fetch
                .uid
                .ok_or("Server returned a message without a UID; folder cache retained")?;
            let raw = fetch
                .body()
                .ok_or("Server returned an incomplete message; folder cache retained")?;
            let mut mail = parse_remote(a, &format!("{remote}:{validity}:{uid}"), folder, raw)?;
            mail.id = format!("{}:{remote}:{validity}:{uid}", a.id);
            mail.uid = Some(uid);
            mail.remote_folder = remote.into();
            mail.remote_labels = fetch
                .flags()
                .iter()
                .filter_map(|f| {
                    if let imap::types::Flag::Custom(label) = f {
                        Some(label.to_string())
                    } else {
                        None
                    }
                })
                .collect();
            mail.unread = !fetch.flags().contains(&imap::types::Flag::Seen);
            mail.starred = fetch.flags().contains(&imap::types::Flag::Flagged);
            result.push(mail);
        }
    }
    let _ = session.logout();
    Ok(result)
}

fn extract(part: &mailparse::ParsedMail<'_>, body: &mut String, attachments: &mut Vec<Attachment>) {
    let mut plain = Vec::new();
    let mut html = Vec::new();
    extract_parts(part, &mut plain, &mut html, attachments);
    *body = if plain.is_empty() {
        html2text::from_read(html.join("\n").as_bytes(), 85).unwrap_or_else(|_| {
            "Unable to convert HTML message to text. Export the original to inspect it.".into()
        })
    } else {
        plain.join("\n")
    };
}
fn extract_parts(
    part: &mailparse::ParsedMail<'_>,
    plain: &mut Vec<String>,
    html: &mut Vec<String>,
    attachments: &mut Vec<Attachment>,
) {
    let disposition = part.get_content_disposition();
    let filename = disposition
        .params
        .get("filename")
        .or_else(|| part.ctype.params.get("name"));
    if disposition.disposition == mailparse::DispositionType::Attachment
        || filename.is_some()
        || (!part.ctype.mimetype.starts_with("text/")
            && !part.ctype.mimetype.starts_with("multipart/"))
    {
        attachments.push(Attachment {
            name: filename.cloned().unwrap_or_else(|| "attachment".into()),
            mime: part.ctype.mimetype.clone(),
            data: part.get_body_raw().unwrap_or_default(),
        });
        return;
    }
    match part.ctype.mimetype.as_str() {
        "text/plain" => plain.push(part.get_body().unwrap_or_default()),
        "text/html" => html.push(part.get_body().unwrap_or_default()),
        _ => (),
    }
    for sub in &part.subparts {
        extract_parts(sub, plain, html, attachments);
    }
}

#[derive(Clone)]
pub enum RemoteAction {
    Label(String, bool),
    Read(bool),
    Star(bool),
    Move(String),
}
pub fn action(a: &Account, mail: &Mail, action: RemoteAction) -> Result<(), String> {
    if a.connection.api() {
        return api::action(a, mail, action);
    }
    if a.connection == Connection::Pop3 {
        return Ok(());
    }
    let Some(uid) = mail.uid else { return Ok(()) };
    let mut s = connect(a)?;
    s.select(&mail.remote_folder).map_err(|e| e.to_string())?;
    match action {
        RemoteAction::Label(label, apply) => {
            if !valid_keyword(&label) {
                return Err("The server label is not a valid IMAP keyword.".into());
            }
            s.uid_store(
                uid.to_string(),
                format!("{}FLAGS.SILENT ({label})", if apply { "+" } else { "-" }),
            )
            .map_err(|e| e.to_string())?;
        }
        RemoteAction::Read(read) => {
            s.uid_store(
                uid.to_string(),
                if read {
                    "+FLAGS.SILENT (\\Seen)"
                } else {
                    "-FLAGS.SILENT (\\Seen)"
                },
            )
            .map_err(|e| e.to_string())?;
        }
        RemoteAction::Star(star) => {
            s.uid_store(
                uid.to_string(),
                if star {
                    "+FLAGS.SILENT (\\Flagged)"
                } else {
                    "-FLAGS.SILENT (\\Flagged)"
                },
            )
            .map_err(|e| e.to_string())?;
        }
        RemoteAction::Move(destination) => {
            // Require UID MOVE: never EXPUNGE unrelated messages marked deleted by another client.
            s.uid_mv(uid.to_string(), &destination)
                .map_err(|e| e.to_string())?;
        }
    }
    let _ = s.logout();
    Ok(())
}
fn valid_keyword(label: &str) -> bool {
    !label.is_empty()
        && label
            .bytes()
            .all(|c| c.is_ascii_graphic() && !b"(){}%*\"\\]".contains(&c))
}
pub fn build_message(a: &Account, draft: &Draft) -> Result<Message, String> {
    let mut builder = Message::builder()
        .from(Mailbox::new(
            if a.name.is_empty() {
                None
            } else {
                Some(a.name.clone())
            },
            a.email
                .parse()
                .map_err(|e| format!("Invalid sender address: {e}"))?,
        ))
        .subject(&draft.subject);
    let mut count = 0;
    let mut seen = std::collections::HashSet::new();
    for (field, kind) in [(&draft.to, 0), (&draft.cc, 1), (&draft.bcc, 2)] {
        for recipient in parse_addresses(field)? {
            if !seen.insert(recipient.email.to_string().to_lowercase()) {
                continue;
            }
            builder = match kind {
                0 => builder.to(recipient),
                1 => builder.cc(recipient),
                _ => builder.bcc(recipient),
            };
            count += 1;
        }
    }
    if count == 0 {
        return Err("Add at least one recipient.".into());
    }
    if !draft.in_reply_to.is_empty() {
        builder = builder.in_reply_to(draft.in_reply_to.clone());
    }
    if !draft.references.is_empty() {
        builder = builder.references(draft.references.clone());
    }
    let rich = draft.mode == crate::rich_text::MessageMode::Html;
    let mut multipart = if rich {
        let mut related = MultiPart::related().multipart(MultiPart::alternative_plain_html(
            draft.body.clone(),
            draft.rich.html(&draft.body),
        ));
        let mut seen = std::collections::HashSet::new();
        for image in draft
            .rich
            .all_images()
            .into_iter()
            .filter(|i| seen.insert(i.cid.clone()))
        {
            related = related.singlepart(
                MimeAttachment::new_inline_with_name(image.cid.clone(), image.name.clone()).body(
                    image.data.clone(),
                    image
                        .mime
                        .parse::<ContentType>()
                        .map_err(|e| e.to_string())?,
                ),
            );
        }
        MultiPart::mixed().multipart(related)
    } else {
        MultiPart::mixed().singlepart(SinglePart::plain(draft.body.clone()))
    };
    for file in &draft.attachments {
        multipart =
            multipart.singlepart(MimeAttachment::new(file.name.clone()).body(
                file.data.clone(),
                file.mime.parse::<ContentType>().unwrap_or_else(|_| {
                    "application/octet-stream".parse().expect("valid MIME type")
                }),
            ));
    }
    builder.multipart(multipart).map_err(|e| e.to_string())
}
pub fn parse_addresses(value: &str) -> Result<Vec<Mailbox>, String> {
    if value.trim().is_empty() {
        return Ok(Vec::new());
    }
    value
        .parse::<Mailboxes>()
        .map(|m| m.into_iter().collect())
        .map_err(|e| format!("Invalid recipient list: {e}"))
}
pub fn reply_recipients(
    mail: &Mail,
    accounts: &[Account],
    all: bool,
) -> Result<(String, String), String> {
    let own: std::collections::HashSet<_> =
        accounts.iter().map(|a| a.email.to_lowercase()).collect();
    let mut seen = own;
    let mut to = Vec::new();
    let mut cc = Vec::new();
    let primary = if mail.reply_to.trim().is_empty() {
        &mail.from
    } else {
        &mail.reply_to
    };
    for address in parse_addresses(primary)? {
        if seen.insert(address.email.to_string().to_lowercase()) {
            to.push(address.to_string());
        }
    }
    // A reply to one's own sent message goes to its original recipients.
    if all || to.is_empty() {
        for field in [&mail.to, &mail.cc] {
            for address in parse_addresses(field)? {
                if seen.insert(address.email.to_string().to_lowercase()) {
                    if to.is_empty() {
                        to.push(address.to_string());
                    } else {
                        cc.push(address.to_string());
                    }
                }
            }
        }
    }
    Ok((to.join(", "), cc.join(", ")))
}
pub fn send(a: &Account, draft: &Draft) -> Result<Vec<u8>, String> {
    if a.connection.api() {
        return api::send(a, draft);
    }
    let message = build_message(a, draft)?;
    let transport = if a.smtp_port == 465 {
        SmtpTransport::relay(&a.smtp_host)
    } else {
        SmtpTransport::starttls_relay(&a.smtp_host)
    }
    .map_err(|e| e.to_string())?
    .port(a.smtp_port)
    .credentials(Credentials::new(a.email.clone(), password(a)?))
    .timeout(Some(std::time::Duration::from_secs(30)))
    .build();
    transport.send(&message).map_err(|e| e.to_string())?;
    Ok(message.formatted())
}

pub(super) fn parse_remote(
    a: &Account,
    id: &str,
    folder: &str,
    raw: &[u8],
) -> Result<Mail, String> {
    let parsed = mailparse::parse_mail(raw).map_err(|e| e.to_string())?;
    let mut body = String::new();
    let mut attachments = Vec::new();
    extract(&parsed, &mut body, &mut attachments);
    let h = |name: &str| parsed.headers.get_first_value(name).unwrap_or_default();
    Ok(Mail {
        id: format!("{}:remote:{id}", a.id),
        account: a.id.clone(),
        remote_id: id.into(),
        uid: Some(0),
        remote_folder: folder.into(),
        folder: folder.into(),
        from: h("From"),
        to: h("To"),
        cc: h("Cc"),
        subject: h("Subject"),
        message_id: h("Message-ID"),
        reply_to: h("Reply-To"),
        references: h("References"),
        in_reply_to: h("In-Reply-To"),
        date: mailparse::dateparse(&h("Date")).unwrap_or_else(|_| chrono::Utc::now().timestamp()),
        body,
        attachments,
        raw: raw.to_vec(),
        unread: true,
        ..Default::default()
    })
}
pub fn test_connection(a: &Account, secret: &str) -> Result<(), String> {
    if a.connection == Connection::Pop3 {
        pop::test(a, secret)?;
    } else {
        let mut session = connect_with_password(a, secret)?;
        session.logout().map_err(|e| e.to_string())?;
    }
    let credentials = Credentials::new(a.email.clone(), secret.to_owned());
    let transport = if a.smtp_port == 465 {
        SmtpTransport::relay(&a.smtp_host)
    } else {
        SmtpTransport::starttls_relay(&a.smtp_host)
    }
    .map_err(|e| e.to_string())?
    .port(a.smtp_port)
    .credentials(credentials)
    .timeout(Some(std::time::Duration::from_secs(20)))
    .build();
    if !transport.test_connection().map_err(|e| e.to_string())? {
        return Err("SMTP server did not accept the connection".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imap_keywords_cannot_inject_commands_or_system_flags() {
        assert!(valid_keyword("$label1"));
        assert!(valid_keyword("Work"));
        for value in ["", "Work label", "\\Seen", "tag)\r\nDELETE INBOX", "%", "*"] {
            assert!(!valid_keyword(value), "{value:?}");
        }
    }
    #[test]
    fn folder_retry_reconnects_after_connection_loss_and_stops_for_permanent_errors() {
        let mut attempts = 0;
        let result = retry_folder(|| {
            attempts += 1;
            if attempts < 3 {
                Err("Connection Lost".into())
            } else {
                Ok(vec!["complete-folder"])
            }
        })
        .unwrap();
        assert_eq!(attempts, 3);
        assert_eq!(result, vec!["complete-folder"]);
        let mut attempts = 0;
        let error = retry_folder::<()>(|| {
            attempts += 1;
            Err("Permission denied".into())
        })
        .unwrap_err();
        assert_eq!(attempts, 1);
        assert_eq!(error, "Permission denied");
    }
    #[test]
    fn folder_retry_is_bounded_when_server_keeps_disconnecting() {
        let mut attempts = 0;
        assert!(
            retry_folder::<()>(|| {
                attempts += 1;
                Err("Connection Lost".into())
            })
            .is_err()
        );
        assert_eq!(attempts, 3);
    }
    #[test]
    fn plain_text_mode_omits_html_even_when_formatting_is_retained_in_draft() {
        let mut draft = Draft {
            mode: crate::rich_text::MessageMode::Plain,
            to: "you@example.com".into(),
            body: "Plain content".into(),
            ..Default::default()
        };
        draft.rich.apply(0, 5, crate::rich_text::Mark::Bold);
        let message = build_message(
            &Account {
                email: "me@example.com".into(),
                ..Default::default()
            },
            &draft,
        )
        .unwrap();
        let raw = String::from_utf8(message.formatted()).unwrap();
        assert!(!raw.contains("text/html"));
        assert!(raw.contains("text/plain"));
        assert!(raw.contains("Plain content"));
    }
    #[test]
    fn rich_message_has_plain_html_and_cid_image_parts() {
        use crate::rich_text::{Alignment, InlineImage, Mark, RichBody, Span};
        let draft = Draft {
            to: "you@example.com".into(),
            body: "Hello".into(),
            rich: RichBody {
                paragraphs: Vec::new(),
                tables: Vec::new(),
                rules: Vec::new(),
                spans: vec![Span {
                    start: 0,
                    end: 5,
                    mark: Mark::Bold,
                }],
                images: vec![InlineImage {
                    cid: "photo@barid".into(),
                    name: "photo.png".into(),
                    mime: "image/png".into(),
                    data: vec![0, 255, 42],
                    position: 5,
                    width: 320,
                    alt: "Photo".into(),
                }],
                alignment: Alignment::Left,
            },
            ..Default::default()
        };
        let message = build_message(
            &Account {
                email: "me@example.com".into(),
                ..Default::default()
            },
            &draft,
        )
        .unwrap();
        let raw = message.formatted();
        let parsed = mailparse::parse_mail(&raw).unwrap();
        fn collect<'a>(
            mail: &'a mailparse::ParsedMail<'a>,
            parts: &mut Vec<&'a mailparse::ParsedMail<'a>>,
        ) {
            parts.push(mail);
            for part in &mail.subparts {
                collect(part, parts);
            }
        }
        let mut parts = Vec::new();
        collect(&parsed, &mut parts);
        assert!(
            parts.iter().any(
                |p| p.ctype.mimetype == "text/plain" && p.get_body().unwrap().contains("Hello")
            )
        );
        let html = parts
            .iter()
            .find(|p| p.ctype.mimetype == "text/html")
            .unwrap()
            .get_body()
            .unwrap();
        assert!(html.contains("font-weight:bold"));
        assert!(html.contains("cid:photo@barid"));
        let image = parts
            .iter()
            .find(|p| p.ctype.mimetype == "image/png")
            .unwrap();
        assert_eq!(image.get_body_raw().unwrap(), [0, 255, 42]);
        assert_eq!(
            image.headers.get_first_value("Content-ID").unwrap(),
            "<photo@barid>"
        );
    }
    #[test]
    fn recipient_lists_support_quoted_commas_and_bcc_is_private() {
        let account = Account {
            email: "me@example.com".into(),
            name: "Alex Morgan".into(),
            ..Default::default()
        };
        let draft = Draft {
            to: "\"Chen, Amelia\" <amelia@example.com>, bob@example.com".into(),
            cc: "AMELIA@example.com".into(),
            bcc: "private@example.com".into(),
            subject: "Test".into(),
            in_reply_to: "<parent@example.com>".into(),
            references: "<root@example.com> <parent@example.com>".into(),
            ..Default::default()
        };
        let message = build_message(&account, &draft).unwrap();
        assert_eq!(message.envelope().to().len(), 3);
        let raw = String::from_utf8(message.formatted()).unwrap();
        assert!(!raw.contains("private@example.com"));
        assert!(raw.contains("In-Reply-To: <parent@example.com>"));
        assert!(raw.contains("References: <root@example.com> <parent@example.com>"));
        assert!(raw.contains("Alex Morgan"));
    }
    #[test]
    fn reply_all_respects_reply_to_deduplicates_and_excludes_own_addresses() {
        let mail = Mail {
            from: "alerts@example.com".into(),
            reply_to: "Amelia <amelia@example.com>".into(),
            to: "Me <me@example.com>, Bob <bob@example.com>".into(),
            cc: "AMELIA@example.com, bob@example.com, Other <other@example.com>".into(),
            ..Default::default()
        };
        let accounts = vec![Account {
            email: "me@example.com".into(),
            ..Default::default()
        }];
        let (to, cc) = reply_recipients(&mail, &accounts, true).unwrap();
        assert!(to.contains("amelia@example.com"));
        assert!(!cc.contains("me@example.com"));
        assert_eq!(parse_addresses(&cc).unwrap().len(), 2);
        assert_eq!(reply_recipients(&mail, &accounts, false).unwrap().1, "");
    }
    #[test]
    fn malformed_recipient_cannot_inject_headers() {
        assert!(parse_addresses("victim@example.com\r\nBcc: attacker@example.com").is_err());
        assert!(
            build_message(
                &Account {
                    email: "me@example.com".into(),
                    ..Default::default()
                },
                &Draft::default()
            )
            .is_err()
        );
    }
    #[test]
    fn multipart_alternative_prefers_plain_text_without_duplicate_html() {
        let raw = b"Content-Type: multipart/alternative; boundary=x\r\n\r\n--x\r\nContent-Type: text/html\r\n\r\n<p>HTML version</p>\r\n--x\r\nContent-Type: text/plain\r\n\r\nPlain version\r\n--x--\r\n";
        let parsed = mailparse::parse_mail(raw).unwrap();
        let mut body = String::new();
        extract(&parsed, &mut body, &mut Vec::new());
        assert!(body.contains("Plain version"));
        assert!(!body.contains("HTML version"));
    }
    #[test]
    fn mime_decodes_plain_body_and_binary_attachment() {
        let raw = b"From: a@example.com\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=x\r\n\r\n--x\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\nCaf=C3=A9\r\n--x\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=sample.bin\r\nContent-Transfer-Encoding: base64\r\n\r\nAP8q\r\n--x--\r\n";
        let parsed = mailparse::parse_mail(raw).unwrap();
        let mut body = String::new();
        let mut files = Vec::new();
        extract(&parsed, &mut body, &mut files);
        assert!(body.contains("Café"));
        assert_eq!(files[0].name, "sample.bin");
        assert_eq!(files[0].data, [0, 255, 42]);
    }
    #[test]
    fn html_message_is_readable_without_loading_remote_resources() {
        let parsed = mailparse::parse_mail(b"Content-Type: text/html\r\n\r\n<h1>Hello</h1><p>A useful message.</p><img src=\"https://tracker.example/pixel\">").unwrap();
        let mut body = String::new();
        extract(&parsed, &mut body, &mut Vec::new());
        assert!(body.contains("Hello"));
        assert!(body.contains("A useful message."));
    }
}
