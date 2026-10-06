//! Chat labels, as cached from app-state sync.

use super::*;

impl ChatDb {
    /// Labels that are not deleted, by name.
    pub fn labels(&self) -> rusqlite::Result<Vec<LabelRow>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, color FROM labels WHERE deleted = 0 ORDER BY name")?;
        let rows = stmt.query_map([], |r| {
            Ok(LabelRow {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
            })
        })?;
        rows.collect()
    }

    /// Every (chat id, label id) association that is active.
    pub fn all_chat_labels(&self) -> rusqlite::Result<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT chat_id, label_id FROM chat_labels WHERE labeled = 1")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.collect()
    }

    /// Ids of the labels currently on a chat.
    pub fn chat_labels(&self, chat_id: &str) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT label_id FROM chat_labels WHERE chat_id = ?1 AND labeled = 1")?;
        let rows = stmt.query_map(params![chat_id], |r| r.get::<_, String>(0))?;
        rows.collect()
    }

    /// Records a label from app-state sync (create, rename, recolor, or delete).
    pub fn upsert_label(
        &self,
        id: &str,
        name: &str,
        color: i64,
        deleted: bool,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO labels (id, name, color, deleted) VALUES (?1,?2,?3,?4)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, color=excluded.color, deleted=excluded.deleted",
            params![id, name, color, deleted],
        )?;
        Ok(())
    }

    /// Drops a label and its chat associations.
    pub fn delete_label(&self, id: &str) -> rusqlite::Result<()> {
        self.conn
            .execute("DELETE FROM chat_labels WHERE label_id = ?1", params![id])?;
        self.conn
            .execute("DELETE FROM labels WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Associates or dissociates a label and a chat.
    pub fn set_chat_label(
        &self,
        label_id: &str,
        chat_id: &str,
        labeled: bool,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO chat_labels (label_id, chat_id, labeled) VALUES (?1,?2,?3)
             ON CONFLICT(label_id, chat_id) DO UPDATE SET labeled=excluded.labeled",
            params![label_id, chat_id, labeled],
        )?;
        Ok(())
    }
}
