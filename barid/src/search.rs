use crate::model::Mail;
/// Quotes preserve spaces; OR splits alternatives, while terms within an alternative use AND.
pub fn tokens(query: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut term = String::new();
    let (mut quoted, mut escape) = (false, false);
    for c in query.chars() {
        if escape {
            term.push(c);
            escape = false;
        } else if c == '\\' && quoted {
            escape = true;
        } else if c == '"' {
            quoted = !quoted;
        } else if c.is_whitespace() && !quoted {
            if !term.is_empty() {
                result.push(std::mem::take(&mut term));
            }
        } else {
            term.push(c);
        }
    }
    if escape {
        term.push('\\');
    }
    if !term.is_empty() {
        result.push(term);
    }
    result
}
pub fn matches(mail: &Mail, query: &str) -> bool {
    let tokens = tokens(query);
    tokens.split(|t| t.eq_ignore_ascii_case("OR")).any(|group| {
        !group.is_empty()
            && group.iter().all(|t| {
                if let Some(t) = t.strip_prefix('-') {
                    !term(mail, t)
                } else {
                    term(mail, t)
                }
            })
    }) || tokens.is_empty()
}
fn term(m: &Mail, term: &str) -> bool {
    let t = term.to_lowercase();
    let Some((key, value)) = t.split_once(':') else {
        return format!(
            "{} {} {} {} {}",
            m.from,
            m.to,
            m.subject,
            m.body,
            m.tags.join(" ")
        )
        .to_lowercase()
        .contains(&t);
    };
    match key {
        "from" => m.from.to_lowercase().contains(value),
        "to" => m.to.to_lowercase().contains(value),
        "cc" => m.cc.to_lowercase().contains(value),
        "subject" => m.subject.to_lowercase().contains(value),
        "body" => m.body.to_lowercase().contains(value),
        "in" | "folder" => {
            m.folder.eq_ignore_ascii_case(value) || m.remote_folder.eq_ignore_ascii_case(value)
        }
        "account" => m.account.eq_ignore_ascii_case(value),
        "label" | "tag" => m
            .tags
            .iter()
            .chain(&m.remote_labels)
            .any(|s| s.eq_ignore_ascii_case(value)),
        "is" => match value {
            "read" => !m.unread,
            "unread" => m.unread,
            "starred" | "flagged" => m.starred,
            "pinned" => m.pinned,
            _ => false,
        },
        "has" => match value {
            "attachment" => !m.attachments.is_empty(),
            _ => false,
        },
        "filename" => m
            .attachments
            .iter()
            .any(|a| a.name.to_lowercase().contains(value)),
        "after" | "before" => chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .ok()
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .is_some_and(|d| {
                if key == "after" {
                    m.date >= d.and_utc().timestamp()
                } else {
                    m.date < d.and_utc().timestamp()
                }
            }),
        "larger" | "smaller" => size(value).is_some_and(|v| {
            let len = if m.raw.is_empty() {
                m.body.len() + m.attachments.iter().map(|a| a.data.len()).sum::<usize>()
            } else {
                m.raw.len()
            };
            if key == "larger" {
                len as u64 > v
            } else {
                (len as u64) < v
            }
        }),
        _ => false,
    }
}
fn size(s: &str) -> Option<u64> {
    let (s, mult) = if let Some(v) = s.strip_suffix('m') {
        (v, 1024 * 1024)
    } else if let Some(v) = s.strip_suffix('k') {
        (v, 1024)
    } else {
        (s, 1)
    };
    s.parse::<u64>().ok()?.checked_mul(mult)
}
pub fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn phrases_negation_or_and_dates() {
        let m = Mail {
            from: "Alice Smith <alice@example.com>".into(),
            subject: "Project plan".into(),
            date: chrono::NaiveDate::from_ymd_opt(2026, 10, 30)
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap()
                .and_utc()
                .timestamp(),
            unread: true,
            pinned: true,
            ..Default::default()
        };
        assert!(matches(
            &m,
            "from:\"Alice Smith\" -is:read after:2026-10-29 before:2026-11-01"
        ));
        assert!(matches(&m, "from:bob OR is:pinned"));
        assert!(!matches(&m, "subject:\"other plan\""));
        assert!(!matches(&m, "after:not-a-date"));
        assert!(!matches(&m, "-is:unread"));
        assert_eq!(
            tokens(&format!("subject:{}", quote("plan \"A\""))),
            vec!["subject:plan \"A\""]
        );
    }
}
