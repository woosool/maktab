use serde::{Deserialize, Serialize};

pub const FOLDERS: &[&str] = &[
    "Inbox", "Starred", "Snoozed", "Sent", "Drafts", "Archive", "Junk", "Trash",
];
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Connection {
    #[default]
    Imap,
    Pop3,
    Gmail,
    Outlook,
}
impl Connection {
    pub fn api(self) -> bool {
        matches!(self, Self::Gmail | Self::Outlook)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    #[serde(default)]
    pub contacts_url: String,
    #[serde(default)]
    pub connection: Connection,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub oauth_client_id: String,
    pub id: String,
    pub name: String,
    pub email: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    pub smtp_port: u16,
    #[serde(default = "account_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub signature: String,
}
impl Default for Account {
    fn default() -> Self {
        Self {
            contacts_url: String::new(),
            connection: Connection::Imap,
            provider: String::new(),
            oauth_client_id: String::new(),
            id: String::new(),
            name: String::new(),
            email: String::new(),
            imap_host: String::new(),
            imap_port: 993,
            smtp_host: String::new(),
            smtp_port: 587,
            enabled: true,
            signature: String::new(),
        }
    }
}
fn account_enabled() -> bool {
    true
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Folder {
    #[serde(default)]
    pub is_label: bool,
    pub account: String,
    pub path: String,
    pub name: String,
}
#[derive(Default)]
pub struct SyncBatch {
    pub failed_folders: Vec<String>,
    pub warnings: Vec<String>,
    pub folders: Vec<Folder>,
    pub messages: Vec<Mail>,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Attachment {
    pub name: String,
    pub mime: String,
    pub data: Vec<u8>,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Mail {
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub remote_labels: Vec<String>,
    #[serde(default)]
    pub thread_id: String,
    #[serde(default)]
    pub remote_id: String,
    #[serde(default)]
    pub mode: crate::rich_text::MessageMode,
    #[serde(default)]
    pub rich: crate::rich_text::RichBody,
    #[serde(default)]
    pub reply_to: String,
    #[serde(default)]
    pub references: String,
    #[serde(default)]
    pub in_reply_to: String,
    pub id: String,
    pub account: String,
    pub uid: Option<u32>,
    pub remote_folder: String,
    pub folder: String,
    pub from: String,
    pub to: String,
    pub cc: String,
    #[serde(default)]
    pub bcc: String,
    pub subject: String,
    pub body: String,
    pub date: i64,
    pub unread: bool,
    pub starred: bool,
    pub snoozed_until: Option<i64>,
    pub tags: Vec<String>,
    pub attachments: Vec<Attachment>,
    pub raw: Vec<u8>,
    pub message_id: String,
}
impl Mail {
    pub fn matches(&self, query: &str) -> bool {
        crate::search::matches(self, query)
    }
}
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Draft {
    #[serde(default)]
    pub mode: crate::rich_text::MessageMode,
    #[serde(default)]
    pub rich: crate::rich_text::RichBody,
    #[serde(default)]
    pub in_reply_to: String,
    #[serde(default)]
    pub references: String,
    pub id: String,
    pub account: String,
    pub to: String,
    pub cc: String,
    pub bcc: String,
    pub subject: String,
    pub body: String,
    pub attachments: Vec<Attachment>,
}
#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Contact {
    pub local_override: bool,
    pub source_account: String,
    pub source_id: String,
    pub id: String,
    pub name: String,
    pub email: String,
    pub phone: String,
    pub company: String,
    pub group: String,
    pub notes: String,
    pub favorite: bool,
    pub deleted: bool,
}
impl Contact {
    pub fn key(&self) -> &str {
        if self.id.is_empty() {
            &self.email
        } else {
            &self.id
        }
    }
    pub fn mailbox(&self) -> Result<String, String> {
        let address = self
            .email
            .parse::<lettre::Address>()
            .map_err(|e| e.to_string())?;
        Ok(lettre::message::Mailbox::new(
            if self.name.is_empty() {
                None
            } else {
                Some(self.name.clone())
            },
            address,
        )
        .to_string())
    }
    pub fn matches(&self, query: &str) -> bool {
        format!(
            "{} {} {} {} {}",
            self.name, self.email, self.company, self.phone, self.group
        )
        .to_lowercase()
        .contains(&query.to_lowercase())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub text_scale: f32,
    pub remember_searches: bool,
    pub search_suggestions: bool,
    pub plain_compose: bool,
    pub expanded_compose: bool,
    pub email_body_dark: bool,
    pub group_threads: bool,
    pub reading_below: bool,
    pub minimize_to_tray: bool,
    pub unread_badge: bool,
    #[serde(default)]
    pub language: crate::i18n::Language,
    #[serde(default)]
    pub setup_complete: bool,
    pub default_account: String,
    pub sync_minutes: u32,
    pub mark_read_on_open: bool,
    pub compact_rows: bool,
    pub notify_received: bool,
    pub notify_sent: bool,
    pub notify_previews: bool,
    pub notify_sound: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            text_scale: 1.0,
            remember_searches: true,
            search_suggestions: true,
            plain_compose: false,
            expanded_compose: false,
            email_body_dark: false,
            group_threads: true,
            reading_below: false,
            minimize_to_tray: false,
            unread_badge: true,
            language: Default::default(),
            setup_complete: false,
            default_account: String::new(),
            sync_minutes: 5,
            mark_read_on_open: true,
            compact_rows: true,
            notify_received: true,
            notify_sent: true,
            notify_previews: true,
            notify_sound: true,
        }
    }
}

pub fn demo_mail() -> Vec<Mail> {
    let now = chrono::Utc::now().timestamp();
    [
        ("Amelia Chen <amelia@studio.example>", "A little clarity for the week ahead", "Hi Alex,\n\nI’ve pulled together the notes from our conversation yesterday. There’s something exciting about seeing all the pieces start to fit.\n\nHere’s what we agreed on:\n\n• Keep the experience simple, thoughtful, and personal.\n• Give the details the attention they deserve.\n• Make room for a little unexpected delight.\n\nThe updated direction is ready for your thoughts. Take a look when you have a quiet moment — I’d love to hear what resonates.\n\nShall we find a time on Thursday to walk through it together?\n\nThanks,\nAmelia\n\nAmelia Chen\nDesign Director · Form & Field", "Work", true),
        ("Linear <notifications@linear.example>", "Your workspace, all caught up", "Your weekly digest is here. Three projects moved forward this week, and your team completed 24 issues.\n\nLooking good. Keep the momentum going.", "Updates", true),
        ("Oliver Park <oliver@park.example>", "Coffee on Friday?", "Hey Alex,\n\nIt’s been a while! There’s a new place on the corner that I think you’d love. Are you around Friday morning?\n\nOliver", "Personal", true),
        ("The Monocle Minute <editors@monocle.example>", "Good things take a different perspective", "A fresh perspective on design, culture and the places we call home.\n\nThis week: independent bookshops, thoughtful architecture, and the art of slowing down.", "Reading", false),
        ("Sofia Martinez <sofia@studio.example>", "Re: Brand explorations — round two", "Hi Alex,\n\nThese are looking beautiful. I especially like the warmer direction. Let’s keep exploring that palette.\n\nSofia", "Work", false),
        ("Notion <team@notion.example>", "A space for your next big idea", "Start with a blank page. Turn it into something wonderful.\n\nYour team workspace is ready whenever you are.", "Updates", false),
        ("Daniel Kim <daniel@kim.example>", "Photos from the weekend", "What a lovely weekend. I’ve finally sorted through the photos — so many good memories.\n\nLet’s do it again soon!", "Personal", false),
        ("Figma <hello@figma.example>", "A few things we think you’ll love", "A collection of ideas and inspiration for your next project.\n\nMade for people who care about the details.", "Reading", false),
    ].into_iter().enumerate().map(|(i, (from, subject, body, tag, unread))| Mail {
        id: format!("demo-{i}"), account: "demo".into(), folder: "Inbox".into(),
        from: from.into(), to: "Alex Morgan <alex@example.com>".into(), subject: subject.into(), body: body.into(),
        date: now - i as i64 * 9700, unread, starred: i == 0 || i == 4, tags: vec![tag.into()], ..Default::default()
    }).collect()
}

impl Draft {
    pub fn is_empty(&self) -> bool {
        [&self.to, &self.cc, &self.bcc, &self.subject, &self.body]
            .iter()
            .all(|value| value.trim().is_empty())
            && self.attachments.is_empty()
            && self.rich.images.is_empty()
            && self.rich.tables.is_empty()
    }
    pub fn as_mail(&self) -> Mail {
        Mail {
            mode: self.mode.clone(),
            rich: self.rich.clone(),
            id: self.id.clone(),
            account: self.account.clone(),
            folder: "Drafts".into(),
            to: self.to.clone(),
            cc: self.cc.clone(),
            bcc: self.bcc.clone(),
            subject: self.subject.clone(),
            body: self.body.clone(),
            attachments: self.attachments.clone(),
            in_reply_to: self.in_reply_to.clone(),
            references: self.references.clone(),
            date: chrono::Utc::now().timestamp(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_combines_filters_and_unicode_text() {
        let m = Mail {
            from: "Amelia <amelia@example.com>".into(),
            subject: "Café design".into(),
            unread: true,
            attachments: vec![Attachment::default()],
            tags: vec!["Work".into()],
            ..Default::default()
        };
        assert!(m.matches("from:amelia subject:café is:unread has:attachment Work"));
        assert!(!m.matches("from:amelia is:starred"));
        assert!(!m.matches("to:someone"));
    }
    #[test]
    fn draft_bcc_and_attachment_survive_serialization() {
        let draft = Draft {
            bcc: "private@example.com".into(),
            attachments: vec![Attachment {
                name: "résumé.pdf".into(),
                data: vec![0, 255, 42],
                ..Default::default()
            }],
            ..Default::default()
        };
        let restored: Draft =
            serde_json::from_str(&serde_json::to_string(&draft).unwrap()).unwrap();
        assert_eq!(restored.bcc, draft.bcc);
        assert_eq!(restored.attachments[0].data, draft.attachments[0].data);
    }
}
