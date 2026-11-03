//! Renders the main application states to PNG for visual review.
//! Run with: BARID_SHOTS=/some/dir cargo test visual_tour -- --ignored --test-threads=1
use super::*;
use egui_kittest::Harness;

fn harness(size: Vec2, setup: impl FnOnce(&mut Barid)) -> Harness<'static, Barid> {
    let mut app = Barid::with_store(Store::open(std::path::Path::new(":memory:")).unwrap());
    app.desktop_attempted = true;
    setup(&mut app);
    let mut h = Harness::builder()
        .with_size(size)
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_eframe(move |cc| {
            configure_fonts(&cc.egui_ctx);
            theme::install(&cc.egui_ctx, 1.0);
            app
        });
    h.run_steps(6);
    h
}

fn shoot(name: &str, size: Vec2, setup: impl FnOnce(&mut Barid)) {
    let Ok(dir) = std::env::var("BARID_SHOTS") else {
        return;
    };
    let mut h = harness(size, setup);
    let image = h.render().expect("render");
    image
        .save(std::path::Path::new(&dir).join(format!("{name}.png")))
        .unwrap();
}

#[test]
#[ignore]
fn visual_tour() {
    crate::i18n::set_language(crate::i18n::Language::English);
    let big = Vec2::new(1440.0, 900.0);
    let small = Vec2::new(1024.0, 680.0);
    shoot("01_inbox", big, |_| {});
    shoot("02_inbox_dark", big, |a| a.dark = true);
    shoot("03_inbox_small", small, |_| {});
    shoot("04_compose", big, |a| a.compose("new"));
    shoot("05_compose_dark", big, |a| {
        a.dark = true;
        a.compose("new")
    });
    shoot("06_quick_reply", big, |a| {
        if let Some(id) = a.selected.clone() {
            a.start_quick_reply(&id, false);
        }
    });
    for section in 0..8 {
        shoot(&format!("07_settings_{section}"), big, |a| {
            a.settings = true;
            a.dialogs.section = section;
        });
    }
    shoot("08_contacts", big, |a| a.contact_window = true);
    shoot("09_shortcuts", big, |a| a.shortcuts = true);
    shoot("10_setup", big, |a| a.setup.open = true);
    shoot("11_about", big, |a| a.dialogs.about = true);
    shoot("12_advanced_search", big, |a| a.organize.advanced = true);
    shoot("13_folders", big, |a| a.organize.folders = true);
    shoot("14_search", big, |a| {
        a.search_open = true;
        a.query = "from:".into();
    });
    shoot("15_empty_folder", big, |a| a.select_folder("Archive", None));
    shoot("16_arabic", big, |a| {
        a.prefs.language = crate::i18n::Language::Arabic;
    });
    shoot("06b_quick_reply_dark", big, |a| {
        a.dark = true;
        if let Some(id) = a.selected.clone() {
            a.start_quick_reply(&id, true);
        }
    });
    shoot("17_compose_small", small, |a| a.compose("new"));
    shoot("18_settings_small", small, |a| a.settings = true);
}


fn readme_mail() -> Vec<crate::model::Mail> {
    let now = chrono::Utc::now().timestamp();
    [
        ("Amelia Chen <amelia@studio.example>", "Project kickoff notes for Thursday", "Hi Alex,\n\nThanks for a great conversation yesterday. I’ve pulled the notes together so we can start the week with a clear plan.\n\n• Keep the first release simple and focused.\n• Share the draft designs with the team by Wednesday.\n• Book a review with the client for next Monday.\n\nShall we walk through it together on Thursday morning?\n\nBest,\nAmelia", "Work", true),
        ("Camille Dubois <camille@atelier.example>", "Compte rendu de la réunion de lundi", "Bonjour Alex,\n\nVoici le compte rendu de notre réunion. Merci de vérifier les dates avant vendredi.\n\nBien cordialement,\nCamille", "Work", true),
        ("سلمى الحسيني <salma@example.org>", "دعوة لحضور ورشة العمل يوم الأحد", "مرحباً أليكس،\n\nيسعدنا دعوتك لحضور ورشة العمل القادمة يوم الأحد في الساعة العاشرة صباحاً.\n\nمع أطيب التحيات،\nسلمى", "Personal", true),
        ("Oliver Park <oliver@park.example>", "Coffee on Friday?", "Hey Alex,\n\nThere’s a new place on the corner. Are you around Friday morning?\n\nOliver", "Personal", false),
        ("Julien Moreau <julien@moreau.example>", "Facture n° 2026-118 — échéance le 15", "Bonjour,\n\nVous trouverez ci-joint la facture du mois d’octobre.\n\nCordialement,\nJulien", "Updates", false),
        ("يوسف بن علي <youssef@example.net>", "صور الرحلة إلى مراكش", "أهلاً أليكس،\n\nأرسلت لك صور الرحلة. كانت أياماً جميلة!\n\nيوسف", "Personal", false),
        ("Linear <notifications@linear.example>", "Your weekly digest is ready", "Three projects moved forward this week, and your team completed 24 issues.", "Updates", false),
        ("Sofia Martinez <sofia@studio.example>", "Re: Brand explorations — round two", "These are looking beautiful. Let’s keep exploring the warmer palette.\n\nSofia", "Work", false),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (from, subject, body, tag, unread))| crate::model::Mail {
        id: format!("readme-{i}"),
        account: "demo".into(),
        folder: "Inbox".into(),
        from: from.into(),
        to: "Alex Morgan <alex@example.com>".into(),
        subject: subject.into(),
        body: body.into(),
        date: now - i as i64 * 5400,
        unread,
        starred: i == 0 || i == 2,
        tags: vec![tag.into()],
        ..Default::default()
    })
    .collect()
}

/// README screenshots in English, French and Arabic, using an in-memory store and sample mail.
/// Run with: MAKTAB_SCREENSHOTS=/some/dir cargo test readme_screenshots -- --ignored
#[test]
#[ignore]
fn readme_screenshots() {
    use crate::i18n::Language;
    let Ok(dir) = std::env::var("MAKTAB_SCREENSHOTS") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (code, language, open) in [
        ("en", Language::English, "readme-0"),
        ("fr", Language::French, "readme-1"),
        ("ar", Language::Arabic, "readme-2"),
    ] {
        crate::i18n::set_language(language);
        let mut h = harness(Vec2::new(1440.0, 860.0), |a| {
            a.prefs.language = language;
            a.mails = readme_mail();
            a.selected = Some(open.into());
        });
        let image = h.render().expect("render");
        image.save(dir.join(format!("barid-{code}.png"))).unwrap();
    }
    crate::i18n::set_language(Language::English);
}
