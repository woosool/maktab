use super::*;
use crate::i18n::{self, Language, display};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
#[derive(Default)]
pub(super) struct Setup {
    pub open: bool,
    step: u8,
    provider: usize,
    account: Account,
    secret: String,
    client_secret: String,
    error: String,
    pending: Option<mpsc::Receiver<Result<Account, String>>>,
    cancel: Arc<AtomicBool>,
    first_run: bool,
}
fn text(ui: &mut egui::Ui, label: &str) {
    ui.label(crate::i18n::widget(display(label), ui.style()));
}
fn input(ui: &mut egui::Ui, label: &str, value: &mut String, password: bool) {
    text(ui, label);
    ui.add(
        egui::TextEdit::singleline(value)
            .desired_width(f32::INFINITY)
            .password(password),
    );
    ui.add_space(8.0);
}
fn language_picker(ui: &mut egui::Ui, language: &mut Language) {
    crate::ui::theme::named_combo("Language", egui::ComboBox::from_id_salt("interface_language")
        .selected_text(match language {
            Language::English => "English".into(),
            Language::French => "Français".into(),
            Language::Arabic => i18n::visual("العربية"),
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(language, Language::English, "English");
            ui.selectable_value(language, Language::French, "Français");
            ui.selectable_value(language, Language::Arabic, i18n::visual("العربية"));
        }));
}
impl Barid {
    pub(super) fn reconnect_account(&mut self, account: Account) {
        self.setup = Setup {
            open: true,
            step: 2,
            provider: if account.connection == Connection::Gmail {
                0
            } else {
                1
            },
            account,
            ..Default::default()
        };
        self.settings = false;
    }
    pub(super) fn open_setup(&mut self) {
        self.setup.cancel.store(true, Ordering::Relaxed);
        self.setup = Setup {
            open: true,
            step: 1,
            first_run: false,
            ..Default::default()
        };
        self.settings = false;
    }
    pub(super) fn first_run_setup(&mut self) {
        if self.accounts.is_empty() && !self.prefs.setup_complete {
            self.setup = Setup {
                open: true,
                first_run: true,
                ..Default::default()
            };
        }
    }
    pub(super) fn language_control(&mut self, ui: &mut egui::Ui) {
        let previous = self.prefs.language;
        language_picker(ui, &mut self.prefs.language);
        if previous != self.prefs.language {
            i18n::set_language(self.prefs.language);
            if let Err(e) = self.store.save("preferences", "default", &self.prefs) {
                self.status = e;
            }
            ui.ctx().request_repaint();
        }
    }
    pub(super) fn setup_screen(&mut self, root: &mut egui::Ui) {
        let previous = serde_json::to_string(&self.prefs).unwrap_or_default();
        let mut setup = std::mem::take(&mut self.setup);
        let ctx = root.ctx().clone();
        if let Some(rx) = &setup.pending {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            if let Ok(result) = rx.try_recv() {
                setup.pending = None;
                match result {
                    Ok(account) => {
                        setup.account = account;
                        setup.step = 3;
                        setup.secret.clear();
                        setup.client_secret.clear();
                    }
                    Err(e) => setup.error = e,
                }
            }
        }
        let rtl = self.prefs.language.rtl();
        let mut close = false;
        let mut finish = false;
        egui::CentralPanel::default().frame(egui::Frame::new().fill(surface(self.dark)).inner_margin(24.0)).show(root,|ui|{
   ui.horizontal(|ui|{ui.label(crate::i18n::widget(RichText::new(display("Barid by Woosool")).size(19.0).strong(), ui.style()));ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{language_picker(ui,&mut self.prefs.language);});});
   ui.separator();
   egui::ScrollArea::vertical().show(ui,|ui|{
    let width=560.0_f32.min(ui.available_width());let margin=((ui.available_width()-width)/2.0).max(0.0);
    ui.horizontal(|ui|{ui.add_space(margin);ui.allocate_ui_with_layout(egui::vec2(width,0.0),egui::Layout::top_down(if rtl{egui::Align::Max}else{egui::Align::Min}),|ui|{
     ui.set_width(width);ui.add_space(28.0);
     let title=match setup.step{0=>"Welcome to Barid",1=>"Choose your provider",2=>"Connect securely",_=>"Account connected"};
     ui.label(crate::i18n::widget(RichText::new(display(title)).size(28.0).strong(), ui.style()));ui.add_space(8.0);text(ui,"Your email, all in one place.");ui.add_space(24.0);
     if setup.step==0{
      text(ui,"Set up your email");ui.add_space(12.0);
      if theme::primary_button(ui,[width,44.0],"Continue").clicked(){setup.step=1;}
      if ui.add(action_button("Use preview workspace")).clicked(){close=true;}
     }else if setup.step==1{
      for (index,(name,detail)) in [("Gmail","Gmail API · Google OAuth"),("Outlook / Microsoft 365","Microsoft Graph · Microsoft OAuth"),("Woosool","IMAP / POP3 · SMTP"),("IMAP / SMTP","IMAP keeps folders and message states synchronized."),("POP3 / SMTP","POP3 downloads your inbox and leaves messages on the server. Folders and read states are local.")].iter().enumerate(){
       if ui.add_sized([width,48.0],action_button(*name)).clicked(){
        setup.provider=index;setup.account=Account{id:uuid::Uuid::new_v4().to_string(),provider:(match index{0=>"Google",1=>"Microsoft",2=>"Woosool",_=>"Other"}).into(),connection:match index{0=>Connection::Gmail,1=>Connection::Outlook,4=>Connection::Pop3,_=>Connection::Imap},imap_port:if index==4{995}else{993},..Default::default()};
        if index==0{setup.account.oauth_client_id=std::env::var("BARID_GOOGLE_CLIENT_ID").unwrap_or_default();setup.client_secret=std::env::var("BARID_GOOGLE_CLIENT_SECRET").unwrap_or_default();}
        if index==1{setup.account.oauth_client_id=std::env::var("BARID_MICROSOFT_CLIENT_ID").unwrap_or_default();}
        setup.error.clear();setup.step=2;
       }
       ui.label(crate::i18n::widget(RichText::new(display(detail)).size(12.0).color(MUTED), ui.style()));ui.add_space(10.0);
      }
      if ui.add(action_button(if setup.first_run{"Use preview workspace"}else{"Cancel"})).clicked(){close=true;}
     }else if setup.step==2{
      ui.add_enabled_ui(setup.pending.is_none(),|ui|{
       input(ui,"Account name",&mut setup.account.name,false);
       if setup.account.connection.api(){
        text(ui,"Your browser opens for sign-in. Barid never receives your provider password.");ui.add_space(10.0);
        egui::CollapsingHeader::new(display("OAuth application settings")).default_open(setup.account.oauth_client_id.is_empty()).show(ui,|ui|{
         input(ui,"Client ID",&mut setup.account.oauth_client_id,false);
         if setup.account.connection==Connection::Gmail{input(ui,"Desktop client secret (Google)",&mut setup.client_secret,true);}
         text(ui,"Register a desktop OAuth app and use a loopback redirect URI.");
         if ui.add(action_button("Help")).clicked(){let _=webbrowser::open(if setup.account.connection==Connection::Gmail{"https://developers.google.com/identity/protocols/oauth2/native-app"}else{"https://learn.microsoft.com/en-us/entra/identity-platform/quickstart-register-app"});}
        });
       }else{
        input(ui,"Email address",&mut setup.account.email,false);
        if setup.provider==2{text(ui,"Copy server settings from Woosool Barid → Settings → Security → Mail app server settings.");
         text(ui,"Use your full email address and an Email app password, not your Woosool Account password.");
         ui.horizontal_wrapped(|ui|{
          if ui.add(action_button("Woosool connection help")).clicked(){let _=webbrowser::open("https://support.woosool.com/en/mail/mail-apps");}
          if ui.add(action_button("Create an app password")).clicked(){let _=webbrowser::open("https://support.woosool.com/en/mail/app-passwords");}
         });}
        let previous=setup.account.connection;
        ui.horizontal(|ui|{text(ui,"Incoming protocol");ui.selectable_value(&mut setup.account.connection,Connection::Imap,"IMAP");ui.selectable_value(&mut setup.account.connection,Connection::Pop3,"POP3");});
        if previous!=setup.account.connection{setup.account.imap_port=if setup.account.connection==Connection::Pop3{995}else{993};}
        input(ui,"Incoming server",&mut setup.account.imap_host,false);
        ui.horizontal(|ui|{text(ui,"Incoming TLS port");ui.add(egui::DragValue::new(&mut setup.account.imap_port).range(1..=65535));});
        input(ui,"SMTP server",&mut setup.account.smtp_host,false);
        ui.horizontal(|ui|{text(ui,"SMTP TLS / STARTTLS port");ui.add(egui::DragValue::new(&mut setup.account.smtp_port).range(1..=65535));});
        input(ui,if setup.provider==2{"App password"}else{"Password or app password"},&mut setup.secret,true);
        text(ui,if setup.account.connection==Connection::Pop3{"POP3 downloads your inbox and leaves messages on the server. Folders and read states are local."}else{"IMAP keeps folders and message states synchronized."});
       }
       ui.add_space(16.0);
       let label=match setup.account.connection{Connection::Gmail=>"Sign in with Google",Connection::Outlook=>"Sign in with Microsoft",_=>"Connect"};
       if theme::primary_button(ui,[width,42.0],label).clicked(){
        match validate(&setup.account,&setup.secret){Err(e)=>setup.error=e,Ok(())=>{
         let a=setup.account.clone();let secret=setup.secret.clone();let client_secret=setup.client_secret.clone();let(tx,rx)=mpsc::channel();setup.pending=Some(rx);setup.error.clear();setup.cancel=Arc::new(AtomicBool::new(false));let cancel=setup.cancel.clone();let repaint=ctx.clone();
         std::thread::spawn(move||{let result=if a.connection.api(){network::oauth::authorize(a,client_secret,cancel.clone())}else{network::test_connection(&a,&secret).and_then(|()|{if cancel.load(Ordering::Relaxed){return Err("Connection cancelled".into());}network::save_password(&a,&secret)?;Ok(a)})};let _=tx.send(result);repaint.request_repaint();});
        }}
       }
      });
      if setup.pending.is_some(){ui.spinner();text(ui,if setup.account.connection.api(){"Waiting for browser sign-in…"}else{"Testing your connection…"});}
      if ui.add(action_button("Back")).clicked(){setup.cancel.store(true,Ordering::Relaxed);setup.pending=None;setup.step=1;setup.error.clear();setup.secret.clear();setup.client_secret.clear();}
     }else{
      text(ui,"Connection verified. Your account is ready.");
      if network::password_session_only(&setup.account.id){text(ui,"Password is kept for this session only. Enter it again after restarting Barid.");}ui.add_space(18.0);ui.label(crate::i18n::widget(RichText::new(&setup.account.email).size(20.0), ui.style()));ui.label(crate::i18n::widget(&setup.account.provider, ui.style()));ui.add_space(24.0);
      if theme::primary_button(ui,[width,42.0],"Open inbox").clicked(){finish=true;}
     }
     if !setup.error.is_empty(){ui.add_space(12.0);ui.label(crate::i18n::widget(RichText::new(i18n::display(&setup.error)).color(Color32::from_rgb(190,65,55)), ui.style()));}
    });});
   });
  });
        if finish {
            let account = setup.account.clone();
            if self
                .accounts
                .iter()
                .any(|a| a.id != account.id && a.email.eq_ignore_ascii_case(&account.email))
            {
                setup.error = "This account is already connected.".into();
            } else {
                match self.store.save("account", &account.id, &account) {
                    Ok(()) => {
                        self.accounts.retain(|a| a.id != account.id);
                        self.accounts.push(account.clone());
                        self.mails.retain(|m| m.account != "demo");
                        self.demo = false;
                        self.selected = None;
                        self.folder = "Inbox".into();
                        self.account_filter = Some(account.id.clone());
                        close = true;
                        self.sync_one(&account.id);
                    }
                    Err(e) => setup.error = e,
                }
            }
        }
        if close {
            setup.open = false;
            setup.cancel.store(true, Ordering::Relaxed);
            self.prefs.setup_complete = true;
        }
        i18n::set_language(self.prefs.language);
        if previous != serde_json::to_string(&self.prefs).unwrap_or_default()
            && let Err(e) = self.store.save("preferences", "default", &self.prefs)
        {
            self.status = e;
        }
        self.setup = setup;
    }
}
fn validate(account: &Account, secret: &str) -> Result<(), String> {
    if account.connection.api() {
        if account.oauth_client_id.trim().is_empty() {
            return Err(
                "Enter your OAuth application client ID under OAuth application settings.".into(),
            );
        }
        return Ok(());
    }
    if account.email.parse::<lettre::Address>().is_err() {
        return Err("Enter a valid email address.".into());
    }
    if account.imap_host.trim().is_empty()
        || account.smtp_host.trim().is_empty()
        || secret.is_empty()
    {
        return Err("Enter both server names and a password.".into());
    }
    if account.imap_host.contains(['/', ':', ' ', '\r', '\n'])
        || account.smtp_host.contains(['/', ':', ' ', '\r', '\n'])
    {
        return Err("Enter server hostnames without a URL or port.".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wizard_validates_protocol_requirements() {
        let mut a = Account {
            connection: Connection::Gmail,
            ..Default::default()
        };
        assert!(validate(&a, "").is_err());
        a.oauth_client_id = "desktop-client".into();
        assert!(validate(&a, "").is_ok());
        a.connection = Connection::Pop3;
        a.email = "me@example.com".into();
        a.imap_host = "pop.example.com".into();
        a.smtp_host = "smtp.example.com".into();
        assert!(validate(&a, "").is_err());
        assert!(validate(&a, "password").is_ok());
        a.smtp_host = "https://bad".into();
        assert!(validate(&a, "password").is_err());
    }
}
