use crate::model::*;
use rusqlite::{Connection, params};
use std::path::Path;

pub struct Store {
    db: Connection,
}
impl Store {
    pub fn open(path: &Path) -> Result<Self, String> {
        let db = Connection::open(path).map_err(|e| e.to_string())?;
        db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS records (kind TEXT NOT NULL, id TEXT NOT NULL, data TEXT NOT NULL, PRIMARY KEY(kind,id));").map_err(|e| e.to_string())?;
        Ok(Self { db })
    }
    pub fn load<T: serde::de::DeserializeOwned>(&self, kind: &str) -> Result<Vec<T>, String> {
        let mut stmt = self
            .db
            .prepare("SELECT data FROM records WHERE kind=?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([kind], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.map(|s| {
            serde_json::from_str(&s.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
        })
        .collect()
    }
    pub fn save<T: serde::Serialize>(&self, kind: &str, id: &str, value: &T) -> Result<(), String> {
        let json = serde_json::to_string(value).map_err(|e| e.to_string())?;
        self.db.execute("INSERT INTO records(kind,id,data) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO UPDATE SET data=excluded.data", params![kind,id,json]).map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn remove(&self, kind: &str, id: &str) -> Result<(), String> {
        self.db
            .execute(
                "DELETE FROM records WHERE kind=?1 AND id=?2",
                params![kind, id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn apply_sync(&self, account: &str, batch: &SyncBatch) -> Result<(), String> {
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM records WHERE kind='mail' AND json_extract(data,'$.account')=?1 AND json_extract(data,'$.uid') IS NOT NULL AND COALESCE(json_extract(data,'$.remote_folder'),'') NOT IN (SELECT value FROM json_each(?2))", params![account,serde_json::to_string(&batch.failed_folders).map_err(|e|e.to_string())?]).map_err(|e| e.to_string())?;
        tx.execute(
            "DELETE FROM records WHERE kind='folder' AND json_extract(data,'$.account')=?1",
            [account],
        )
        .map_err(|e| e.to_string())?;
        for mail in &batch.messages {
            if mail.account != account || mail.uid.is_none() {
                return Err("Sync returned an invalid account or message UID".into());
            }
            let data = serde_json::to_string(mail).map_err(|e| e.to_string())?;
            tx.execute("INSERT INTO records(kind,id,data) VALUES('mail',?1,?2) ON CONFLICT(kind,id) DO UPDATE SET data=excluded.data", params![mail.id,data]).map_err(|e| e.to_string())?;
        }
        for folder in &batch.folders {
            if folder.account != account {
                return Err("Sync returned a folder for another account".into());
            }
            let data = serde_json::to_string(folder).map_err(|e| e.to_string())?;
            tx.execute(
                "INSERT INTO records(kind,id,data) VALUES('folder',?1,?2)",
                params![
                    format!(
                        "{}:{}:{}",
                        account,
                        if folder.is_label { "label" } else { "folder" },
                        folder.path
                    ),
                    data
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())
    }
    pub fn save_contacts(&self, contacts: &[Contact]) -> Result<(), String> {
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        for contact in contacts {
            let data = serde_json::to_string(contact).map_err(|e| e.to_string())?;
            tx.execute("INSERT INTO records(kind,id,data) VALUES('contact',?1,?2) ON CONFLICT(kind,id) DO UPDATE SET data=excluded.data",params![contact.key(),data]).map_err(|e|e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())
    }
    pub fn mails(&self) -> Result<Vec<Mail>, String> {
        self.load("mail")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn label_and_folder_can_share_a_path_without_colliding() {
        let store = Store::open(Path::new(":memory:")).unwrap();
        let batch = SyncBatch {
            folders: vec![
                Folder {
                    is_label: false,
                    account: "a".into(),
                    path: "Work".into(),
                    name: "Work".into(),
                },
                Folder {
                    is_label: true,
                    account: "a".into(),
                    path: "Work".into(),
                    name: "Work".into(),
                },
            ],
            ..Default::default()
        };
        store.apply_sync("a", &batch).unwrap();
        assert_eq!(store.load::<Folder>("folder").unwrap().len(), 2);
    }
    #[test]
    fn partial_sync_preserves_failed_folder_but_reconciles_successful_empty_folders() {
        let store = Store::open(Path::new(":memory:")).unwrap();
        for (id, account, path) in [
            ("archive-old", "a", "Archive"),
            ("inbox-deleted", "a", "INBOX"),
            ("other", "b", "INBOX"),
        ] {
            store
                .save(
                    "mail",
                    id,
                    &Mail {
                        id: id.into(),
                        account: account.into(),
                        remote_folder: path.into(),
                        uid: Some(1),
                        ..Default::default()
                    },
                )
                .unwrap();
        }
        let batch = SyncBatch {
            failed_folders: vec!["Archive".into()],
            warnings: vec!["Archive: Connection Lost".into()],
            folders: vec![Folder {
                is_label: false,
                account: "a".into(),
                path: "Archive".into(),
                name: "Archive".into(),
            }],
            ..Default::default()
        };
        store.apply_sync("a", &batch).unwrap();
        let ids: std::collections::HashSet<_> =
            store.mails().unwrap().into_iter().map(|m| m.id).collect();
        assert!(ids.contains("archive-old"));
        assert!(ids.contains("other"));
        assert!(!ids.contains("inbox-deleted"));
        // A later complete snapshot can remove mail deleted remotely from Archive.
        store.apply_sync("a", &SyncBatch::default()).unwrap();
        assert_eq!(store.mails().unwrap().len(), 1);
    }
    #[test]
    fn sync_replacement_preserves_local_drafts_and_other_accounts_and_rolls_back_errors() {
        let store = Store::open(Path::new(":memory:")).unwrap();
        for (id, account, uid) in [
            ("remote-old", "a", Some(1)),
            ("local-draft", "a", None),
            ("other-account", "b", Some(2)),
        ] {
            store
                .save(
                    "mail",
                    id,
                    &Mail {
                        id: id.into(),
                        account: account.into(),
                        uid,
                        ..Default::default()
                    },
                )
                .unwrap();
        }
        let batch = SyncBatch {
            failed_folders: Vec::new(),
            warnings: Vec::new(),
            folders: vec![Folder {
                is_label: false,
                account: "a".into(),
                path: "[Gmail]/Trash".into(),
                name: "Trash".into(),
            }],
            messages: vec![Mail {
                id: "remote-new".into(),
                account: "a".into(),
                uid: Some(3),
                ..Default::default()
            }],
        };
        store.apply_sync("a", &batch).unwrap();
        let ids: std::collections::HashSet<_> =
            store.mails().unwrap().into_iter().map(|m| m.id).collect();
        assert_eq!(ids.len(), 3);
        assert!(ids.contains("local-draft"));
        assert!(ids.contains("other-account"));
        assert!(!ids.contains("remote-old"));
        assert_eq!(
            store.load::<Folder>("folder").unwrap()[0].path,
            "[Gmail]/Trash"
        );
        let invalid = SyncBatch {
            failed_folders: Vec::new(),
            warnings: Vec::new(),
            messages: vec![Mail {
                id: "bad".into(),
                account: "b".into(),
                uid: Some(9),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(store.apply_sync("a", &invalid).is_err());
        assert_eq!(store.mails().unwrap().len(), 3);
        assert_eq!(store.load::<Folder>("folder").unwrap().len(), 1);
    }
    #[test]
    fn database_roundtrip_update_delete_and_reopen() {
        let path = std::env::temp_dir().join(format!("barid-test-{}.db", uuid::Uuid::new_v4()));
        {
            let store = Store::open(&path).unwrap();
            let mut m = Mail {
                id: "test".into(),
                subject: "First".into(),
                ..Default::default()
            };
            store.save("mail", &m.id, &m).unwrap();
            m.subject = "Updated".into();
            store.save("mail", &m.id, &m).unwrap();
            store
                .save(
                    "contact",
                    "test",
                    &Contact {
                        name: "Alex".into(),
                        email: "alex@example.com".into(),
                        ..Default::default()
                    },
                )
                .unwrap();
            assert_eq!(store.mails().unwrap().len(), 1);
        }
        {
            let store = Store::open(&path).unwrap();
            assert_eq!(store.mails().unwrap()[0].subject, "Updated");
            store.remove("mail", "test").unwrap();
            assert!(store.mails().unwrap().is_empty());
            assert_eq!(store.load::<Contact>("contact").unwrap().len(), 1);
        }
        let _ = std::fs::remove_file(path);
    }
}
