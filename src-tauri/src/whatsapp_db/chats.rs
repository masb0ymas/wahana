//! Chats: the list, unread state, and mute and pin state mirrored from the phone.

use super::*;

impl ChatDb {
    /// Records a chat's mute state (see the `mutes` table).
    pub fn set_mute(&self, id: &str, until: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO mutes (id, until) VALUES (?1, ?2) ON CONFLICT (id) DO UPDATE SET until = excluded.until",
            params![id, until],
        )?;
        Ok(())
    }

    /// The recorded mute of a chat, found under its phone-number or privacy id alike.
    pub(super) fn mute_for(&self, id: &str) -> rusqlite::Result<Option<i64>> {
        self.chat_state(id, "SELECT until FROM mutes WHERE id = ?1")
    }

    /// Records a chat's pin state as WhatsApp has it (see the `pins` table).
    pub fn set_pin(&self, id: &str, pinned_at: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO pins (id, pinned_at) VALUES (?1, ?2) ON CONFLICT (id) DO UPDATE SET pinned_at = excluded.pinned_at",
            params![id, pinned_at],
        )?;
        Ok(())
    }

    /// Drops a chat's pin record, under its phone-number and privacy ids alike, for a pin
    /// kept only in this app so a past unpin on the phone does not undo it.
    pub fn forget_pin(&self, id: &str) -> rusqlite::Result<()> {
        let pn = self.pn_for(id)?;
        let lid = self.lid_for(id)?;
        for candidate in [Some(id), pn.as_deref(), lid.as_deref()]
            .into_iter()
            .flatten()
        {
            self.conn
                .execute("DELETE FROM pins WHERE id = ?1", params![candidate])?;
        }
        Ok(())
    }

    /// The recorded WhatsApp pin of a chat, found under its phone-number or privacy id alike.
    pub(super) fn pin_for(&self, id: &str) -> rusqlite::Result<Option<i64>> {
        self.chat_state(id, "SELECT pinned_at FROM pins WHERE id = ?1")
    }

    /// One value of a per-chat table, looked up under the chat's own, phone-number and privacy
    /// ids in turn, since the phone may have recorded it under either.
    pub(super) fn chat_state(&self, id: &str, sql: &str) -> rusqlite::Result<Option<i64>> {
        let pn = self.pn_for(id)?;
        let lid = self.lid_for(id)?;
        for candidate in [Some(id), pn.as_deref(), lid.as_deref()]
            .into_iter()
            .flatten()
        {
            let found = self
                .conn
                .query_row(sql, params![candidate], |r| r.get::<_, i64>(0))
                .optional()?;
            if found.is_some() {
                return Ok(found);
            }
        }
        Ok(None)
    }

    /// Makes sure a chat row exists, e.g. for a conversation history listed without
    /// messages. Its preview and unread count come from `unread` and later messages.
    pub fn ensure_chat(&self, id: &str, timestamp: i64, unread: u32) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO chats (id, fallback_name, last_timestamp, unread) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (id) DO UPDATE SET
                 last_timestamp = MAX(chats.last_timestamp, excluded.last_timestamp),
                 unread = excluded.unread",
            params![id, fallback_name(id), timestamp, unread],
        )?;
        Ok(())
    }

    pub fn mark_read(&self, chat_id: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE chats SET unread = 0 WHERE id = ?1",
            params![chat_id],
        )?;
        Ok(())
    }

    /// Applies a read state another device synced for a chat. The chat may be stored under
    /// its phone-number or privacy id, so every alias is updated; "unread" only flags a chat
    /// that has no count yet. Returns whether anything changed.
    pub fn set_read_from_device(&self, id: &str, read: bool) -> rusqlite::Result<bool> {
        let ids = [Some(id.to_string()), self.pn_for(id)?, self.lid_for(id)?];
        let sql = if read {
            "UPDATE chats SET unread = 0 WHERE id = ?1 AND unread > 0"
        } else {
            "UPDATE chats SET unread = 1 WHERE id = ?1 AND unread = 0"
        };
        let mut changed = false;
        for id in ids.iter().flatten() {
            changed |= self.conn.execute(sql, params![id])? > 0;
        }
        Ok(changed)
    }

    pub fn mark_all_read(&self) -> rusqlite::Result<()> {
        self.conn
            .execute("UPDATE chats SET unread = 0 WHERE unread > 0", [])?;
        Ok(())
    }

    /// Removes a chat with its messages, edit history and label links from this device.
    pub fn delete_chat(&self, chat_id: &str) -> rusqlite::Result<()> {
        for sql in [
            "DELETE FROM poll_votes WHERE poll_id IN (SELECT id FROM messages WHERE chat_id = ?1 AND kind = 'poll')",
            "DELETE FROM messages WHERE chat_id = ?1",
            "DELETE FROM message_edits WHERE chat_id = ?1",
            "DELETE FROM chat_labels WHERE chat_id = ?1",
            "DELETE FROM chats WHERE id = ?1",
        ] {
            self.conn.execute(sql, params![chat_id])?;
        }
        Ok(())
    }

    pub fn unread_chats(&self) -> u32 {
        self.conn
            .query_row("SELECT COUNT(*) FROM chats WHERE unread > 0", [], |r| {
                r.get(0)
            })
            .unwrap_or(0)
    }

    pub fn chats(&self) -> rusqlite::Result<Vec<ChatInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, last_text, last_timestamp, last_from_me, last_sender_id, last_sender, unread,
                    (SELECT ack FROM messages m WHERE m.chat_id = chats.id AND m.from_me = 1
                     ORDER BY m.timestamp DESC LIMIT 1)
             FROM chats WHERE id != 'status@broadcast' ORDER BY last_timestamp DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, bool>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, u32>(6)?,
                r.get::<_, Option<u8>>(7)?,
            ))
        })?;
        let mut senders: HashMap<String, Resolved> = HashMap::new();
        let mut chats = Vec::new();
        for row in rows {
            let (id, last_text, last_timestamp, last_from_me, sender_id, sender, unread, last_ack) =
                row?;
            let label = self.chat_label(&id)?;
            let last_sender = if sender_id.is_empty() {
                sender
            } else {
                if !senders.contains_key(&sender_id) {
                    senders.insert(sender_id.clone(), self.resolve(&sender_id)?);
                }
                let s = &senders[&sender_id];
                s.name
                    .as_ref()
                    .map(|(n, _)| n.clone())
                    .or_else(|| s.phone.clone())
                    .unwrap_or(sender)
            };
            let muted_until = self.mute_for(&id)?;
            let pinned_at = self.pin_for(&id)?;
            let community = self
                .conn
                .query_row(
                    "SELECT parent FROM communities WHERE id = ?1",
                    params![id],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
                .map(|parent| {
                    self.resolve(&parent)
                        .ok()
                        .and_then(|r| r.name.map(|(n, _)| n))
                        .unwrap_or_else(|| fallback_name(&parent))
                });
            chats.push(ChatInfo {
                muted_until,
                pinned_at,
                community,
                saved: label.saved,
                name: label.name,
                phone: label.phone,
                id,
                last_text,
                last_timestamp,
                last_from_me,
                last_ack: last_ack.unwrap_or(1),
                last_sender,
                unread,
            });
        }
        Ok(chats)
    }

    /// How a chat is labelled: the saved contact name (a group's subject counts as saved), else
    /// the phone number, else a fallback. Shared by the chat list and notifications so both
    /// name a chat the same way.
    pub fn chat_label(&self, id: &str) -> rusqlite::Result<ChatLabel> {
        let Resolved { name, phone } = self.resolve(id)?;
        let group = id.ends_with("@g.us");
        Ok(ChatLabel {
            saved: group
                || name
                    .as_ref()
                    .is_some_and(|(_, source)| *source >= NameSource::Contact as i64),
            name: name
                .map(|(n, _)| n)
                .or_else(|| phone.clone())
                .unwrap_or_else(|| fallback_name(id)),
            phone: if group { None } else { phone },
        })
    }
}
