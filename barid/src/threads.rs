use crate::model::Mail;
use std::collections::HashMap;
fn ids(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter(|s| s.starts_with('<') && s.ends_with('>'))
        .map(String::from)
        .collect()
}
pub fn groups(mails: &[Mail]) -> Vec<Vec<usize>> {
    let mut parents: Vec<usize> = (0..mails.len()).collect();
    let mut identities: HashMap<(String, String), usize> = HashMap::new();
    fn root(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    for (i, m) in mails.iter().enumerate() {
        let mut keys = ids(&m.message_id);
        keys.extend(ids(&m.references));
        keys.extend(ids(&m.in_reply_to));
        if !m.thread_id.is_empty() {
            keys.push(format!("provider:{}", m.thread_id));
        }
        for key in keys {
            if let Some(&other) = identities.get(&(m.account.clone(), key.clone())) {
                let a = root(&mut parents, i);
                let b = root(&mut parents, other);
                parents[a] = b;
            } else {
                identities.insert((m.account.clone(), key), i);
            }
        }
    }
    let mut result: Vec<Vec<usize>> = Vec::new();
    let mut positions = HashMap::new();
    for i in 0..mails.len() {
        let key = root(&mut parents, i);
        let slot = *positions.entry(key).or_insert_with(|| {
            result.push(Vec::new());
            result.len() - 1
        });
        result[slot].push(i);
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn references_group_across_missing_parents_but_never_accounts_or_subjects() {
        let mails = vec![
            Mail {
                account: "a".into(),
                references: "<root>".into(),
                message_id: "<one>".into(),
                ..Default::default()
            },
            Mail {
                account: "a".into(),
                in_reply_to: "<root>".into(),
                ..Default::default()
            },
            Mail {
                account: "b".into(),
                references: "<root>".into(),
                ..Default::default()
            },
            Mail {
                account: "a".into(),
                subject: "Re: hello".into(),
                ..Default::default()
            },
        ];
        let g = groups(&mails);
        assert_eq!(g, vec![vec![0, 1], vec![2], vec![3]]);
    }
}
