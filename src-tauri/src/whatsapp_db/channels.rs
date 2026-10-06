//! Channel (newsletter) messages: server ids and reaction totals.

use super::*;

impl ChatDb {
    /// Ids of channel (newsletter) chats, newest first — so their names can be fetched.
    pub fn newsletter_ids(&self) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM chats WHERE id LIKE '%@newsletter' ORDER BY last_timestamp DESC",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect()
    }

    /// Remembers a channel message's server id, which reactions are keyed by.
    pub fn set_server_id(&self, chat_id: &str, id: &str, server_id: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE messages SET server_id = ?3 WHERE chat_id = ?1 AND id = ?2",
            params![chat_id, id, server_id],
        )?;
        Ok(())
    }

    /// The lowest server id stored for a channel: where the next older page starts.
    pub fn oldest_server_id(&self, chat_id: &str) -> rusqlite::Result<Option<i64>> {
        self.conn.query_row(
            "SELECT MIN(server_id) FROM messages WHERE chat_id = ?1",
            params![chat_id],
            |r| r.get(0),
        )
    }

    /// Replaces the reaction totals of one channel message.
    pub fn set_channel_reactions(
        &self,
        chat_id: &str,
        id: &str,
        reactions: &[(String, u64)],
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "DELETE FROM channel_reactions WHERE chat_id = ?1 AND id = ?2",
            params![chat_id, id],
        )?;
        for (emoji, count) in reactions {
            self.conn.execute(
                "INSERT OR REPLACE INTO channel_reactions (chat_id, id, emoji, count) VALUES (?1, ?2, ?3, ?4)",
                params![chat_id, id, emoji, *count as i64],
            )?;
        }
        Ok(())
    }

    /// Reaction totals of a channel's messages, by message id.
    pub(super) fn channel_reactions(
        &self,
        chat_id: &str,
    ) -> rusqlite::Result<HashMap<String, Vec<ChannelReaction>>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, emoji, count FROM channel_reactions WHERE chat_id = ?1 ORDER BY count DESC",
        )?;
        let rows = stmt.query_map(params![chat_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                ChannelReaction {
                    emoji: r.get(1)?,
                    count: r.get::<_, i64>(2)? as u64,
                },
            ))
        })?;
        let mut map: HashMap<String, Vec<ChannelReaction>> = HashMap::new();
        for row in rows {
            let (id, reaction) = row?;
            map.entry(id).or_default().push(reaction);
        }
        Ok(map)
    }
}
