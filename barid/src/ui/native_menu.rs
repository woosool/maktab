//! macOS AppKit menu. Constructed and updated only on eframe's main thread.
use super::menus::NativeAction;
use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{CheckMenuItem, IconMenuItem, Menu, MenuEvent, NativeIcon, PredefinedMenuItem, Submenu};
use std::{collections::HashMap, sync::mpsc};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct State {
    pub language: crate::i18n::Language,
    pub modal: bool,
    pub editor: bool,
    pub format: bool,
    pub message: bool,
    pub compose: bool,
    pub send: bool,
    pub busy: bool,
    pub dark: bool,
    pub unread: bool,
    pub compact: bool,
    pub windows: Vec<(String, String)>,
    pub accounts: Vec<(String, String)>,
}
#[derive(Clone, Copy)]
enum Group {
    Always,
    Ui,
    Editor,
    Format,
    Message,
    Compose,
    Send,
    Refresh,
}
pub(super) struct NativeMenu {
    _root: Menu,
    rx: mpsc::Receiver<String>,
    actions: HashMap<String, NativeAction>,
    items: Vec<(IconMenuItem, Group, String)>,
    menus: Vec<(Submenu, String)>,
    dark: CheckMenuItem,
    unread: CheckMenuItem,
    compact: CheckMenuItem,
    windows: Submenu,
    accounts: Submenu,
    last: Option<State>,
}
fn accel(code: Code) -> Option<Accelerator> {
    Some(Accelerator::new(Modifiers::META, code))
}
fn native_icon(icon: &str) -> NativeIcon {
    match icon {
        "plus" => NativeIcon::Add,
        "settings" => NativeIcon::PreferencesGeneral,
        "help" => NativeIcon::Info,
        "save" => NativeIcon::MultipleDocuments,
        "send" | "forward" => NativeIcon::GoRight,
        "reply" | "reply_all" | "undo" => NativeIcon::GoLeft,
        "redo" => NativeIcon::GoRight,
        "copy" => NativeIcon::MultipleDocuments,
        "cut" | "close" => NativeIcon::Remove,
        "paste" => NativeIcon::IconView,
        "search" => NativeIcon::QuickLook,
        "refresh" => NativeIcon::Refresh,
        "inbox" | "archive" => NativeIcon::Folder,
        "contacts" => NativeIcon::UserGroup,
        "flag" => NativeIcon::Bookmarks,
        "trash" => NativeIcon::TrashEmpty,
        "image" => NativeIcon::Slideshow,
        "expand" => NativeIcon::EnterFullScreen,
        "restore" => NativeIcon::ExitFullScreen,
        "minimize" => NativeIcon::MenuMixedState,
        "link" => NativeIcon::FollowLinkFreestanding,
        "select" => NativeIcon::MenuOnState,
        "text" | "bold" | "italic" | "underline" => NativeIcon::FontPanel,
        "palette" => NativeIcon::ColorPanel,
        "power" => NativeIcon::StopProgress,
        _ => NativeIcon::ListView,
    }
}
impl NativeMenu {
    pub fn new(ctx: &eframe::egui::Context) -> Result<Self, muda::Error> {
        let root = Menu::new();
        let app = Submenu::new("Barid", true);
        let file = Submenu::new("File", true);
        let edit = Submenu::new("Edit", true);
        let view = Submenu::new("View", true);
        let mailbox = Submenu::new("Mailbox", true);
        let message = Submenu::new("Message", true);
        let format = Submenu::new("Format", true);
        let window = Submenu::new("Window", true);
        let help = Submenu::new("Help", true);
        root.append_items(&[
            &app, &file, &edit, &view, &mailbox, &message, &format, &window, &help,
        ])?;
        let windows = Submenu::new("Compose Windows", true);
        let accounts = Submenu::new("Open Inbox", true);
        let dark = CheckMenuItem::with_id("dark", "Dark Appearance", true, false, None);
        let unread = CheckMenuItem::with_id("unread", "Unread Messages Only", true, false, None);
        let compact = CheckMenuItem::with_id("compact", "Compact Message Rows", true, true, None);
        let (tx, rx) = mpsc::channel();
        let repaint = ctx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let _ = tx.send(event.id.0);
            repaint.request_repaint();
        }));
        let mut menu = Self {
            _root: root,
            rx,
            actions: HashMap::new(),
            items: Vec::new(),
            menus: [
                &app, &file, &edit, &view, &mailbox, &message, &format, &window, &help,
            ]
            .into_iter()
            .zip([
                "Barid", "File", "Edit", "View", "Mailbox", "Message", "Format", "Window", "Help",
            ])
            .map(|(menu, title)| (menu.clone(), title.into()))
            .collect(),
            dark,
            unread,
            compact,
            windows,
            accounts,
            last: None,
        };
        menu.actions
            .insert("tray_show".into(), NativeAction::RestoreWindow);
        menu.actions.insert("tray_new".into(), NativeAction::New);
        menu.actions
            .insert("tray_sync".into(), NativeAction::Refresh);
        menu.actions.insert("tray_quit".into(), NativeAction::Quit);
        menu.item(
            &app,
            "about",
            "About Barid",
            "help",
            NativeAction::About,
            Group::Always,
            None,
        )?;
        app.append(&PredefinedMenuItem::separator())?;
        menu.item(
            &app,
            "settings",
            "Settings…",
            "settings",
            NativeAction::Settings,
            Group::Ui,
            accel(Code::Comma),
        )?;
        app.append_items(&[
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(None),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
        ])?;
        menu.item(
            &app,
            "quit",
            "Quit Barid",
            "power",
            NativeAction::Quit,
            Group::Always,
            accel(Code::KeyQ),
        )?;
        menu.item(
            &file,
            "add_account",
            "Add an email account",
            "plus",
            NativeAction::AddAccount,
            Group::Ui,
            None,
        )?;
        menu.item(
            &file,
            "new",
            "New Message",
            "plus",
            NativeAction::New,
            Group::Ui,
            accel(Code::KeyN),
        )?;
        menu.item(
            &file,
            "save",
            "Save All Drafts",
            "save",
            NativeAction::Save,
            Group::Compose,
            accel(Code::KeyS),
        )?;
        menu.item(
            &file,
            "send",
            "Send Active Message",
            "send",
            NativeAction::Send,
            Group::Send,
            Some(Accelerator::new(Modifiers::META, Code::Enter)),
        )?;
        file.append(&PredefinedMenuItem::separator())?;
        menu.item(
            &file,
            "close",
            "Close Draft / Window",
            "close",
            NativeAction::Close,
            Group::Ui,
            accel(Code::KeyW),
        )?;
        for (id, label, icon, action, key) in [
            ("undo", "Undo", "undo", NativeAction::Undo, Some(Code::KeyZ)),
            ("redo", "Redo", "redo", NativeAction::Redo, None),
            ("cut", "Cut", "cut", NativeAction::Cut, Some(Code::KeyX)),
            ("copy", "Copy", "copy", NativeAction::Copy, Some(Code::KeyC)),
            (
                "paste",
                "Paste",
                "paste",
                NativeAction::Paste,
                Some(Code::KeyV),
            ),
            (
                "paste_plain",
                "Paste and Match Style",
                "text",
                NativeAction::PastePlain,
                None,
            ),
            (
                "select_all",
                "Select All",
                "select",
                NativeAction::SelectAll,
                Some(Code::KeyA),
            ),
        ] {
            let shortcut = if id == "redo" {
                Some(Accelerator::new(
                    Modifiers::META | Modifiers::SHIFT,
                    Code::KeyZ,
                ))
            } else if id == "paste_plain" {
                Some(Accelerator::new(
                    Modifiers::META | Modifiers::SHIFT,
                    Code::KeyV,
                ))
            } else {
                key.and_then(accel)
            };
            menu.item(&edit, id, label, icon, action, Group::Editor, shortcut)?;
        }
        edit.append(&PredefinedMenuItem::separator())?;
        menu.item(
            &edit,
            "find",
            "Find Mail",
            "search",
            NativeAction::Find,
            Group::Ui,
            accel(Code::KeyF),
        )?;
        edit.append(&PredefinedMenuItem::emoji_and_symbols(None))?;
        menu.item(
            &view,
            "unified",
            "All Inboxes",
            "inbox",
            NativeAction::Unified,
            Group::Ui,
            None,
        )?;
        view.append_items(&[
            &menu.unread,
            &menu.compact,
            &PredefinedMenuItem::separator(),
            &menu.dark,
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::fullscreen(None),
        ])?;
        menu.actions.insert("dark".into(), NativeAction::Dark);
        menu.actions.insert("unread".into(), NativeAction::Unread);
        menu.actions.insert("compact".into(), NativeAction::Compact);
        menu.item(
            &mailbox,
            "refresh",
            "Get New Mail",
            "refresh",
            NativeAction::Refresh,
            Group::Refresh,
            None,
        )?;
        mailbox.append(&menu.accounts)?;
        for (folder, id) in [
            ("Inbox", "folder_inbox"),
            ("Sent", "folder_sent"),
            ("Drafts", "folder_drafts"),
            ("Archive", "folder_archive"),
            ("Junk", "folder_junk"),
            ("Trash", "folder_trash"),
        ] {
            menu.item(
                &mailbox,
                id,
                folder,
                super::folder_icon(folder),
                NativeAction::Folder(folder.into()),
                Group::Ui,
                None,
            )?;
        }
        mailbox.append(&PredefinedMenuItem::separator())?;
        menu.item(
            &mailbox,
            "accounts",
            "Manage Accounts…",
            "settings",
            NativeAction::Accounts,
            Group::Ui,
            None,
        )?;
        menu.item(
            &mailbox,
            "contacts",
            "Contact Manager…",
            "contacts",
            NativeAction::Contacts,
            Group::Ui,
            None,
        )?;
        for (id, label, icon, action) in [
            ("reply", "Reply", "reply", NativeAction::Reply(false)),
            (
                "reply_all",
                "Reply All",
                "reply_all",
                NativeAction::Reply(true),
            ),
            ("forward", "Forward", "forward", NativeAction::Forward),
            (
                "read",
                "Read / Unread",
                "mail",
                NativeAction::Message("read"),
            ),
            (
                "flag",
                "Flag / Unflag",
                "flag",
                NativeAction::Message("star"),
            ),
            (
                "archive",
                "Archive",
                "archive",
                NativeAction::Message("Archive"),
            ),
            (
                "trash",
                "Move to Trash",
                "trash",
                NativeAction::Message("Trash"),
            ),
            (
                "junk",
                "Mark as Junk",
                "junk",
                NativeAction::Message("Junk"),
            ),
            (
                "sender",
                "Save Sender as Contact…",
                "contacts",
                NativeAction::Sender,
            ),
            (
                "export",
                "Export Original Message…",
                "save",
                NativeAction::Export,
            ),
        ] {
            menu.item(&message, id, label, icon, action, Group::Message, None)?;
        }
        for (id, label, icon, action, key) in [
            ("bold", "Bold", "bold", NativeAction::Bold, Some(Code::KeyB)),
            (
                "italic",
                "Italic",
                "italic",
                NativeAction::Italic,
                Some(Code::KeyI),
            ),
            (
                "underline",
                "Underline",
                "underline",
                NativeAction::Underline,
                Some(Code::KeyU),
            ),
            (
                "strike",
                "Strikethrough",
                "text",
                NativeAction::Strike,
                None,
            ),
            (
                "clear",
                "Clear Formatting",
                "text",
                NativeAction::Clear,
                None,
            ),
            ("image", "Insert Image…", "image", NativeAction::Image, None),
            (
                "bullet",
                "Bulleted List",
                "text",
                NativeAction::Bullet,
                None,
            ),
            (
                "numbered",
                "Numbered List",
                "text",
                NativeAction::Numbered,
                None,
            ),
            (
                "left",
                "Align Left",
                "text",
                NativeAction::Align(crate::rich_text::Alignment::Left),
                None,
            ),
        ] {
            menu.item(
                &format,
                id,
                label,
                icon,
                action,
                Group::Format,
                key.and_then(accel),
            )?;
        }
        menu.item(
            &format,
            "center",
            "Align Center",
            "text",
            NativeAction::Align(crate::rich_text::Alignment::Center),
            Group::Format,
            None,
        )?;
        menu.item(
            &format,
            "right",
            "Align Right",
            "text",
            NativeAction::Align(crate::rich_text::Alignment::Right),
            Group::Format,
            None,
        )?;
        window.append_items(&[
            &PredefinedMenuItem::minimize(None),
            &PredefinedMenuItem::zoom(None),
            &PredefinedMenuItem::bring_all_to_front(None),
            &PredefinedMenuItem::separator(),
        ])?;
        menu.item(
            &window,
            "min_compose",
            "Minimize All Composers",
            "minimize",
            NativeAction::Minimize(true),
            Group::Compose,
            None,
        )?;
        menu.item(
            &window,
            "restore_compose",
            "Restore All Composers",
            "restore",
            NativeAction::Minimize(false),
            Group::Compose,
            None,
        )?;
        window.append(&menu.windows)?;
        menu.item(
            &help,
            "shortcuts",
            "Keyboard Shortcuts and Search",
            "help",
            NativeAction::Shortcuts,
            Group::Ui,
            None,
        )?;
        menu._root.init_for_nsapp();
        window.set_as_windows_menu_for_nsapp();
        help.set_as_help_menu_for_nsapp();
        Ok(menu)
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Native menu entries keep action, icon and shortcut together"
    )]
    fn item(
        &mut self,
        parent: &Submenu,
        id: &str,
        label: &str,
        icon: &str,
        action: NativeAction,
        group: Group,
        accelerator: Option<Accelerator>,
    ) -> Result<(), muda::Error> {
        let item = IconMenuItem::with_id_and_native_icon(
            id,
            label,
            true,
            Some(native_icon(icon)),
            accelerator,
        );
        parent.append(&item)?;
        self.actions.insert(id.into(), action);
        self.items.push((item, group, label.into()));
        Ok(())
    }
    pub fn update(&mut self, state: State) -> Result<(), muda::Error> {
        if self.last.as_ref() == Some(&state) {
            return Ok(());
        }
        for (item, group, label) in &self.items {
            item.set_text(crate::i18n::tr(label));
            let enabled = match group {
                Group::Always => true,
                Group::Ui => !state.modal,
                Group::Editor => state.editor,
                Group::Format => !state.modal && state.format,
                Group::Message => !state.modal && state.message,
                Group::Compose => !state.modal && state.compose,
                Group::Send => !state.modal && state.send,
                Group::Refresh => !state.modal && !state.busy,
            };
            if item.is_enabled() != enabled {
                item.set_enabled(enabled);
            }
        }
        for (menu, title) in &self.menus {
            menu.set_text(crate::i18n::tr(title));
        }
        self.windows.set_text(crate::i18n::tr("Compose Windows"));
        self.accounts.set_text(crate::i18n::tr("Open Inbox"));
        self.dark.set_text(crate::i18n::tr("Dark Appearance"));
        self.unread
            .set_text(crate::i18n::tr("Unread Messages Only"));
        self.compact
            .set_text(crate::i18n::tr("Compact Message Rows"));
        self.dark.set_checked(state.dark);
        self.unread.set_checked(state.unread);
        self.compact.set_checked(state.compact);
        for item in [&self.dark, &self.unread, &self.compact] {
            item.set_enabled(!state.modal);
        }
        if self
            .last
            .as_ref()
            .is_none_or(|old| old.windows != state.windows)
        {
            while !self.windows.items().is_empty() {
                self.windows.remove_at(0);
            }
            self.actions.retain(|id, _| !id.starts_with("compose:"));
            for (id, title) in &state.windows {
                let menu_id = format!("compose:{id}");
                self.windows.append(&IconMenuItem::with_id_and_native_icon(
                    &menu_id,
                    title,
                    true,
                    Some(NativeIcon::MultipleDocuments),
                    None,
                ))?;
                self.actions
                    .insert(menu_id, NativeAction::FocusCompose(id.clone()));
            }
        }
        if self
            .last
            .as_ref()
            .is_none_or(|old| old.accounts != state.accounts)
        {
            while !self.accounts.items().is_empty() {
                self.accounts.remove_at(0);
            }
            self.actions.retain(|id, _| !id.starts_with("inbox:"));
            for (id, title) in &state.accounts {
                let menu_id = format!("inbox:{id}");
                self.accounts
                    .append(&IconMenuItem::with_id_and_native_icon(
                        &menu_id,
                        title,
                        true,
                        Some(NativeIcon::Folder),
                        None,
                    ))?;
                self.actions
                    .insert(menu_id, NativeAction::Account(id.clone()));
            }
        }
        self.windows
            .set_enabled(!state.modal && !state.windows.is_empty());
        self.accounts
            .set_enabled(!state.modal && !state.accounts.is_empty());
        self.last = Some(state);
        Ok(())
    }
    pub fn drain(&self) -> Vec<NativeAction> {
        self.rx
            .try_iter()
            .filter_map(|id| self.actions.get(&id).cloned())
            .collect()
    }
}
