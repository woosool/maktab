//! Local desktop notifications for received and sent mail.

/// Shows a notification without blocking the UI thread.
pub fn show(title: String, body: String, sound: bool) {
    if cfg!(test) {
        return;
    }
    std::thread::spawn(move || {
        let mut n = notify_rust::Notification::new();
        n.appname("Barid").summary(&title).body(&body);
        if sound {
            n.sound_name("default");
        }
        if let Err(e) = n.show() {
            eprintln!("notification failed: {e}");
        }
    });
}

/// Title and body for newly received messages, or `None` when there is nothing to announce.
pub fn received_text(new: &[(String, String)], previews: bool) -> Option<(String, String)> {
    match new {
        [] => None,
        [(from, subject)] if previews => Some((from.clone(), subject.clone())),
        [_] => Some(("Barid".into(), crate::i18n::display("You have a new message"))),
        many if previews => Some((
            format!("{} new messages", many.len()),
            many.iter()
                .take(3)
                .map(|(from, _)| from.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        )),
        many => Some(("Barid".into(), format!("You have {} new messages", many.len()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn received_text_respects_privacy_and_counts() {
        assert_eq!(received_text(&[], true), None);
        let one = vec![("Amelia".to_string(), "Hello".to_string())];
        assert_eq!(received_text(&one, true), Some(("Amelia".into(), "Hello".into())));
        assert_eq!(received_text(&one, false).unwrap().1, "You have a new message");
        let two = vec![one[0].clone(), ("Oliver".into(), "Coffee".into())];
        assert_eq!(received_text(&two, true).unwrap(), ("2 new messages".into(), "Amelia, Oliver".into()));
        assert_eq!(received_text(&two, false).unwrap().1, "You have 2 new messages");
    }
}
