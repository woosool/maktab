mod desktop;
mod dialogs;
mod editor;
mod icons;
mod menus;
#[cfg(target_os = "macos")]
mod native_menu;
mod organize;
mod search_ui;
mod setup;
mod theme;
#[cfg(test)]
mod visual_tour;
use crate::{
    model::*,
    network::{self, RemoteAction},
    store::Store,
};
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use icons::{action_button, icon_label};
use std::{
    collections::HashSet,
    sync::mpsc::{self, Receiver, Sender},
};
const INK: Color32 = Color32::from_rgb(42, 44, 47);
const MUTED: Color32 = Color32::from_rgb(130, 134, 139);
const BLUE: Color32 = Color32::from_rgb(24, 118, 224);
const ACCENT: Color32 = Color32::from_rgb(255, 153, 0);
const BG: Color32 = Color32::from_rgb(250, 250, 250);
/// Shared geometry so the message list and reader headers line up.
const PANE_HEADER_HEIGHT: f32 = 32.0;
const PANE_HEADER_MARGIN: i8 = 8;
/// Label column width for the composer's From / To / Cc / Subject rows.
const ENVELOPE_LABEL_WIDTH: f32 = 52.0;

enum Event {
    ReaderImages(String, Vec<network::message_html::ImageResult>),
    Contacts(String, Result<Vec<Contact>, String>),
    Folders(String, Result<(), String>),
    Synced(String, Result<SyncBatch, String>),
    Sent(String, Box<Draft>, Result<Vec<u8>, String>),
    Action(String, RemoteAction, Result<(), String>),
}
pub struct Barid {
    organize: organize::Organize,
    desktop: Option<desktop::Desktop>,
    desktop_attempted: bool,
    quitting: bool,
    thread_members: std::collections::HashMap<String, Vec<String>>,
    setup: setup::Setup,
    #[cfg(target_os = "macos")]
    native_menu: Option<native_menu::NativeMenu>,
    store: Store,
    mails: Vec<Mail>,
    accounts: Vec<Account>,
    contacts: Vec<Contact>,
    folders: Vec<Folder>,
    folder: String,
    account_filter: Option<String>,
    selected: Option<String>,
    checked: HashSet<String>,
    query: String,
    collapsed_groups: HashSet<String>,
    search_history: Vec<String>,
    search_open: bool,
    search_cursor: Option<usize>,
    composer: Option<Draft>,
    compose_sessions: Vec<dialogs::ComposeSession>,
    editor_states: std::collections::HashMap<String, editor::EditorState>,
    active_editor: String,
    pending_editor: Option<editor::EditorCommand>,
    focused_composer: String,
    focus_search: bool,
    settings: bool,
    contact_window: bool,
    shortcuts: bool,
    account_form: Account,
    secret: String,
    status: String,
    busy: usize,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    dark: bool,
    unread_only: bool,
    sort_oldest: bool,
    signature: String,
    tag_input: String,
    dialogs: dialogs::DialogState,
    prefs: Preferences,
    last_sync: std::time::Instant,
    account_status: std::collections::HashMap<String, String>,
    demo: bool,
    refresh_pending: bool,
}
impl Barid {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_fonts(&cc.egui_ctx);
        theme::install(&cc.egui_ctx, 1.0);
        let path = directories::ProjectDirs::from("com", "barid", "Barid")
            .expect("User data directory unavailable")
            .data_dir()
            .to_owned();
        std::fs::create_dir_all(&path).expect("Cannot create Barid data directory");
        let store = Store::open(&path.join("mail.db")).expect("Cannot open mail database");
        let mut app = Self::with_store(store);
        app.first_run_setup();
        crate::i18n::set_language(app.prefs.language);
        #[cfg(target_os = "macos")]
        let app = {
            let mut app = app;
            match native_menu::NativeMenu::new(&cc.egui_ctx) {
                Ok(menu) => app.native_menu = Some(menu),
                Err(error) => app.status = format!("Menu unavailable: {error}"),
            }
            app
        };
        app
    }
    fn with_store(store: Store) -> Self {
        let accounts = store.load("account").unwrap_or_default();
        let mails = store.mails().unwrap_or_default();
        let folders = store.load::<Folder>("folder").unwrap_or_default();
        let contacts = store.load("contact").unwrap_or_default();
        let dark = store
            .load::<bool>("appearance")
            .unwrap_or_default()
            .first()
            .copied()
            .unwrap_or(false);
        let signature = store
            .load::<String>("signature")
            .unwrap_or_default()
            .first()
            .cloned()
            .unwrap_or_default();
        let prefs = store
            .load::<Preferences>("preferences")
            .unwrap_or_default()
            .into_iter()
            .next()
            .unwrap_or_default();
        let search_history = store
            .load::<Vec<String>>("search_history")
            .unwrap_or_default()
            .into_iter()
            .next()
            .unwrap_or_default();
        let (tx, rx) = mpsc::channel();
        let demo = accounts.is_empty();
        let mut mails = mails;
        if demo {
            mails.extend(demo_mail());
        }
        let selected = mails
            .iter()
            .filter(|m| m.folder == "Inbox")
            .max_by_key(|m| m.date)
            .map(|m| m.id.clone());
        let initial_folder = if accounts.len() > 1 {
            "All Inboxes"
        } else {
            "Inbox"
        };
        Self {
            organize: Default::default(),
            desktop: None,
            desktop_attempted: false,
            quitting: false,
            thread_members: Default::default(),
            setup: Default::default(),
            #[cfg(target_os = "macos")]
            native_menu: None,
            store,
            mails,
            accounts,
            contacts,
            folders,
            folder: initial_folder.into(),
            account_filter: None,
            selected,
            checked: HashSet::new(),
            query: String::new(),
            collapsed_groups: HashSet::new(),
            search_history,
            search_open: false,
            search_cursor: None,
            composer: None,
            compose_sessions: Vec::new(),
            editor_states: std::collections::HashMap::new(),
            active_editor: String::new(),
            pending_editor: None,
            focused_composer: String::new(),
            focus_search: false,
            settings: false,
            contact_window: false,
            shortcuts: false,
            account_form: Account {
                imap_port: 993,
                smtp_port: 587,
                enabled: true,
                ..Default::default()
            },
            secret: String::new(),
            status: if demo {
                "Preview workspace · Connect an account to receive your mail".into()
            } else {
                "Ready · Mail stored on this device".into()
            },
            busy: 0,
            tx,
            rx,
            dark,
            unread_only: false,
            sort_oldest: false,
            signature,
            tag_input: String::new(),
            dialogs: dialogs::DialogState::default(),
            prefs,
            last_sync: std::time::Instant::now(),
            account_status: std::collections::HashMap::new(),
            demo,
            refresh_pending: false,
        }
    }
    fn persist(&mut self, mail: &Mail) {
        if mail.account != "demo"
            && let Err(e) = self.store.save("mail", &mail.id, mail)
        {
            self.status = e;
        }
    }
    fn compose(&mut self, mode: &str) {
        if mode == "draft"
            && let Some(id) = self.selected.clone()
            && (self.composer.as_ref().is_some_and(|d| d.id == id)
                || self.compose_sessions.iter().any(|s| s.draft.id == id))
        {
            self.focus_compose(&id);
            return;
        }

        if let Err(e) = self.park_composer() {
            self.status = e;
            return;
        }
        self.dialogs.compose_slot = self.next_compose_slot();
        let mut d = Draft {
            id: uuid::Uuid::new_v4().to_string(),
            account: self.preferred_account(),
            body: self.signature.clone(),
            mode: if self.prefs.plain_compose {
                crate::rich_text::MessageMode::Plain
            } else {
                crate::rich_text::MessageMode::Html
            },
            ..Default::default()
        };
        if let Some(account) = self.accounts.iter().find(|a| a.id == d.account)
            && !account.signature.is_empty()
        {
            d.body = account.signature.clone();
        }
        self.dialogs.compose_error.clear();
        self.dialogs.compose_minimized = false;
        self.dialogs.compose_expanded = self.prefs.expanded_compose;
        if let Some(m) = self
            .mails
            .iter()
            .find(|m| Some(&m.id) == self.selected.as_ref())
        {
            if mode == "draft" {
                self.composer = Some(Draft {
                    mode: m.mode.clone(),
                    rich: m.rich.clone(),
                    id: m.id.clone(),
                    account: if self.accounts.iter().any(|a| a.id == m.account) {
                        m.account.clone()
                    } else {
                        self.accounts
                            .first()
                            .map(|a| a.id.clone())
                            .unwrap_or_default()
                    },
                    to: m.to.clone(),
                    cc: m.cc.clone(),
                    bcc: m.bcc.clone(),
                    subject: m.subject.clone(),
                    body: m.body.clone(),
                    attachments: m.attachments.clone(),
                    in_reply_to: m.in_reply_to.clone(),
                    references: m.references.clone(),
                });
                if let Some(draft) = &self.composer {
                    self.focused_composer = draft.id.clone();
                    self.active_editor = draft.id.clone();
                    self.editor_states
                        .entry(draft.id.clone())
                        .or_default()
                        .focus = true;
                }
                return;
            }
            if mode != "new" {
                d.account = m.account.clone();
                d.subject = format!(
                    "{}{}",
                    if mode == "forward" { "Fwd: " } else { "Re: " },
                    m.subject.trim_start_matches("Re: ")
                );
                if mode != "forward" {
                    match network::reply_recipients(m, &self.accounts, mode == "all") {
                        Ok((to, cc)) => {
                            d.to = to;
                            d.cc = cc;
                        }
                        Err(e) => {
                            self.status = e;
                            return;
                        }
                    }
                    d.in_reply_to = m.message_id.clone();
                    d.references = format!("{} {}", m.references, m.message_id)
                        .trim()
                        .to_string();
                }
                let signature = self
                    .accounts
                    .iter()
                    .find(|a| a.id == d.account)
                    .filter(|a| !a.signature.is_empty())
                    .map(|a| a.signature.as_str())
                    .unwrap_or(&self.signature);
                d.body = format!(
                    "{}\n\nOn {}, {} wrote:\n{}",
                    signature,
                    date(m.date, "%b %d, %Y"),
                    m.from,
                    m.body
                        .lines()
                        .map(|s| format!("> {s}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                );
                if mode == "forward" {
                    d.attachments = m.attachments.clone();
                }
            }
        }
        self.focused_composer = d.id.clone();
        self.active_editor = d.id.clone();
        self.editor_states.entry(d.id.clone()).or_default().focus = true;
        self.dialogs.show_cc = false;
        self.dialogs.show_bcc = false;
        self.composer = Some(d);
    }
    fn ids(&self) -> Vec<String> {
        if self.checked.is_empty() {
            self.selected
                .as_ref()
                .map(|id| {
                    if self.prefs.group_threads {
                        self.thread_members
                            .get(id)
                            .cloned()
                            .unwrap_or_else(|| vec![id.clone()])
                    } else {
                        vec![id.clone()]
                    }
                })
                .unwrap_or_default()
        } else {
            self.checked.iter().cloned().collect()
        }
    }
    fn action(&mut self, action: &str) {
        self.action_ids(action, self.ids());
    }
    fn action_ids(&mut self, action: &str, ids: Vec<String>) {
        if action == "pin" {
            let pinned = !ids
                .iter()
                .any(|id| self.mails.iter().any(|m| &m.id == id && m.pinned));
            for id in ids {
                if let Some(mail) = self.mails.iter_mut().find(|m| m.id == id) {
                    mail.pinned = pinned;
                    let mail = mail.clone();
                    self.persist(&mail);
                }
            }
            return;
        }
        if self.busy > 0 {
            self.status = "Wait for the current mail operation to finish".into();
            return;
        }
        let mark_read = ids
            .iter()
            .any(|id| self.mails.iter().any(|m| &m.id == id && m.unread));
        let mark_star = !ids
            .iter()
            .all(|id| self.mails.iter().any(|m| &m.id == id && m.starred));
        for id in ids {
            let Some(m) = self.mails.iter().find(|m| m.id == id).cloned() else {
                continue;
            };
            let remote = match action {
                "read" => RemoteAction::Read(mark_read),
                "star" => RemoteAction::Star(mark_star),
                folder => {
                    let destination = self
                        .folders
                        .iter()
                        .find(|f| f.account == m.account && (f.name == folder || f.path == folder))
                        .map(|f| f.path.clone());
                    let destination = match destination {
                        Some(path) => path,
                        None if m.uid.is_some() => {
                            self.status = format!(
                                "The account has no {folder} folder. Sync to refresh its folder list."
                            );
                            continue;
                        }
                        None => folder.to_string(),
                    };
                    RemoteAction::Move(destination)
                }
            };
            if let Some(a) = self
                .accounts
                .iter()
                .find(|a| a.id == m.account)
                .cloned()
                .filter(|_| m.uid.is_some())
            {
                let tx = self.tx.clone();
                self.busy += 1;
                std::thread::spawn(move || {
                    let result = network::action(&a, &m, remote.clone());
                    let _ = tx.send(Event::Action(id, remote, result));
                });
            } else {
                self.apply(&id, remote);
            }
        }
    }
    fn apply(&mut self, id: &str, action: RemoteAction) {
        if let Some(m) = self.mails.iter_mut().find(|m| m.id == id) {
            match action {
                RemoteAction::Label(label, apply) => {
                    let name = self
                        .folders
                        .iter()
                        .find(|f| f.account == m.account && f.path == label)
                        .map(|f| f.name.clone())
                        .unwrap_or(label);
                    m.remote_labels.retain(|l| l != &name);
                    if apply {
                        m.remote_labels.push(name);
                    }
                }
                RemoteAction::Read(v) => m.unread = !v,
                RemoteAction::Star(v) => m.starred = v,
                RemoteAction::Move(v) => {
                    m.folder = self
                        .folders
                        .iter()
                        .find(|f| f.account == m.account && f.path == v)
                        .map(|f| f.name.clone())
                        .unwrap_or_else(|| v.clone());
                    m.remote_folder = v;
                }
            }
            let m = m.clone();
            self.persist(&m);
        }
    }
    fn sync(&mut self) {
        if self.accounts.is_empty() {
            self.settings = true;
            return;
        }
        if self.busy > 0 {
            return;
        }
        if !self.accounts.iter().any(|a| a.enabled) {
            self.status = "All accounts have sync paused. Enable an account in Settings.".into();
            return;
        }
        self.status = "Connecting securely to your mail servers…".into();
        self.last_sync = std::time::Instant::now();
        for a in self.accounts.clone().into_iter().filter(|a| a.enabled) {
            self.account_status.insert(a.id.clone(), "Syncing…".into());
            let tx = self.tx.clone();
            self.busy += 1;
            std::thread::spawn(move || {
                let _ = tx.send(Event::Synced(a.id.clone(), network::sync(&a)));
            });
        }
    }
    fn events(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            if !matches!(&event, Event::ReaderImages(..)) {
                self.busy = self.busy.saturating_sub(1);
            }
            match event {
                Event::ReaderImages(id, images) => {
                    let state = self.editor_states.entry(id).or_default();
                    state.reader_loading = false;
                    let mut failures = 0;
                    for (cid, result) in images {
                        match result {
                            Ok(data) => {
                                let mime = network::message_html::validate_image(&data)
                                    .map(|(_, mime)| mime)
                                    .unwrap_or_default();
                                state.assets.insert(
                                    cid.clone(),
                                    crate::rich_text::InlineImage {
                                        cid,
                                        name: "Remote image".into(),
                                        mime,
                                        data,
                                        position: 0,
                                        width: 240,
                                        alt: "Remote image".into(),
                                    },
                                );
                            }
                            Err(_) => failures += 1,
                        }
                    }
                    if failures > 0 {
                        state.reader_image_error =
                            format!("{failures} images could not be loaded. You can try again.");
                    }
                }
                Event::Contacts(account, result) => match result {
                    Ok(incoming) => {
                        let mut contacts = self.contacts.clone();
                        for mut remote in incoming {
                            if let Some(old) = contacts.iter_mut().find(|c| {
                                c.source_account == account
                                    && c.source_id == remote.source_id
                                    && c.email.eq_ignore_ascii_case(&remote.email)
                            }) {
                                if old.local_override {
                                    continue;
                                }
                                remote.id = old.id.clone();
                                remote.favorite = old.favorite;
                                remote.group = old.group.clone();
                                remote.notes = old.notes.clone();
                                remote.deleted = old.deleted;
                                *old = remote;
                            } else {
                                contacts.push(remote);
                            }
                        }
                        match self.store.save_contacts(&contacts) {
                            Ok(()) => {
                                self.contacts = contacts;
                                self.status = "Contacts synchronized to this device".into();
                            }
                            Err(e) => self.status = e,
                        }
                    }
                    Err(e) => {
                        self.status = format!(
                            "Contacts sync failed: {e} API accounts may need reconnection with contacts permission."
                        )
                    }
                },
                Event::Folders(id, result) => match result {
                    Ok(()) => {
                        self.status = "Server folders updated".into();
                        self.organize.confirm_delete = false;
                        if self.busy == 0 {
                            self.sync_one(&id);
                        } else {
                            self.refresh_pending = true;
                        }
                    }
                    Err(e) => self.status = format!("Folder update failed: {e}"),
                },
                Event::Synced(account, Ok(mut batch)) => {
                    for mail in &batch.messages {
                        if let Some(state) =
                            self.editor_states.get_mut(&format!("read:{}", mail.id))
                        {
                            state.reader_fragment = None;
                        }
                    }
                    self.account_status.insert(
                        account.clone(),
                        format!(
                            "{} messages synced · {}",
                            batch.messages.len(),
                            chrono::Local::now().format("%H:%M")
                        ),
                    );
                    for mail in &mut batch.messages {
                        if let Some(old) = self.mails.iter().find(|old| old.id == mail.id) {
                            if self
                                .accounts
                                .iter()
                                .any(|a| a.id == account && a.connection == Connection::Pop3)
                            {
                                mail.folder = old.folder.clone();
                                mail.remote_folder = old.remote_folder.clone();
                                mail.unread = old.unread;
                                mail.starred = old.starred;
                            }
                            mail.pinned = old.pinned;
                            mail.tags = old.tags.clone();
                            mail.snoozed_until = old.snoozed_until;
                        }
                    }
                    if self
                        .accounts
                        .iter()
                        .any(|a| a.id == account && a.connection == Connection::Pop3)
                    {
                        let missing: Vec<_> = self
                            .mails
                            .iter()
                            .filter(|m| {
                                m.account == account
                                    && m.uid.is_some()
                                    && !batch.messages.iter().any(|new| new.id == m.id)
                            })
                            .cloned()
                            .collect();
                        batch.messages.extend(missing);
                    }
                    // Announce only genuinely new inbox mail, never an account's first sync.
                    let had_mail = self.mails.iter().any(|m| m.account == account);
                    let arrived: Vec<(String, String)> = batch
                        .messages
                        .iter()
                        .filter(|m| {
                            had_mail
                                && m.unread
                                && m.folder == "Inbox"
                                && !self.mails.iter().any(|old| old.id == m.id)
                        })
                        .map(|m| (sender(&m.from), m.subject.clone()))
                        .collect();
                    match self.store.apply_sync(&account, &batch) {
                        Ok(()) => {
                            if self.prefs.notify_received
                                && let Some((title, body)) =
                                    crate::notify::received_text(&arrived, self.prefs.notify_previews)
                            {
                                crate::notify::show(title, body, self.prefs.notify_sound);
                            }
                            let count = batch.messages.len();
                            self.mails.retain(|m| {
                                m.account != account
                                    || m.uid.is_none()
                                    || batch.failed_folders.contains(&m.remote_folder)
                            });
                            self.mails.extend(batch.messages);
                            self.folders.retain(|f| f.account != account);
                            self.folders.extend(batch.folders);
                            if self
                                .selected
                                .as_ref()
                                .is_none_or(|id| !self.mails.iter().any(|m| &m.id == id))
                            {
                                self.selected = self
                                    .mails
                                    .iter()
                                    .filter(|m| self.visible_in(m, &self.folder))
                                    .max_by_key(|m| m.date)
                                    .map(|m| m.id.clone());
                            }
                            self.checked
                                .retain(|id| self.mails.iter().any(|m| &m.id == id));
                            self.status = if batch.warnings.is_empty() {
                                format!(
                                    "Synced {count} messages · {}",
                                    chrono::Local::now().format("%H:%M")
                                )
                            } else {
                                format!(
                                    "Synced {count} messages; some folders need another refresh (cached mail kept): {}",
                                    batch.warnings.join("; ")
                                )
                            };
                            self.account_status
                                .insert(account.clone(), self.status.clone());
                        }
                        Err(e) => {
                            self.status =
                                format!("Sync could not be saved; previous cache retained: {e}")
                        }
                    }
                }
                Event::Synced(account, Err(e)) => {
                    if e.starts_with(network::PASSWORD_UNAVAILABLE)
                        && !self.setup.open
                        && let Some(a) = self.accounts.iter().find(|a| a.id == account)
                    {
                        self.account_form = a.clone();
                        self.secret.clear();
                        self.dialogs.section = 0;
                        self.settings = true;
                    }

                    self.account_status
                        .insert(account, format!("Sync failed: {e}"));
                    self.status = format!("Sync failed: {e}");
                }
                Event::Action(id, action, Ok(())) => {
                    if matches!(action, RemoteAction::Move(_)) {
                        self.mails.retain(|m| m.id != id);
                        self.checked.remove(&id);
                        if let Err(e) = self.store.remove("mail", &id) {
                            self.status = e;
                        }
                        self.refresh_pending = true;
                    } else {
                        self.apply(&id, action);
                    }
                    self.status = "Message updated".into();
                }
                Event::Action(_, _, Err(e)) => {
                    self.status = format!("Server action failed; message unchanged: {e}")
                }
                Event::Sent(account, draft, result) => match result {
                    Ok(raw) => {
                        if self.prefs.notify_sent {
                            let body = if self.prefs.notify_previews {
                                format!(
                                    "{} · {}",
                                    draft.to,
                                    if draft.subject.is_empty() {
                                        crate::i18n::display("(no subject)")
                                    } else {
                                        draft.subject.clone()
                                    }
                                )
                            } else {
                                String::new()
                            };
                            crate::notify::show(
                                crate::i18n::display("Message sent"),
                                body,
                                false,
                            );
                        }
                        let m = Mail {
                            mode: draft.mode,
                            rich: draft.rich,
                            id: uuid::Uuid::new_v4().to_string(),
                            account: account.clone(),
                            folder: "Sent".into(),
                            from: self
                                .accounts
                                .iter()
                                .find(|a| a.id == account)
                                .map(|a| a.email.clone())
                                .unwrap_or_default(),
                            to: draft.to,
                            cc: draft.cc,
                            subject: draft.subject,
                            body: draft.body,
                            date: chrono::Utc::now().timestamp(),
                            raw,
                            attachments: draft.attachments,
                            ..Default::default()
                        };
                        self.persist(&m);
                        self.mails.push(m);
                        self.mails.retain(|m| m.id != draft.id);
                        let _ = self.store.remove("mail", &draft.id);
                        self.status = "Message sent successfully".into();
                    }
                    Err(e) => {
                        if let Err(save_error) = self.park_composer() {
                            self.status = format!(
                                "Sending failed: {e}. Failed message remains in Drafts; current draft could not be saved: {save_error}"
                            );
                            continue;
                        }
                        self.dialogs.compose_slot = self.next_compose_slot();
                        self.dialogs.compose_minimized = false;
                        self.dialogs.compose_expanded = false;
                        self.composer = Some(*draft);
                        self.status = format!("Sending failed; draft restored: {e}");
                    }
                },
            }
        }
    }
    fn select_folder(&mut self, folder: &str, account: Option<String>) {
        self.folder = folder.into();
        self.account_filter = account;
        self.checked.clear();
        self.selected = self
            .mails
            .iter()
            .filter(|m| self.visible_in(m, folder) && m.matches(&self.query))
            .max_by_key(|m| m.date)
            .map(|m| m.id.clone());
    }
    fn toolbar(&mut self, root: &mut egui::Ui) {
        egui::Panel::top("main_toolbar_v2")
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(surface(self.dark))
                    .inner_margin(egui::Margin::symmetric(12, 6)),
            )
            .show(root, |ui| {
                theme::scope(ui, |ui| {
                    let (row, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 40.0),
                        egui::Sense::hover(),
                    );
                    ui.scope_builder(
                        egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                            if self.prefs.language.rtl() {
                                egui::pos2(row.right() - 130.0, row.top())
                            } else {
                                row.min
                            },
                            Vec2::new(130.0, 40.0),
                        )),
                        |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.add_space(4.0);
                                let (mark, _) =
                                    ui.allocate_exact_size(Vec2::splat(28.0), egui::Sense::hover());
                                ui.painter().rect_filled(mark, 8.0, ACCENT);
                                draw_icon(ui.painter(), mark.center(), "mail", Color32::WHITE, 0.95);
                                ui.add_space(2.0);
                                ui.label(crate::i18n::widget(
                                    RichText::new(crate::i18n::display("Barid"))
                                        .font(bold_font(16.0)),
                                    ui.style(),
                                ));
                            });
                        },
                    );
                    let width = (row.width() * 0.44).clamp(280.0, 620.0);
                    let search = egui::Rect::from_center_size(row.center(), Vec2::new(width, 36.0));
                    let palette = theme::palette(self.dark);
                    let focused = ui.ctx().memory(|m| m.has_focus(egui::Id::unique("global_search")));
                    ui.scope_builder(egui::UiBuilder::new().max_rect(search), |ui| {
                        egui::Frame::new()
                            .fill(if focused { palette.input } else { palette.control })
                            .stroke(Stroke::new(
                                1.0,
                                if focused { palette.accent } else { Color32::TRANSPARENT },
                            ))
                            .corner_radius(10.0)
                            .inner_margin(egui::Margin::symmetric(12, 3))
                            .show(ui, |ui| {
                                ui.set_width(width - 24.0);
                                ui.horizontal_centered(|ui| {
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    let (rect, _) = ui.allocate_exact_size(
                                        Vec2::splat(18.0),
                                        egui::Sense::hover(),
                                    );
                                    draw_icon(ui.painter(), rect.center(), "search", palette.text_muted, 0.95);
                                    let trailing = if self.query.is_empty() { 32.0 } else { 72.0 };
                                    let response = ui.add(
                                        egui::TextEdit::singleline(&mut self.query)
                                            .id(egui::Id::unique("global_search"))
                                            .frame(egui::Frame::NONE)
                                            .hint_text(crate::i18n::display("Search mail…"))
                                            .align(egui::Align2::LEFT_CENTER)
                                            .desired_width(ui.available_width() - trailing),
                                    );
                                    self.search_popup(ui, &response);
                                    if !self.query.is_empty()
                                        && tool_button(ui, "", "close", false)
                                            .on_hover_text(crate::i18n::display("Clear search"))
                                            .clicked()
                                    {
                                        self.query.clear();
                                        self.search_open = false;
                                        let folder = self.folder.clone();
                                        self.select_folder(&folder, self.account_filter.clone());
                                        response.request_focus();
                                    }

                                    if response.changed() {
                                        let folder = self.folder.clone();
                                        self.select_folder(&folder, self.account_filter.clone());
                                    }
                                    let (rect, advanced) = ui.allocate_exact_size(
                                        Vec2::splat(22.0),
                                        egui::Sense::click(),
                                    );
                                    advanced.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::Role::Button,
                                            ui.is_enabled(),
                                            "Advanced search",
                                        )
                                    });
                                    if advanced.hovered() {
                                        ui.painter().rect_filled(rect, 5.0, palette.control_hover);
                                    }
                                    draw_icon(ui.painter(), rect.center(), "filter", palette.text_muted, 0.9);
                                    if advanced
                                        .on_hover_text(crate::i18n::display("Advanced search"))
                                        .clicked()
                                    {
                                        self.organize.advanced = true;
                                    }
                                    if self.focus_search {
                                        response.request_focus();
                                        self.focus_search = false;
                                    }
                                });
                            });
                    });
                    ui.add_space(5.0);
                    ui.horizontal_wrapped(|ui| {
                        if tool_button(ui, "New", "plus", true).clicked() {
                            self.compose("new");
                        }
                        ui.add_enabled_ui(
                            self.busy == 0 && self.accounts.iter().any(|a| a.enabled),
                            |ui| {
                                if tool_button(ui, "Refresh", "refresh", false).clicked() {
                                    self.sync();
                                }
                            },
                        );
                        ui.separator();
                        tool_menu(ui, "Inboxes", "inbox", |ui| {
                            if ui.add(action_button("Folders & labels…")).clicked() {
                                self.organize.folders = true;
                                ui.close();
                            }
                            if ui.add(action_button("Minimize to tray")).clicked() {
                                self.hide_to_tray(ui.ctx());
                                ui.close();
                            }
                            if ui.add(action_button("Add an email account")).clicked() {
                                self.open_setup();
                                ui.close();
                            }
                            ui.separator();
                            if ui
                                .selectable_label(
                                    self.account_filter.is_none(),
                                    crate::i18n::widget(
                                        crate::i18n::display("All inboxes"),
                                        ui.style(),
                                    ),
                                )
                                .clicked()
                            {
                                self.select_folder("All Inboxes", None);
                                ui.close();
                            }
                            ui.separator();
                            for account in self.accounts.clone() {
                                if ui
                                    .selectable_label(
                                        self.account_filter.as_ref() == Some(&account.id),
                                        format!("{} <{}>", account.name, account.email),
                                    )
                                    .clicked()
                                {
                                    self.select_folder("Inbox", Some(account.id));
                                    ui.close();
                                }
                            }
                            ui.separator();
                            if ui.add(action_button("Manage accounts…")).clicked() {
                                self.dialogs.section = 0;
                                self.settings = true;
                                ui.close();
                            }
                        });
                        self.window_menu(ui);
                    });
                })
            });
    }
    fn sidebar(&mut self, root: &mut egui::Ui) {
        (if self.prefs.language.rtl() {
            egui::Panel::right("navigation_v2_rtl")
        } else {
            egui::Panel::left("navigation_v2")
        })
        .exact_size(234.0)
        .resizable(false)
        .frame(
            egui::Frame::new()
                .fill(surface(self.dark))
                .inner_margin(0.0),
        )
        .show(root, |ui| {
            theme::scope(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(14, 12))
                    .show(ui, |ui| {
                        ui.label(crate::i18n::widget(
                            RichText::new(crate::i18n::display("Mail"))
                                .size(17.0)
                                .strong(),
                            ui.style(),
                        ));
                    });
                let height = (ui.available_height() - 46.0).max(60.0);
                egui::ScrollArea::vertical()
                    .id_salt("folder_tree_v2")
                    .max_height(height)
                    .show(ui, |ui| {
                        if tree_heading(
                            ui,
                            "Favorites",
                            self.collapsed_groups.contains("favorites"),
                        )
                        .clicked()
                        {
                            toggle_group(&mut self.collapsed_groups, "favorites");
                        }
                        if !self.collapsed_groups.contains("favorites") {
                            for (folder, label, icon) in [
                                ("All Inboxes", "All Inboxes", "inbox"),
                                ("Unread", "Unread", "clock"),
                                ("Starred", "Flagged", "flag"),
                                ("Pinned", "Pinned", "pin"),
                            ] {
                                let count = self
                                    .mails
                                    .iter()
                                    .filter(|m| match folder {
                                        "All Inboxes" => m.folder == "Inbox" && m.unread,
                                        "Unread" => {
                                            m.unread && m.folder != "Trash" && m.folder != "Junk"
                                        }
                                        "Pinned" => m.pinned && m.folder != "Trash",
                                        _ => m.starred,
                                    })
                                    .count();
                                if navigation_row(
                                    ui,
                                    label,
                                    icon,
                                    count,
                                    self.folder == folder && self.account_filter.is_none(),
                                    self.dark,
                                )
                                .clicked()
                                {
                                    self.select_folder(folder, None);
                                }
                            }
                        }
                        ui.add_space(14.0);
                        let accounts: Vec<(Option<String>, String)> = if self.accounts.is_empty() {
                            vec![(None, "Personal account".into())]
                        } else {
                            self.accounts
                                .iter()
                                .map(|a| {
                                    (
                                        Some(a.id.clone()),
                                        if a.name.is_empty() {
                                            a.email.clone()
                                        } else {
                                            a.name.clone()
                                        },
                                    )
                                })
                                .collect()
                        };
                        for (account, name) in accounts {
                            let key = format!("account:{}", account.as_deref().unwrap_or("demo"));
                            if tree_heading(ui, &name, self.collapsed_groups.contains(&key))
                                .clicked()
                            {
                                toggle_group(&mut self.collapsed_groups, &key);
                            }
                            if self.collapsed_groups.contains(&key) {
                                continue;
                            }
                            let folders: std::collections::BTreeSet<String> = self
                                .folders
                                .iter()
                                .filter(|f| {
                                    !f.is_label
                                        && account.as_ref().is_none_or(|id| id == &f.account)
                                })
                                .map(|f| f.name.clone())
                                .filter(|f| !FOLDERS.contains(&f.as_str()))
                                .collect();
                            for folder in [
                                "Inbox", "Sent", "Drafts", "Archive", "Junk", "Trash", "All Mail",
                                "Snoozed",
                            ]
                            .into_iter()
                            .map(str::to_string)
                            .chain(folders)
                            {
                                let count = self
                                    .mails
                                    .iter()
                                    .filter(|m| {
                                        account.as_ref().is_none_or(|id| id == &m.account)
                                            && (if folder == "All Mail" {
                                                m.folder != "Trash" && m.folder != "Junk"
                                            } else {
                                                m.folder == folder
                                            })
                                            && (m.unread || folder == "Drafts")
                                    })
                                    .count();
                                let label = if folder == "Junk" {
                                    "Junk Email"
                                } else {
                                    &folder
                                };
                                if navigation_row(
                                    ui,
                                    label,
                                    folder_icon(&folder),
                                    count,
                                    self.folder == folder && self.account_filter == account,
                                    self.dark,
                                )
                                .clicked()
                                {
                                    self.select_folder(&folder, account.clone());
                                }
                            }
                            ui.add_space(12.0);
                        }
                        ui.add_space(4.0);
                        if tree_heading(ui, "Labels", self.collapsed_groups.contains("labels"))
                            .clicked()
                        {
                            toggle_group(&mut self.collapsed_groups, "labels");
                        }
                        if !self.collapsed_groups.contains("labels") {
                            let tags: std::collections::BTreeSet<_> = self
                                .mails
                                .iter()
                                .flat_map(|m| m.tags.iter().chain(&m.remote_labels))
                                .cloned()
                                .chain(
                                    self.folders
                                        .iter()
                                        .filter(|f| f.is_label)
                                        .map(|f| f.name.clone()),
                                )
                                .collect();
                            if tags.is_empty() {
                                theme::scope(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.add_space(34.0);
                                        ui.label(crate::i18n::widget(
                                            RichText::new(crate::i18n::display("No labels"))
                                                .size(12.0)
                                                .color(MUTED),
                                            ui.style(),
                                        ));
                                    });
                                });
                            }
                            for tag in tags {
                                let query = format!("label:{}", crate::search::quote(&tag));
                                if navigation_row(ui, &tag, "tag", 0, self.query == query, self.dark)
                                    .clicked()
                                {
                                    self.query = query;
                                    self.folder = "All Mail".into();
                                }
                            }
                        }
                    });
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.add_space(10.0);
                        let _ = tool_button(ui, "", "mail", false)
                            .on_hover_text(crate::i18n::display("Mail"));
                        if tool_button(ui, "", "contacts", false)
                            .on_hover_text(crate::i18n::display("Contacts"))
                            .clicked()
                        {
                            self.contact_window = true;
                        }
                        if tool_button(ui, "", "settings", false)
                            .on_hover_text(crate::i18n::display("Settings & accounts"))
                            .clicked()
                        {
                            self.settings = true;
                        }
                    });
                });
            })
        });
    }
    fn visible_in(&self, m: &Mail, folder: &str) -> bool {
        if self
            .account_filter
            .as_ref()
            .is_some_and(|a| a != &m.account)
        {
            return false;
        }
        let snoozed = m
            .snoozed_until
            .is_some_and(|t| t > chrono::Utc::now().timestamp());
        match folder {
            "All Inboxes" => m.folder == "Inbox" && !snoozed,
            "Unread" => m.unread && m.folder != "Trash" && m.folder != "Junk",
            "All Mail" => m.folder != "Trash" && m.folder != "Junk",
            "Pinned" => m.pinned && m.folder != "Trash",
            "Starred" => m.starred && m.folder != "Trash",
            "Snoozed" => snoozed,
            "Inbox" => m.folder == folder && !snoozed,
            _ => m.folder == folder || m.remote_labels.iter().any(|l| l == folder),
        }
    }
    fn mail_list(&mut self, root: &mut egui::Ui) {
        (if self.prefs.reading_below {
            egui::Panel::top("messages_below_v1")
        } else if self.prefs.language.rtl() {
            egui::Panel::right("messages_compact_v2_rtl")
        } else {
            egui::Panel::left("messages_compact_v2")
        })
        .default_size(if self.prefs.reading_below {
            240.0
        } else {
            (root.available_width() * 0.31).clamp(340.0, 540.0)
        })
        .size_range(if self.prefs.reading_below {
            160.0..=500.0
        } else {
            320.0..=650.0
        })
        .frame(
            egui::Frame::new()
                .fill(surface(self.dark))
                .inner_margin(0.0),
        )
        .show(root, |ui| {
            theme::scope(ui, |ui| {
                let mut messages: Vec<Mail> = self
                    .mails
                    .iter()
                    .filter(|m| {
                        self.visible_in(m, &self.folder)
                            && m.matches(&self.query)
                            && (!self.unread_only || m.unread)
                    })
                    .cloned()
                    .collect();
                messages
                    .sort_by_key(|m| (!m.pinned, if self.sort_oldest { m.date } else { -m.date }));
                self.thread_members.clear();
                if self.prefs.group_threads {
                    let groups = crate::threads::groups(&messages);
                    let mut grouped = Vec::new();
                    for indices in groups {
                        let ids: Vec<_> = indices.iter().map(|&i| messages[i].id.clone()).collect();
                        let mut mail = messages[indices[0]].clone();
                        let count = ids.len();
                        mail.date = indices
                            .iter()
                            .map(|&i| messages[i].date)
                            .max()
                            .unwrap_or(mail.date);
                        mail.unread = indices.iter().any(|&i| messages[i].unread);
                        mail.starred = indices.iter().any(|&i| messages[i].starred);
                        mail.pinned = indices.iter().any(|&i| messages[i].pinned);
                        if count > 1 {
                            mail.subject = format!("{} · {count}", mail.subject);
                        }
                        for id in &ids {
                            self.thread_members.insert(id.clone(), ids.clone());
                        }
                        grouped.push(mail);
                    }
                    messages = grouped;
                }
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(12, PANE_HEADER_MARGIN))
                    .show(ui, |ui| {
                        let row_layout = if ui.layout().prefer_right_to_left() {
                            egui::Layout::right_to_left(egui::Align::Center)
                        } else {
                            egui::Layout::left_to_right(egui::Align::Center)
                        };
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), PANE_HEADER_HEIGHT),
                            row_layout,
                            |ui| {
                            ui.label(crate::i18n::widget(
                                RichText::new(crate::i18n::display(&self.folder))
                                    .size(16.0)
                                    .strong(),
                                ui.style(),
                            ));
                            ui.label(
                                RichText::new(messages.len().to_string())
                                    .size(13.0)
                                    .color(MUTED),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    tool_menu(ui, "Filter", "filter", |ui| {
                                        if ui.add(action_button("Advanced search…")).clicked() {
                                            self.organize.advanced = true;
                                            ui.close();
                                        }
                                        ui.checkbox(
                                            &mut self.prefs.group_threads,
                                            crate::i18n::display("Group conversations"),
                                        );
                                        ui.checkbox(
                                            &mut self.unread_only,
                                            crate::i18n::display("Unread only"),
                                        );
                                        ui.checkbox(
                                            &mut self.sort_oldest,
                                            crate::i18n::display("Oldest first"),
                                        );
                                    });
                                    if tool_button(ui, "", "select", false)
                                        .on_hover_text(crate::i18n::display(
                                            "Select / clear all visible messages",
                                        ))
                                        .clicked()
                                    {
                                        if messages.iter().all(|m| {
                                            self.thread_members
                                                .get(&m.id)
                                                .map(|ids| {
                                                    ids.iter().all(|id| self.checked.contains(id))
                                                })
                                                .unwrap_or_else(|| self.checked.contains(&m.id))
                                        }) {
                                            self.checked.clear();
                                        } else {
                                            self.checked = messages
                                                .iter()
                                                .flat_map(|m| {
                                                    self.thread_members
                                                        .get(&m.id)
                                                        .cloned()
                                                        .unwrap_or_else(|| vec![m.id.clone()])
                                                })
                                                .collect();
                                        }
                                    }
                                },
                            );
                        });
                    });
                ui.separator();
                if !self.checked.is_empty() {
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.label(crate::i18n::widget(
                            RichText::new(format!("{} selected", self.checked.len()))
                                .size(12.0)
                                .color(BLUE),
                            ui.style(),
                        ));
                        if ui.add(action_button("Clear").small()).clicked() {
                            self.checked.clear();
                        }
                    });
                }
                egui::ScrollArea::vertical()
                    .id_salt("compact_mail_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if messages.is_empty() {
                            ui.add_space(60.0);
                            ui.vertical_centered(|ui| {
                                ui.label(crate::i18n::widget(
                                    RichText::new(crate::i18n::display("No messages"))
                                        .size(17.0)
                                        .color(MUTED),
                                    ui.style(),
                                ));
                            });
                        }
                        let mut previous = String::new();
                        for m in messages {
                            let group = if m.pinned {
                                crate::i18n::display("Pinned")
                            } else {
                                day_group(m.date)
                            };
                            if previous != group {
                                let key = format!(
                                    "date:{}:{}:{}",
                                    self.account_filter.as_deref().unwrap_or("all"),
                                    self.folder,
                                    group
                                );
                                if date_heading(ui, &group, self.collapsed_groups.contains(&key))
                                    .clicked()
                                {
                                    toggle_group(&mut self.collapsed_groups, &key);
                                }
                                previous = group.clone();
                            }
                            let key = format!(
                                "date:{}:{}:{}",
                                self.account_filter.as_deref().unwrap_or("all"),
                                self.folder,
                                group
                            );
                            if self.collapsed_groups.contains(&key) {
                                continue;
                            }
                            let response = message_row(
                                ui,
                                &m,
                                self.selected.as_ref().is_some_and(|id| {
                                    id == &m.id
                                        || self
                                            .thread_members
                                            .get(&m.id)
                                            .is_some_and(|ids| ids.contains(id))
                                }),
                                self.checked.contains(&m.id),
                                !self.checked.is_empty(),
                                self.dark,
                                self.prefs.compact_rows,
                            );
                            response.0.clone().context_menu(|ui| {
                                if ui
                                    .add(action_button(if m.pinned { "Unpin" } else { "Pin" }))
                                    .clicked()
                                {
                                    self.action_ids(
                                        "pin",
                                        self.thread_members
                                            .get(&m.id)
                                            .cloned()
                                            .unwrap_or_else(|| vec![m.id.clone()]),
                                    );
                                    ui.close();
                                }
                                self.provider_labels(ui, &m);
                                ui.menu_button(icon_label("Move to folder"), |ui| {
                                    for folder in self
                                        .folders
                                        .clone()
                                        .into_iter()
                                        .filter(|f| f.account == m.account && !f.is_label)
                                    {
                                        if ui.add(action_button(&folder.name)).clicked() {
                                            self.action_ids(
                                                &folder.path,
                                                self.thread_members
                                                    .get(&m.id)
                                                    .cloned()
                                                    .unwrap_or_else(|| vec![m.id.clone()]),
                                            );
                                            ui.close();
                                        }
                                    }
                                });
                                if ui.add(action_button("Reply")).clicked() {
                                    self.selected = Some(m.id.clone());
                                    self.start_quick_reply(&m.id, false);
                                    ui.close();
                                }
                                if ui.add(action_button("Reply all")).clicked() {
                                    self.selected = Some(m.id.clone());
                                    self.start_quick_reply(&m.id, true);
                                    ui.close();
                                }
                                if ui.add(action_button("Forward")).clicked() {
                                    self.selected = Some(m.id.clone());
                                    self.compose("forward");
                                    ui.close();
                                }
                                ui.separator();
                                for (label, action) in [
                                    ("Read / unread", "read"),
                                    ("Flag / unflag", "star"),
                                    ("Archive", "Archive"),
                                    ("Move to Trash", "Trash"),
                                ] {
                                    if ui.add(action_button(label)).clicked() {
                                        self.action_ids(
                                            action,
                                            self.thread_members
                                                .get(&m.id)
                                                .cloned()
                                                .unwrap_or_else(|| vec![m.id.clone()]),
                                        );
                                        ui.close();
                                    }
                                }
                                ui.separator();
                                if ui.add(action_button("Copy sender")).clicked() {
                                    ui.ctx().copy_text(m.from.clone());
                                    ui.close();
                                }
                                if ui.add(action_button("Save sender as contact…")).clicked() {
                                    self.selected = Some(m.id.clone());
                                    self.contact_from_sender();
                                    ui.close();
                                }
                            });
                            if response.2 {
                                self.action_ids(
                                    "star",
                                    self.thread_members
                                        .get(&m.id)
                                        .cloned()
                                        .unwrap_or_else(|| vec![m.id.clone()]),
                                );
                            } else if response.1 {
                                let ids = self
                                    .thread_members
                                    .get(&m.id)
                                    .cloned()
                                    .unwrap_or_else(|| vec![m.id.clone()]);
                                let remove = ids.iter().all(|id| self.checked.contains(id));
                                for id in ids {
                                    if remove {
                                        self.checked.remove(&id);
                                    } else {
                                        self.checked.insert(id);
                                    }
                                }
                            } else if response.0.clicked() {
                                self.remember_search();
                                self.search_open = false;
                                self.selected = Some(m.id.clone());
                                if m.unread && self.prefs.mark_read_on_open {
                                    self.action_ids(
                                        "read",
                                        self.thread_members
                                            .get(&m.id)
                                            .cloned()
                                            .unwrap_or_else(|| vec![m.id.clone()]),
                                    );
                                }
                                if m.folder == "Drafts" {
                                    self.compose("draft");
                                }
                            }
                        }
                    });
            })
        });
    }
    fn message_toolbar(&mut self, ui: &mut egui::Ui) {
        let selected = self
            .selected
            .as_ref()
            .is_some_and(|id| self.mails.iter().any(|m| &m.id == id));
        let can_edit = self.selected.as_ref().is_some_and(|id| {
            self.mails
                .iter()
                .any(|m| &m.id == id && m.folder == "Drafts")
        });
        let can_export = self
            .selected
            .as_ref()
            .is_some_and(|id| self.mails.iter().any(|m| &m.id == id && !m.raw.is_empty()));
        let targets = self
            .ids()
            .iter()
            .any(|id| self.mails.iter().any(|m| &m.id == id));
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(10, PANE_HEADER_MARGIN))
            .show(ui, |ui| {
                ui.set_min_height(PANE_HEADER_HEIGHT);
                ui.horizontal_wrapped(|ui| {
                    ui.add_enabled_ui(targets, |ui| {
                        ui.add_enabled_ui(selected, |ui| {
                            if tool_button(ui, "Reply", "reply", false).clicked()
                                && let Some(id) = self.selected.clone()
                            {
                                self.start_quick_reply(&id, false);
                            }
                            if tool_button(ui, "Reply all", "reply_all", false).clicked()
                                && let Some(id) = self.selected.clone()
                            {
                                self.start_quick_reply(&id, true);
                            }
                            if tool_button(ui, "Forward", "forward", false).clicked() {
                                self.compose("forward");
                            }
                        });
                        ui.separator();
                        tool_menu(ui, "Mark", "flag", |ui| {
                            if ui.add(action_button("Read / unread")).clicked() {
                                self.action("read");
                                ui.close();
                            }
                            if ui.add(action_button("Flag / unflag")).clicked() {
                                self.action("star");
                                ui.close();
                            }
                            if ui.add(action_button("Junk email")).clicked() {
                                self.action("Junk");
                                ui.close();
                            }
                        });
                        if tool_button(ui, "Archive", "archive", false).clicked() {
                            self.action("Archive");
                        }
                        tool_menu(ui, "Snooze", "clock", |ui| {
                            for (label, hours) in [
                                ("In one hour", 1),
                                ("Tomorrow", 24),
                                ("Next week", 168),
                                ("Unsnooze", 0),
                            ] {
                                if ui.add(action_button(label)).clicked() {
                                    for id in self.ids() {
                                        if let Some(m) = self.mails.iter_mut().find(|m| m.id == id)
                                        {
                                            m.snoozed_until = if hours == 0 {
                                                None
                                            } else {
                                                Some(chrono::Utc::now().timestamp() + hours * 3600)
                                            };
                                            let m = m.clone();
                                            self.persist(&m);
                                        }
                                    }
                                    ui.close();
                                }
                            }
                        });
                        if tool_button(ui, "Delete", "trash", false).clicked() {
                            self.action("Trash");
                        }
                        tool_menu(ui, "More", "more", |ui| {
                            if ui.add(action_button("Advanced search…")).clicked() {
                                self.organize.advanced = true;
                                ui.close();
                            }
                            if ui.add(action_button("Pin / unpin")).clicked() {
                                self.action("pin");
                                ui.close();
                            }
                            if ui
                                .add_enabled(selected, action_button("Save sender as contact…"))
                                .clicked()
                            {
                                self.contact_from_sender();
                                ui.close();
                            }
                            ui.separator();
                            if ui
                                .add_enabled(can_export, action_button("Export original (.eml)"))
                                .clicked()
                            {
                                self.export();
                                ui.close();
                            }
                            if ui
                                .add_enabled(can_edit, action_button("Edit draft"))
                                .clicked()
                            {
                                self.compose("draft");
                                ui.close();
                            }
                            ui.menu_button(icon_label("Move to"), |ui| {
                                let folders: std::collections::BTreeSet<String> = self
                                    .folders
                                    .iter()
                                    .filter(|f| !f.is_label)
                                    .map(|f| f.name.clone())
                                    .chain(
                                        ["Inbox", "Archive", "Sent", "Junk", "Trash"]
                                            .map(str::to_string),
                                    )
                                    .collect();
                                for f in folders {
                                    if ui.add(action_button(&f)).clicked() {
                                        self.action(&f);
                                        ui.close();
                                    }
                                }
                            });
                        });
                    });
                });
            });
        ui.separator();
    }
    fn reader(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(surface(self.dark))
                    .inner_margin(0.0),
            )
            .show(root, |ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    theme::scope(ui, |ui| self.message_toolbar(ui));
                    let Some(m) = self
                        .mails
                        .iter()
                        .find(|m| Some(&m.id) == self.selected.as_ref())
                        .cloned()
                    else {
                        ui.add_space(100.0);
                        ui.vertical_centered(|ui| {
                            ui.label(crate::i18n::widget(
                                RichText::new(crate::i18n::display("Select a message to read"))
                                    .size(18.0)
                                    .color(MUTED),
                                ui.style(),
                            ));
                        });
                        return;
                    };
                    if let Some(ids) = self
                        .thread_members
                        .get(&m.id)
                        .cloned()
                        .filter(|ids| ids.len() > 1)
                    {
                        egui::CollapsingHeader::new(format!(
                            "Conversation · {} messages",
                            ids.len()
                        ))
                        .id_salt("thread_members")
                        .show(ui, |ui| {
                            for id in ids {
                                if let Some(mail) = self.mails.iter().find(|mail| mail.id == id)
                                    && ui
                                        .selectable_label(
                                            mail.id == m.id,
                                            format!(
                                                "{} · {} · {}",
                                                sender(&mail.from),
                                                date(mail.date, "%b %d %H:%M"),
                                                mail.subject
                                            ),
                                        )
                                        .clicked()
                                {
                                    self.selected = Some(id);
                                }
                            }
                        });
                    }
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(18, 13))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&m.subject).font(bold_font(17.0)),
                                    )
                                    .truncate(),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(crate::i18n::widget(
                                            RichText::new(format!(" {} ", m.folder))
                                                .size(12.0)
                                                .color(MUTED)
                                                .background_color(if self.dark {
                                                    Color32::from_gray(45)
                                                } else {
                                                    Color32::from_gray(238)
                                                }),
                                            ui.style(),
                                        ));
                                    },
                                );
                            });
                        });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("reader_scroll_v2")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            egui::Frame::new()
                                .inner_margin(egui::Margin::symmetric(20, 18))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        avatar(ui, &m.from, 38.0);
                                        ui.vertical(|ui| {
                                            // The name and the address are drawn apart: in a line that
                                            // starts in Arabic, egui would lay "<address>" out right to
                                            // left too ("<gro.elpmaxe@amlas>").
                                            let (name, address) = match m.from.rsplit_once(" <") {
                                                Some((n, a)) if a.ends_with('>') => (n.trim(), Some(format!("<{a}"))),
                                                _ => (m.from.as_str(), None),
                                            };
                                            ui.horizontal(|ui| {
                                                ui.spacing_mut().item_spacing.x = 4.0;
                                                ui.add(egui::Label::new(RichText::new(name).size(14.0).color(BLUE)).truncate());
                                                if let Some(a) = address {
                                                    ui.add(egui::Label::new(RichText::new(a).size(14.0).color(BLUE)).truncate());
                                                }
                                            });
                                            ui.label(crate::i18n::widget(
                                                RichText::new(format!("To: {}", m.to))
                                                    .size(12.0)
                                                    .color(MUTED),
                                                ui.style(),
                                            ));
                                            if !m.cc.is_empty() {
                                                ui.label(crate::i18n::widget(
                                                    RichText::new(format!("Cc: {}", m.cc))
                                                        .size(12.0)
                                                        .color(MUTED),
                                                    ui.style(),
                                                ));
                                            }
                                        });
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::TOP),
                                            |ui| {
                                                ui.label(crate::i18n::widget(
                                                    RichText::new(date(m.date, "%b %d, %H:%M"))
                                                        .size(12.0)
                                                        .color(MUTED),
                                                    ui.style(),
                                                ));
                                            },
                                        );
                                    });
                                    ui.add_space(8.0);
                                    ui.horizontal(|ui| {
                                        for tag in m.tags.iter().chain(&m.remote_labels) {
                                            ui.label(crate::i18n::widget(
                                                RichText::new(tag).size(11.0).color(MUTED),
                                                ui.style(),
                                            ));
                                        }
                                        tool_menu(ui, "Label", "tag", |ui| {
                                            let known: std::collections::BTreeSet<String> = self
                                                .mails
                                                .iter()
                                                .flat_map(|x| x.tags.iter().cloned())
                                                .chain(
                                                    self.folders
                                                        .iter()
                                                        .filter(|f| f.is_label)
                                                        .map(|f| f.name.clone()),
                                                )
                                                .collect();
                                            let mut toggle = None;
                                            for tag in &known {
                                                let mut on = m.tags.contains(tag);
                                                if ui.checkbox(&mut on, tag.as_str()).changed() {
                                                    toggle = Some((tag.clone(), on));
                                                }
                                            }
                                            if !known.is_empty() {
                                                ui.separator();
                                            }
                                            let field = ui.add(
                                                egui::TextEdit::singleline(&mut self.tag_input)
                                                    .hint_text(crate::i18n::display("New label")),
                                            );
                                            let submit = field.lost_focus()
                                                && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                            if (ui
                                                .add_enabled(
                                                    !self.tag_input.trim().is_empty(),
                                                    action_button("Add label"),
                                                )
                                                .clicked()
                                                || submit)
                                                && !self.tag_input.trim().is_empty()
                                            {
                                                toggle = Some((self.tag_input.trim().to_string(), true));
                                                self.tag_input.clear();
                                            }
                                            if let Some((tag, on)) = toggle
                                                && let Some(mail) =
                                                    self.mails.iter_mut().find(|x| x.id == m.id)
                                            {
                                                mail.tags.retain(|t| t != &tag);
                                                if on {
                                                    mail.tags.push(tag);
                                                }
                                                let mail = mail.clone();
                                                self.persist(&mail);
                                            }
                                        });
                                    });
                                });
                            ui.separator();
                            egui::Frame::new()
                                .inner_margin(egui::Margin::symmetric(24, 22))
                                .show(ui, |ui| {
                                    egui::Frame::new()
                                        .fill(if self.prefs.email_body_dark {
                                            surface(self.dark)
                                        } else {
                                            Color32::WHITE
                                        })
                                        .inner_margin(16.0)
                                        .show(ui, |ui| {
                                            ui.set_min_width(ui.available_width());
                                            ui.set_min_height(160.0);
                                            if !self.prefs.email_body_dark {
                                                *ui.visuals_mut() = theme::visuals(false);
                                            }
                                            self.rich_message(ui, &m);
                                        });
                                    if !m.attachments.is_empty() {
                                        ui.add_space(18.0);
                                        ui.separator();
                                        ui.label(crate::i18n::widget(
                                            RichText::new(format!(
                                                "Attachments ({})",
                                                m.attachments.len()
                                            ))
                                            .strong(),
                                            ui.style(),
                                        ));
                                        for a in &m.attachments {
                                            if ui
                                                .add(action_button(format!(
                                                    "{} · {} KB   Save",
                                                    a.name,
                                                    a.data.len().div_ceil(1024)
                                                )))
                                                .clicked()
                                                && let Some(path) = rfd::FileDialog::new()
                                                    .set_file_name(
                                                        std::path::Path::new(&a.name)
                                                            .file_name()
                                                            .unwrap_or_default()
                                                            .to_string_lossy(),
                                                    )
                                                    .save_file()
                                            {
                                                self.status = match std::fs::write(path, &a.data) {
                                                    Ok(()) => "Attachment saved".into(),
                                                    Err(e) => e.to_string(),
                                                };
                                            }
                                        }
                                    }
                                    ui.add_space(24.0);
                                    self.quick_reply_ui(ui, &m);
                                });
                        });
                })
            });
    }
    fn workspace_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let modal_open = self.settings
            || self.contact_window
            || self.shortcuts
            || self.dialogs.about
            || self.organize.advanced
            || self.organize.folders;
        ui.add_enabled_ui(!modal_open, |ui| {
            // Block input beneath dialogs without fading it: the modal
            // backdrop already dims the workspace.
            ui.visuals_mut().disabled_alpha = 1.0;
            if self.prefs.language.rtl() {
                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                    self.main_menu(ui);
                });
            } else {
                self.main_menu(ui);
            }
            self.toolbar(ui);
            self.sidebar(ui);
            self.mail_list(ui);
            self.reader(ui);
        });
        if !modal_open {
            self.compose_windows(&ctx);
        }
        if self.organize.advanced {
            self.advanced_search(&ctx);
        } else if self.organize.folders {
            self.folder_manager(&ctx);
        } else if self.dialogs.about {
            self.about_window(&ctx);
        } else if self.shortcuts {
            self.shortcuts_window(&ctx);
        } else if self.contact_window {
            self.contacts_window(&ctx);
        } else if self.settings {
            self.settings_window(&ctx);
        }
    }
    fn export(&mut self) {
        if let Some(m) = self
            .mails
            .iter()
            .find(|m| Some(&m.id) == self.selected.as_ref())
            && let Some(path) = rfd::FileDialog::new()
                .set_file_name("message.eml")
                .save_file()
        {
            if m.raw.is_empty() {
                self.status = "Original source is unavailable for this local message".into();
            } else {
                self.status = match std::fs::write(path, &m.raw) {
                    Ok(()) => "Message exported".into(),
                    Err(e) => e.to_string(),
                };
            }
        }
    }
}
impl eframe::App for Barid {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        crate::i18n::set_language(self.prefs.language);
        self.background(ctx);
        #[cfg(target_os = "macos")]
        self.handle_native_menu(ctx);
    }
    fn auto_save_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(5)
    }
    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        for draft in self
            .compose_sessions
            .iter()
            .map(|s| s.draft.clone())
            .filter(|d| !d.is_empty())
            .collect::<Vec<_>>()
        {
            if let Err(e) = self.save_local_draft(&draft) {
                self.status = e;
            }
        }
        if let Some(d) = self.composer.clone().filter(|draft| !draft.is_empty()) {
            let m = d.as_mail();
            self.persist(&m);
            self.mails.retain(|old| old.id != m.id);
            self.mails.push(m);
        }
        if let Some(draft) = self
            .dialogs
            .quick_reply
            .as_ref()
            .map(|r| r.draft.clone())
            .filter(|d| !d.is_empty())
            && let Err(e) = self.save_local_draft(&draft)
        {
            self.status = e;
        }
        if let Err(e) = self.store.save("preferences", "default", &self.prefs) {
            self.status = e;
        }
        if let Err(e) = self.store.save("appearance", "dark", &self.dark) {
            self.status = e;
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        crate::i18n::set_language(self.prefs.language);
        #[cfg(target_os = "macos")]
        self.handle_native_menu(&ctx);
        self.events();
        if self.prefs.sync_minutes > 0
            && self.busy == 0
            && self.accounts.iter().any(|a| a.enabled)
            && self.last_sync.elapsed().as_secs() >= u64::from(self.prefs.sync_minutes) * 60
        {
            self.sync();
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(30));
        if self.refresh_pending && self.busy == 0 {
            self.refresh_pending = false;
            self.sync();
        }
        theme::install(&ctx, 1.0);
        ctx.set_zoom_factor(self.prefs.text_scale.clamp(0.85, 1.3));
        // Follow the app's appearance setting, not the OS theme: the root `ui`
        // is created from the OS-theme style before this runs.
        ctx.set_theme(if self.dark {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        });
        ui.set_style(ctx.global_style());
        if self.busy > 0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if !self.organize.advanced
            && !self.organize.folders
            && !self.setup.open
            && !self.settings
            && !self.contact_window
            && !self.shortcuts
            && !self.dialogs.about
        {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::N)) {
                self.compose("new");
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
                self.focus_search = true;
            }
            for (key, mark) in [
                (egui::Key::B, crate::rich_text::Mark::Bold),
                (egui::Key::I, crate::rich_text::Mark::Italic),
                (egui::Key::U, crate::rich_text::Mark::Underline),
            ] {
                if !self.active_editor.is_empty()
                    && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, key))
                {
                    self.pending_editor = Some(editor::EditorCommand::Mark(mark));
                }
            }
        }
        if self.composer.is_none()
            && !self.organize.advanced
            && !self.organize.folders
            && !self.setup.open
            && !self.settings
            && !self.contact_window
            && !self.shortcuts
            && !ctx.egui_wants_keyboard_input()
        {
            if ctx.input(|i| i.key_pressed(egui::Key::R))
                && let Some(id) = self.selected.clone()
            {
                self.start_quick_reply(&id, false);
            }
            if ctx.input(|i| i.key_pressed(egui::Key::E)) {
                self.action("Archive");
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Delete)) {
                self.action("Trash");
            }
        }
        egui::Panel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(if self.dark {
                        Color32::from_gray(22)
                    } else {
                        BG
                    })
                    .inner_margin(8.0),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if self.busy > 0 {
                        ui.spinner();
                    } else {
                        ui.label(crate::i18n::widget(
                            RichText::new(crate::i18n::display("●"))
                                .color(BLUE)
                                .size(8.0),
                            ui.style(),
                        ));
                    }
                    ui.label(crate::i18n::widget(
                        RichText::new(crate::i18n::display(&self.status))
                            .size(11.0)
                            .color(MUTED),
                        ui.style(),
                    ));
                });
            });
        if self.setup.open {
            self.setup_screen(ui);
            if self.dialogs.about {
                self.about_window(&ctx);
            }
        } else {
            self.workspace_ui(ui);
        }
    }
}
fn sender(from: &str) -> String {
    from.split('<')
        .next()
        .unwrap_or(from)
        .trim()
        .trim_matches('"')
        .to_string()
}
fn date(timestamp: i64, format: &str) -> String {
    chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|d| d.with_timezone(&chrono::Local).format(format).to_string())
        .unwrap_or_default()
}
fn avatar(ui: &mut egui::Ui, name: &str, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let initials: String = sender(name)
        .split_whitespace()
        .take(2)
        .filter_map(|s| s.chars().next())
        .collect();
    let hash: usize = name.bytes().map(|b| b as usize).sum();
    let colors = [
        Color32::from_rgb(222, 231, 212),
        Color32::from_rgb(238, 225, 207),
        Color32::from_rgb(224, 221, 236),
        Color32::from_rgb(215, 230, 233),
    ];
    ui.painter()
        .circle_filled(rect.center(), size / 2.0, colors[hash % colors.len()]);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        initials,
        egui::FontId::proportional(size * 0.32),
        BLUE,
    );
}

fn surface(dark: bool) -> Color32 {
    theme::palette(dark).surface
}
fn toggle_group(groups: &mut HashSet<String>, key: &str) {
    if !groups.remove(key) {
        groups.insert(key.to_owned());
    }
}
fn tree_heading(ui: &mut egui::Ui, name: &str, collapsed: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::click());
    draw_icon(
        ui.painter(),
        if crate::i18n::language().rtl() {
            rect.right_center() - egui::vec2(17.0, 0.0)
        } else {
            rect.left_center() + egui::vec2(17.0, 0.0)
        },
        if collapsed { "forward" } else { "chevron" },
        MUTED,
        0.65,
    );
    ui.painter().text(
        if crate::i18n::language().rtl() {
            rect.right_center() - egui::vec2(32.0, 0.0)
        } else {
            rect.left_center() + egui::vec2(32.0, 0.0)
        },
        if crate::i18n::language().rtl() {
            egui::Align2::RIGHT_CENTER
        } else {
            egui::Align2::LEFT_CENTER
        },
        crate::i18n::display(name),
        egui::FontId::proportional(13.0),
        MUTED,
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::Role::Button,
            true,
            !collapsed,
            crate::i18n::display(name),
        )
    });
    response
}
fn folder_icon(folder: &str) -> &str {
    match folder {
        "Inbox" | "All Inboxes" => "inbox",
        "Sent" => "send",
        "Drafts" => "draft",
        "Archive" => "archive",
        "Trash" => "trash",
        "Snoozed" | "Unread" => "clock",
        "Starred" => "flag",
        "Junk" => "junk",
        _ => "folder",
    }
}
fn navigation_row(
    ui: &mut egui::Ui,
    label: &str,
    icon: &str,
    count: usize,
    selected: bool,
    dark: bool,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 32.0), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, selected, label));
    let p = ui.painter();
    let rtl = crate::i18n::language().rtl();
    if selected || response.hovered() {
        p.rect_filled(
            rect,
            0.0,
            if selected {
                if dark {
                    Color32::from_rgb(39, 62, 91)
                } else {
                    Color32::from_rgb(219, 234, 253)
                }
            } else {
                if dark {
                    Color32::from_gray(36)
                } else {
                    Color32::from_gray(246)
                }
            },
        );
    }
    draw_icon(
        p,
        if rtl {
            rect.right_center() - egui::vec2(43.0, 0.0)
        } else {
            rect.left_center() + egui::vec2(43.0, 0.0)
        },
        icon,
        MUTED,
        1.0,
    );
    p.text(
        if rtl {
            rect.right_center() - egui::vec2(63.0, 0.0)
        } else {
            rect.left_center() + egui::vec2(63.0, 0.0)
        },
        if rtl {
            egui::Align2::RIGHT_CENTER
        } else {
            egui::Align2::LEFT_CENTER
        },
        crate::i18n::display(label),
        egui::FontId::proportional(14.0),
        ui.visuals().text_color(),
    );
    if count > 0 {
        p.text(
            if rtl {
                rect.left_center() + egui::vec2(14.0, 0.0)
            } else {
                rect.right_center() - egui::vec2(14.0, 0.0)
            },
            if rtl {
                egui::Align2::LEFT_CENTER
            } else {
                egui::Align2::RIGHT_CENTER
            },
            count.to_string(),
            egui::FontId::proportional(13.0),
            BLUE,
        );
    }
    response
}
fn tool_button(ui: &mut egui::Ui, text: &str, icon: &str, primary: bool) -> egui::Response {
    let color = if !ui.is_enabled() {
        ui.visuals().weak_text_color()
    } else if primary {
        Color32::WHITE
    } else {
        ui.visuals().text_color()
    };
    let font = egui::FontId::proportional(13.0);
    let job = crate::i18n::widget(
        RichText::new(format!(
            "{}{}",
            crate::i18n::display(text.trim()),
            if text.ends_with("   ") { "   " } else { "" }
        ))
        .font(font)
        .color(color),
        ui.style(),
    )
    .into_layout_job(ui.style(), egui::FontSelection::Default, egui::Align::Min);
    let galley = ui.painter().layout_job((*job).clone());
    let width = if text.is_empty() {
        32.0
    } else {
        galley.size().x + 40.0
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 32.0), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            ui.is_enabled(),
            if text.is_empty() { icon } else { text },
        )
    });
    let p = ui.painter();
    if primary || response.hovered() || response.has_focus() {
        p.rect_filled(
            rect,
            6.0,
            if primary {
                if !ui.is_enabled() {
                    Color32::from_gray(200)
                } else if response.hovered() {
                    Color32::from_rgb(237, 140, 0)
                } else {
                    ACCENT
                }
            } else {
                ui.visuals().widgets.hovered.weak_bg_fill
            },
        );
    }
    draw_icon(
        p,
        if crate::i18n::language().rtl() {
            rect.right_center() - egui::vec2(16.0, 0.0)
        } else {
            rect.left_center() + egui::vec2(16.0, 0.0)
        },
        icon,
        color,
        1.0,
    );
    if !text.is_empty() {
        p.galley(
            egui::pos2(
                if crate::i18n::language().rtl() {
                    rect.right() - 31.0 - galley.size().x
                } else {
                    rect.left() + 31.0
                },
                rect.top() + (32.0 - galley.size().y) / 2.0,
            ),
            galley,
            ui.visuals().text_color(),
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
fn tool_menu(ui: &mut egui::Ui, text: &str, icon: &str, content: impl FnOnce(&mut egui::Ui)) {
    ui.push_id(("tool_menu", text), |ui| {
        let response = tool_button(ui, &format!("{text}   "), icon, false);
        draw_icon(
            ui.painter(),
            if crate::i18n::language().rtl() {
                response.rect.left_center() + egui::vec2(7.0, 0.0)
            } else {
                response.rect.right_center() - egui::vec2(7.0, 0.0)
            },
            "chevron",
            ui.visuals().text_color(),
            0.55,
        );
        let popup_id = ui.make_persistent_id(("tool_popup", ui.layer_id()));
        #[cfg(test)]
        if ui
            .ctx()
            .data(|d| d.get_temp::<String>(egui::Id::unique("test_forced_tool_menu")))
            .as_deref()
            == Some(text)
        {
            egui::Popup::open_id(ui.ctx(), popup_id);
        }
        egui::Popup::menu(&response)
            .id(popup_id)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .width(240.0)
            .layout(egui::Layout::top_down_justified(
                if crate::i18n::language().rtl() {
                    egui::Align::Max
                } else {
                    egui::Align::Min
                },
            ))
            .show(|ui| {
                #[cfg(test)]
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::unique("test_opened_tool_menu"), text.to_owned())
                });
                ui.set_min_width(180.0);
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 5.0);
                ui.set_max_width((ui.ctx().viewport_rect().width() - 32.0).clamp(180.0, 480.0));
                content(ui);
            });
    });
}
fn day_group(timestamp: i64) -> String {
    let day = chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|d| d.with_timezone(&chrono::Local).date_naive())
        .unwrap_or_default();
    let today = chrono::Local::now().date_naive();
    match (today - day).num_days() {
        0 => "Today".into(),
        1 => "Yesterday".into(),
        2..=6 => day.format("%A").to_string(),
        _ => day.format("%B %d, %Y").to_string(),
    }
}
fn date_heading(ui: &mut egui::Ui, text: &str, collapsed: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::click());
    let p = ui.painter();
    draw_icon(
        p,
        if crate::i18n::language().rtl() {
            rect.right_center() - egui::vec2(12.0, 0.0)
        } else {
            rect.left_center() + egui::vec2(12.0, 0.0)
        },
        if collapsed { "forward" } else { "chevron" },
        MUTED,
        0.65,
    );
    p.text(
        if crate::i18n::language().rtl() {
            rect.right_center() - egui::vec2(27.0, 0.0)
        } else {
            rect.left_center() + egui::vec2(27.0, 0.0)
        },
        if crate::i18n::language().rtl() {
            egui::Align2::RIGHT_CENTER
        } else {
            egui::Align2::LEFT_CENTER
        },
        crate::i18n::display(text),
        egui::FontId::proportional(13.0),
        MUTED,
    );
    p.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0, theme::line(ui.visuals().dark_mode)),
    );
    response
}
fn row_text(
    p: &egui::Painter,
    pos: egui::Pos2,
    text: &str,
    width: f32,
    size: f32,
    color: Color32,
    bold: bool,
) {
    let mut job = egui::text::LayoutJob::simple(
        crate::i18n::display(text),
        if bold {
            bold_font(size)
        } else {
            egui::FontId::proportional(size)
        },
        color,
        width.max(20.0),
    );
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let job = crate::i18n::widget(job, &egui::Style::default()).into_layout_job(
        &egui::Style::default(),
        egui::FontSelection::Default,
        egui::Align::Min,
    );
    let galley = p.layout_job((*job).clone());
    let pos = if crate::i18n::language().rtl() {
        egui::pos2(pos.x + width - galley.size().x, pos.y)
    } else {
        pos
    };
    p.galley(pos, galley, color);
}
fn message_row(
    ui: &mut egui::Ui,
    m: &Mail,
    selected: bool,
    checked: bool,
    selecting: bool,
    dark: bool,
    compact: bool,
) -> (egui::Response, bool, bool) {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), if compact { 74.0 } else { 88.0 }),
        egui::Sense::click(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::Role::ListBoxOption,
            true,
            selected,
            format!("{}: {}", sender(&m.from), m.subject),
        )
    });
    let p = ui.painter().with_clip_rect(rect);
    if selected || response.hovered() {
        p.rect_filled(
            rect,
            0.0,
            if selected {
                if dark {
                    Color32::from_gray(48)
                } else {
                    Color32::from_gray(234)
                }
            } else {
                if dark {
                    Color32::from_gray(34)
                } else {
                    Color32::from_gray(249)
                }
            },
        );
    }
    let check_rect = egui::Rect::from_center_size(
        if crate::i18n::language().rtl() {
            rect.right_center() - egui::vec2(13.0, 0.0)
        } else {
            rect.left_center() + egui::vec2(13.0, 0.0)
        },
        Vec2::splat(14.0),
    );
    if selecting || response.hovered() {
        p.rect_stroke(
            check_rect,
            2.0,
            Stroke::new(1.0, if checked { BLUE } else { MUTED }),
            egui::StrokeKind::Inside,
        );
        if checked {
            p.line_segment(
                [
                    check_rect.left_center() + egui::vec2(3.0, 0.0),
                    check_rect.center() + egui::vec2(0.0, 3.0),
                ],
                Stroke::new(1.5, BLUE),
            );
            p.line_segment(
                [
                    check_rect.center() + egui::vec2(0.0, 3.0),
                    check_rect.right_top() + egui::vec2(-2.0, 3.0),
                ],
                Stroke::new(1.5, BLUE),
            );
        }
    } else if m.unread {
        p.circle_filled(
            if crate::i18n::language().rtl() {
                rect.right_center() - egui::vec2(13.0, 0.0)
            } else {
                rect.left_center() + egui::vec2(13.0, 0.0)
            },
            3.5,
            ACCENT,
        );
    }
    let avatar_rect = egui::Rect::from_center_size(
        if crate::i18n::language().rtl() {
            rect.right_top() + egui::vec2(-43.0, 32.0)
        } else {
            rect.left_top() + egui::vec2(43.0, 32.0)
        },
        Vec2::splat(30.0),
    );
    let hash: usize = m.from.bytes().map(usize::from).sum();
    let colors = [
        Color32::from_rgb(235, 151, 50),
        Color32::from_rgb(72, 136, 200),
        Color32::from_rgb(146, 118, 167),
        Color32::from_rgb(78, 151, 143),
    ];
    p.circle_stroke(
        avatar_rect.center(),
        14.0,
        Stroke::new(1.5, colors[hash % colors.len()]),
    );
    p.text(
        avatar_rect.center(),
        egui::Align2::CENTER_CENTER,
        sender(&m.from)
            .chars()
            .next()
            .unwrap_or('?')
            .to_uppercase()
            .to_string(),
        egui::FontId::proportional(17.0),
        colors[hash % colors.len()],
    );
    let x = rect.min.x + 67.0;
    let width = rect.width() - 80.0;
    let color = if dark { Color32::from_gray(225) } else { INK };
    row_text(
        &p,
        egui::pos2(
            if crate::i18n::language().rtl() {
                rect.left() + 91.0
            } else {
                x
            },
            rect.min.y + 8.0,
        ),
        &sender(&m.from),
        width - 78.0,
        13.0,
        color,
        m.unread,
    );
    p.text(
        if crate::i18n::language().rtl() {
            rect.left_top() + egui::vec2(12.0, 9.0)
        } else {
            rect.right_top() + egui::vec2(-12.0, 9.0)
        },
        if crate::i18n::language().rtl() {
            egui::Align2::LEFT_TOP
        } else {
            egui::Align2::RIGHT_TOP
        },
        date(m.date, "%H:%M"),
        egui::FontId::proportional(11.0),
        MUTED,
    );
    row_text(
        &p,
        egui::pos2(
            if crate::i18n::language().rtl() {
                rect.left() + 31.0
            } else {
                x
            },
            rect.min.y + 27.0,
        ),
        &m.subject,
        width - 18.0,
        13.0,
        if m.unread {
            color
        } else {
            Color32::from_gray(105)
        },
        m.unread,
    );
    let preview = format!(
        "{}  {}",
        m.folder,
        m.body
            .lines()
            .filter(|s| !s.trim().is_empty())
            .take(2)
            .collect::<Vec<_>>()
            .join(" ")
    );
    row_text(
        &p,
        egui::pos2(
            if crate::i18n::language().rtl() {
                rect.left() + 13.0
            } else {
                x
            },
            rect.min.y + 47.0,
        ),
        &preview,
        width,
        12.0,
        MUTED,
        false,
    );
    if m.pinned {
        draw_icon(
            &p,
            if crate::i18n::language().rtl() {
                rect.left_top() + egui::vec2(32.0, 34.0)
            } else {
                rect.right_top() + egui::vec2(-32.0, 34.0)
            },
            "pin",
            BLUE,
            0.75,
        );
    }
    if m.starred || response.hovered() {
        draw_icon(
            &p,
            if crate::i18n::language().rtl() {
                rect.left_top() + egui::vec2(14.0, 34.0)
            } else {
                rect.right_top() + egui::vec2(-14.0, 34.0)
            },
            "flag",
            if m.starred { ACCENT } else { MUTED },
            0.7,
        );
    }
    let toggled = response.clicked()
        && response.interact_pointer_pos().is_some_and(|pos| {
            if crate::i18n::language().rtl() {
                pos.x > rect.max.x - 26.0
            } else {
                pos.x < rect.min.x + 26.0
            }
        });
    let flagged = response.clicked()
        && response.interact_pointer_pos().is_some_and(|pos| {
            egui::Rect::from_center_size(
                if crate::i18n::language().rtl() {
                    rect.left_top() + egui::vec2(14.0, 34.0)
                } else {
                    rect.right_top() + egui::vec2(-14.0, 34.0)
                },
                Vec2::splat(22.0),
            )
            .contains(pos)
        });
    (response, toggled, flagged)
}
/// Phosphor Regular glyph for each icon name used in the interface.
fn icon_glyph(kind: &str) -> &'static str {
    match kind {
        "align_center" => "\u{E480}",
        "align_left" => "\u{E484}",
        "align_right" => "\u{E486}",
        "archive" => "\u{E00C}",
        "attach" => "\u{E39A}",
        "bell" => "\u{E0CE}",
        "bold" => "\u{E5BE}",
        "chevron" => "\u{E136}",
        "clock" => "\u{E19A}",
        "close" => "\u{E4F6}",
        "collapse" => "\u{E09E}",
        "contacts" => "\u{E4C2}",
        "copy" => "\u{E1CA}",
        "cut" => "\u{EAE0}",
        "density" => "\u{E5A2}",
        "download" => "\u{E20C}",
        "draft" => "\u{E230}",
        "edit" => "\u{E3B4}",
        "eraser" => "\u{E21E}",
        "expand" => "\u{E0A6}",
        "eye" => "\u{E220}",
        "filter" => "\u{E268}",
        "flag" => "\u{E244}",
        "folder" => "\u{E24A}",
        "forward" => "\u{E026}",
        "help" => "\u{E3E8}",
        "image" => "\u{E2CA}",
        "inbox" => "\u{E4AA}",
        "indent" => "\u{EA1E}",
        "italic" => "\u{E5C0}",
        "junk" => "\u{E4E4}",
        "link" => "\u{E2E6}",
        "list" => "\u{E2F2}",
        "mail" => "\u{E218}",
        "menu" => "\u{E2F0}",
        "minimize" => "\u{E32A}",
        "moon" => "\u{E330}",
        "more" => "\u{E1FE}",
        "numbered" => "\u{E2F6}",
        "outdent" => "\u{EA1C}",
        "palette" => "\u{E6C8}",
        "paste" => "\u{E198}",
        "pin" => "\u{E3E2}",
        "plus" => "\u{E3D4}",
        "power" => "\u{E3DA}",
        "redo" => "\u{E036}",
        "refresh" => "\u{E094}",
        "replace" => "\u{E0A0}",
        "reply" => "\u{E024}",
        "reply_all" => "\u{E03A}",
        "restore" => "\u{E13C}",
        "save" => "\u{E248}",
        "search" => "\u{E30C}",
        "select" => "\u{E186}",
        "send" => "\u{E398}",
        "sent" => "\u{E396}",
        "settings" => "\u{E272}",
        "star" => "\u{E46A}",
        "strike" => "\u{E5C2}",
        "sun" => "\u{E472}",
        "tag" => "\u{E478}",
        "text" => "\u{E6EE}",
        "trash" => "\u{E4A6}",
        "underline" => "\u{E5C4}",
        "undo" => "\u{E038}",
        "upload" => "\u{E4C0}",
        _ => "\u{E1FE}",
    }
}
fn draw_icon(p: &egui::Painter, c: egui::Pos2, kind: &str, color: Color32, scale: f32) {
    p.text(
        c,
        egui::Align2::CENTER_CENTER,
        icon_glyph(kind),
        egui::FontId::new(18.0 * scale, egui::FontFamily::Name("icons".into())),
        color,
    );
}

fn bold_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name("barid-bold".into()))
}
fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for path in [
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "C:/Windows/Fonts/segoeui.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert("barid-ui".into(), egui::FontData::from_owned(bytes).into());
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "barid-ui".into());
            break;
        }
    }

    let mut family = fonts.families[&egui::FontFamily::Proportional].clone();
    for path in [
        "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
        "C:/Windows/Fonts/segoeuib.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(
                "barid-bold-face".into(),
                egui::FontData::from_owned(bytes).into(),
            );
            family.insert(0, "barid-bold-face".into());
            break;
        }
    }
    for path in [
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "C:/Windows/Fonts/arial.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(
                "barid-arabic".into(),
                egui::FontData::from_owned(bytes).into(),
            );
            for names in fonts.families.values_mut() {
                names.push("barid-arabic".into());
            }
            break;
        }
    }
    fonts
        .families
        .insert(egui::FontFamily::Name("barid-bold".into()), family);
    let mut serif = fonts.families[&egui::FontFamily::Proportional].clone();
    for path in [
        "/System/Library/Fonts/Supplemental/Georgia.ttf",
        "C:/Windows/Fonts/georgia.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(
                "barid-serif-face".into(),
                egui::FontData::from_owned(bytes).into(),
            );
            serif.insert(0, "barid-serif-face".into());
            break;
        }
    }
    fonts
        .families
        .insert(egui::FontFamily::Name("barid-serif".into()), serif);
    for (name, paths, fallback) in [
        (
            "barid-serif-bold",
            [
                "/System/Library/Fonts/Supplemental/Georgia Bold.ttf",
                "C:/Windows/Fonts/georgiab.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSerif-Bold.ttf",
            ],
            egui::FontFamily::Name("barid-serif".into()),
        ),
        (
            "barid-mono-bold",
            [
                "/System/Library/Fonts/Supplemental/Courier New Bold.ttf",
                "C:/Windows/Fonts/courbd.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf",
            ],
            egui::FontFamily::Monospace,
        ),
    ] {
        let mut family = fonts.families[&fallback].clone();
        for path in paths {
            if let Ok(bytes) = std::fs::read(path) {
                fonts
                    .font_data
                    .insert(name.into(), egui::FontData::from_owned(bytes).into());
                family.insert(0, name.into());
                break;
            }
        }
        fonts
            .families
            .insert(egui::FontFamily::Name(name.into()), family);
    }
    // Phosphor icons (MIT) — see assets/fonts/PHOSPHOR-LICENSE.
    fonts.font_data.insert(
        "phosphor".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Phosphor.ttf")).into(),
    );
    fonts
        .families
        .insert(egui::FontFamily::Name("icons".into()), vec!["phosphor".into()]);
    ctx.set_fonts(fonts);
}
#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> Barid {
        Barid::with_store(Store::open(std::path::Path::new(":memory:")).unwrap())
    }
    #[test]
    fn opening_another_composer_preserves_the_previous_draft() {
        let mut app = app();
        app.composer = Some(Draft {
            id: "recover-me".into(),
            bcc: "private@example.com".into(),
            in_reply_to: "<parent@example.com>".into(),
            body: "Unsaved text".into(),
            ..Default::default()
        });
        app.compose("new");
        let saved = app.store.mails().unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].body, "Unsaved text");
        assert_eq!(saved[0].bcc, "private@example.com");
        app.selected = Some("recover-me".into());
        app.compose("draft");
        assert_eq!(
            app.composer.as_ref().unwrap().in_reply_to,
            "<parent@example.com>"
        );
    }
    #[test]
    fn reader_and_dialogs_render_at_supported_window_sizes() {
        let mut app = app();
        let ctx = egui::Context::default();
        configure_fonts(&ctx);
        for size in [egui::vec2(1040.0, 680.0), egui::vec2(1910.0, 940.0)] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| {
                    app.toolbar(ui);
                    app.sidebar(ui);
                    app.mail_list(ui);
                    app.reader(ui);
                },
            );
            assert!(!output.shapes.is_empty());
            output.textures_delta.clear();
        }
        app.settings = true;
        app.contact_window = true;
        app.compose("new");
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {
            app.settings_window(&ctx);
            app.contacts_window(&ctx);
            app.compose_window(&ctx);
        });
        assert!(!output.shapes.is_empty());
        output.textures_delta.clear();
    }
    #[test]
    fn partial_sync_keeps_failed_folder_in_memory_and_reports_the_warning() {
        let mut app = app();
        app.mails = vec![
            Mail {
                id: "cached-archive".into(),
                account: "a".into(),
                uid: Some(1),
                remote_folder: "Archive".into(),
                folder: "Archive".into(),
                ..Default::default()
            },
            Mail {
                id: "deleted-inbox".into(),
                account: "a".into(),
                uid: Some(2),
                remote_folder: "INBOX".into(),
                folder: "Inbox".into(),
                ..Default::default()
            },
        ];
        for mail in &app.mails {
            app.store.save("mail", &mail.id, mail).unwrap();
        }
        app.tx
            .send(Event::Synced(
                "a".into(),
                Ok(SyncBatch {
                    failed_folders: vec!["Archive".into()],
                    warnings: vec!["Archive: Connection Lost".into()],
                    ..Default::default()
                }),
            ))
            .unwrap();
        app.events();
        assert_eq!(app.mails.len(), 1);
        assert_eq!(app.mails[0].id, "cached-archive");
        assert_eq!(app.store.mails().unwrap().len(), 1);
        assert!(app.status.contains("cached mail kept"));
        assert!(app.account_status["a"].contains("Archive"));
    }
    #[test]
    fn conversation_actions_include_members_and_use_one_read_and_flag_state() {
        let mut app = app();
        app.mails = vec![
            Mail {
                id: "one".into(),
                account: "a".into(),
                folder: "Inbox".into(),
                message_id: "<root>".into(),
                unread: false,
                pinned: true,
                ..Default::default()
            },
            Mail {
                id: "two".into(),
                account: "a".into(),
                folder: "Inbox".into(),
                in_reply_to: "<root>".into(),
                unread: true,
                ..Default::default()
            },
        ];
        app.selected = Some("one".into());
        app.prefs.group_threads = true;
        let ctx = egui::Context::default();
        configure_fonts(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.mail_list(ui));
        output.textures_delta.clear();
        let ids = app.ids();
        assert_eq!(ids.len(), 2);
        app.action_ids("pin", ids.clone());
        assert!(app.mails.iter().all(|m| !m.pinned));
        app.action_ids("read", ids.clone());
        assert!(app.mails.iter().all(|m| !m.unread));
        app.action_ids("star", ids);
        assert!(app.mails.iter().all(|m| m.starred));
    }
    #[test]
    fn contact_refresh_preserves_local_metadata_and_locally_edited_copies() {
        let mut app = app();
        app.contacts = vec![Contact {
            id: "one".into(),
            source_account: "a".into(),
            source_id: "remote".into(),
            name: "Old name".into(),
            email: "one@example.com".into(),
            favorite: true,
            group: "Work".into(),
            notes: "Local note".into(),
            ..Default::default()
        }];
        let incoming = Contact {
            id: "remote-generated".into(),
            source_account: "a".into(),
            source_id: "remote".into(),
            name: "Updated name".into(),
            email: "one@example.com".into(),
            ..Default::default()
        };
        app.tx
            .send(Event::Contacts("a".into(), Ok(vec![incoming.clone()])))
            .unwrap();
        app.events();
        assert_eq!(app.contacts.len(), 1);
        assert_eq!(app.contacts[0].name, "Updated name");
        assert!(app.contacts[0].favorite);
        assert_eq!(app.contacts[0].notes, "Local note");
        assert_eq!(app.contacts[0].group, "Work");
        app.contacts[0].local_override = true;
        app.contacts[0].name = "My edited name".into();
        app.tx
            .send(Event::Contacts("a".into(), Ok(vec![incoming])))
            .unwrap();
        app.events();
        assert_eq!(app.contacts[0].name, "My edited name");
    }
    #[test]
    fn organization_dialogs_and_stacked_reading_layout_render_at_minimum_size() {
        let mut app = app();
        let ctx = egui::Context::default();
        configure_fonts(&ctx);
        for view in 0..5 {
            app.organize.advanced = view == 0;
            app.organize.folders = view == 1;
            app.settings = view == 2 || view == 3;
            app.dialogs.section = if view == 2 { 4 } else { 5 };
            app.prefs.reading_below = view == 4;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1040.0, 680.0),
                    )),
                    ..Default::default()
                },
                |ui| app.workspace_ui(ui),
            );
            assert!(!output.shapes.is_empty());
            output.textures_delta.clear();
        }
    }
    #[test]
    fn failed_sync_preserves_the_in_memory_mail_cache() {
        let mut app = app();
        let count = app.mails.len();
        app.busy = 1;
        app.tx
            .send(Event::Synced("a".into(), Err("offline".into())))
            .unwrap();
        app.events();
        assert_eq!(app.mails.len(), count);
        assert_eq!(app.busy, 0);
        assert!(app.status.contains("offline"));
    }
}
