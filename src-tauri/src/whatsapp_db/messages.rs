//! Messages: storing, reading back, receipts, edits and deletes.

use super::*;

impl ChatDb {
    /// Stores a message and updates its chat. Returns false when it was already known, so
    /// a redelivery neither notifies nor counts as unread twice. `count_unread` is off for
    /// history, whose unread counts come from the conversation itself.
    pub fn insert_message(
        &self,
        msg: &IncomingMessage,
        count_unread: bool,
    ) -> rusqlite::Result<bool> {
        let v = &msg.view;
        let m = msg.media.as_ref();
        let inserted = self.conn.execute(
            "INSERT OR IGNORE INTO messages (chat_id, id, from_me, sender_id, sender_name, kind, body, timestamp,
                 media_kind, mimetype, file_name, file_size, seconds, width, height, thumbnail, media_proto, ack,
                 quote_id, quote_sender, quote_text, album_id,
                 preview_url, preview_title, preview_description, preview_image, quote_chat,
                 status_mention_id, interactive, poll_secret, poll_creator)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31)",
            params![
                v.chat_id,
                v.id,
                v.from_me,
                msg.sender_id,
                v.sender_name,
                kind_str(v.kind),
                v.body,
                v.timestamp,
                m.map(|m| m.kind),
                m.map(|m| m.mimetype.as_str()),
                m.and_then(|m| m.file_name.as_deref()),
                m.and_then(|m| m.size).map(|s| s as i64),
                m.and_then(|m| m.seconds),
                m.and_then(|m| m.width),
                m.and_then(|m| m.height),
                m.and_then(|m| m.thumbnail.as_deref()),
                m.map(|m| m.proto.as_slice()),
                v.ack,
                msg.quote.as_ref().map(|q| q.id.as_str()),
                msg.quote.as_ref().map(|q| q.sender.as_str()),
                msg.quote.as_ref().map(|q| q.text.as_str()),
                msg.album,
                v.preview.as_ref().map(|p| p.url.as_str()),
                v.preview.as_ref().and_then(|p| p.title.as_deref()),
                v.preview.as_ref().and_then(|p| p.description.as_deref()),
                v.preview.as_ref().and_then(|p| p.image.as_deref()),
                msg.quote.as_ref().and_then(|q| q.chat.as_deref()),
                v.status_mention.as_deref(),
                v.interactive
                    .as_ref()
                    .and_then(|i| serde_json::to_string(i).ok()),
                msg.poll_key.as_ref().map(|k| k.secret.as_slice()),
                msg.poll_key.as_ref().and_then(|k| k.creator.as_deref()),
            ],
        )? > 0;
        if !inserted {
            // A history resend may know a later delivery state than what was stored.
            if v.from_me {
                self.conn.execute(
                    "UPDATE messages SET ack = ?3 WHERE chat_id = ?1 AND id = ?2 AND ack < ?3",
                    params![v.chat_id, v.id, v.ack],
                )?;
            }
            // Messages stored before attachments were kept have no media columns; fill
            // them in when the same message comes round again (e.g. "load older").
            if let Some(m) = m {
                self.conn.execute(
                    "UPDATE messages SET media_kind = ?3, mimetype = ?4, file_name = ?5, file_size = ?6,
                         seconds = ?7, width = ?8, height = ?9, thumbnail = ?10, media_proto = ?11
                     WHERE chat_id = ?1 AND id = ?2 AND media_proto IS NULL",
                    params![
                        v.chat_id,
                        v.id,
                        m.kind,
                        m.mimetype,
                        m.file_name,
                        m.size.map(|s| s as i64),
                        m.seconds,
                        m.width,
                        m.height,
                        m.thumbnail,
                        m.proto,
                    ],
                )?;
            }
            // Likewise fill in a link preview when a later copy of the message carries one.
            if let Some(p) = &v.preview {
                self.conn.execute(
                    "UPDATE messages SET preview_url = ?3, preview_title = ?4, preview_description = ?5, preview_image = ?6
                     WHERE chat_id = ?1 AND id = ?2 AND preview_url IS NULL",
                    params![
                        v.chat_id,
                        v.id,
                        p.url.as_str(),
                        p.title.as_deref(),
                        p.description.as_deref(),
                        p.image.as_deref(),
                    ],
                )?;
            }
            // And a poll's key, for polls stored before keys were kept.
            if let Some(k) = &msg.poll_key {
                self.conn.execute(
                    "UPDATE messages SET poll_secret = ?3, poll_creator = ?4
                     WHERE chat_id = ?1 AND id = ?2 AND poll_secret IS NULL",
                    params![v.chat_id, v.id, k.secret, k.creator],
                )?;
            }
            // And the story a mention points at, for mentions stored before it was kept.
            if let Some(id) = &v.status_mention {
                self.conn.execute(
                    "UPDATE messages SET status_mention_id = ?3
                     WHERE chat_id = ?1 AND id = ?2 AND status_mention_id IS NULL",
                    params![v.chat_id, v.id, id],
                )?;
            }
            return Ok(false);
        }
        if !v.from_me && !v.sender_name.is_empty() {
            self.set_name(&msg.sender_id, &v.sender_name, NameSource::PushName)?;
        }
        // Statuses live under `status@broadcast` but are not a chat: no row, no unread.
        if v.chat_id == "status@broadcast" {
            return Ok(true);
        }
        // Replying from another device means the chat was read there.
        let unread_sql = match (count_unread, v.from_me) {
            (_, true) => "0",
            (true, false) => "chats.unread + 1",
            (false, false) => "chats.unread",
        };
        // A corrupt future message time must not pin the chat to the top (see
        // `repair_implausible_chats`); the message itself is still stored as received.
        let chat_ts = v.timestamp.min(now_millis() + FUTURE_SLACK_MS);
        self.conn.execute(
            &format!(
                "INSERT INTO chats (id, fallback_name, last_text, last_timestamp, last_from_me, last_sender_id, last_sender, unread)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT (id) DO UPDATE SET
                     last_text = CASE WHEN excluded.last_timestamp >= chats.last_timestamp THEN excluded.last_text ELSE chats.last_text END,
                     last_from_me = CASE WHEN excluded.last_timestamp >= chats.last_timestamp THEN excluded.last_from_me ELSE chats.last_from_me END,
                     last_sender_id = CASE WHEN excluded.last_timestamp >= chats.last_timestamp THEN excluded.last_sender_id ELSE chats.last_sender_id END,
                     last_sender = CASE WHEN excluded.last_timestamp >= chats.last_timestamp THEN excluded.last_sender ELSE chats.last_sender END,
                     last_timestamp = MAX(chats.last_timestamp, excluded.last_timestamp),
                     unread = {unread_sql}"
            ),
            params![
                v.chat_id,
                fallback_name(&v.chat_id),
                preview(v.kind, &v.body, m.map(|m| m.kind)),
                chat_ts,
                v.from_me,
                msg.sender_id,
                v.sender_name,
                if count_unread && !v.from_me { 1 } else { 0 }
            ],
        )?;
        Ok(true)
    }

    /// Moves my messages with these ids forward to `ack` (never back). Matched by id alone:
    /// a receipt may name the chat by phone number or privacy id, whichever the chat isn't
    /// stored under. Returns whether any message changed.
    pub fn raise_ack(&self, ids: &[String], ack: u8) -> rusqlite::Result<bool> {
        let mut stmt = self
            .conn
            .prepare("UPDATE messages SET ack = ?2 WHERE id = ?1 AND from_me = 1 AND ack < ?2")?;
        let mut changed = false;
        for id in ids {
            changed |= stmt.execute(params![id, ack])? > 0;
        }
        Ok(changed)
    }

    /// Records when `recipient` reached `ack` (2 delivered, 3 read, 4 played) for each id.
    /// A later level implies the earlier ones, so those are filled in when missing.
    pub fn add_receipts(
        &self,
        ids: &[String],
        recipient: &str,
        ack: u8,
        at: i64,
    ) -> rusqlite::Result<()> {
        let mut stmt = self.conn.prepare(
            "INSERT OR IGNORE INTO receipts (message_id, recipient, ack, at) VALUES (?1, ?2, ?3, ?4)",
        )?;
        for id in ids {
            for level in 2..=ack {
                stmt.execute(params![id, recipient, level, at])?;
            }
        }
        Ok(())
    }

    /// Per recipient: (recipient, ack, unix ms), ordered by time.
    pub fn receipts_for(&self, id: &str) -> rusqlite::Result<Vec<(String, u8, i64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT recipient, ack, at FROM receipts WHERE message_id = ?1 ORDER BY at")?;
        let rows = stmt.query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect()
    }

    /// Removes one stored message (e.g. a status I deleted).
    pub fn delete_message(&self, chat_id: &str, id: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "DELETE FROM messages WHERE chat_id = ?1 AND id = ?2",
            params![chat_id, id],
        )?;
        Ok(())
    }

    /// Sender and id of the newest incoming messages, newest first: what a read receipt covers.
    pub fn incoming_ids(
        &self,
        chat_id: &str,
        limit: u32,
    ) -> rusqlite::Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT sender_id, id FROM messages WHERE chat_id = ?1 AND from_me = 0 ORDER BY timestamp DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![chat_id, limit], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        rows.collect()
    }

    /// The newest `limit` messages of a chat, oldest first.
    pub fn messages(&self, chat_id: &str, limit: u32) -> rusqlite::Result<Vec<MessageView>> {
        self.query_messages(chat_id, limit, false)
    }

    /// The newest `limit` messages with an attachment, oldest first: the chat's gallery.
    pub fn media_messages(&self, chat_id: &str, limit: u32) -> rusqlite::Result<Vec<MessageView>> {
        self.query_messages(chat_id, limit, true)
    }

    pub(super) fn query_messages(
        &self,
        chat_id: &str,
        limit: u32,
        media_only: bool,
    ) -> rusqlite::Result<Vec<MessageView>> {
        let filter = if media_only {
            "AND media_proto IS NOT NULL"
        } else {
            ""
        };
        let mut stmt = self.conn.prepare(&format!(
            "SELECT * FROM (
                 SELECT id, chat_id, from_me, sender_id, sender_name, kind, body, timestamp,
                        media_kind, mimetype, file_name, file_size, seconds, width, height, thumbnail, ack,
                        revoked_at, edited_at, quote_id, quote_sender, quote_text, album_id,
                        preview_url, preview_title, preview_description, preview_image, quote_chat,
                        status_mention_id, interactive, poll_secret IS NOT NULL
                 FROM messages
                 WHERE chat_id = ?1 {filter}
                 ORDER BY timestamp DESC
                 LIMIT ?2
             ) ORDER BY timestamp ASC"
        ))?;
        let rows = stmt.query_map(params![chat_id, limit], |r| {
            let media_kind: Option<String> = r.get(8)?;
            let media = media_kind.map(|kind| -> rusqlite::Result<MediaInfo> {
                Ok(MediaInfo {
                    kind,
                    mimetype: r.get::<_, Option<String>>(9)?.unwrap_or_default(),
                    file_name: r.get(10)?,
                    size: r.get::<_, Option<i64>>(11)?.map(|s| s as u64),
                    seconds: r.get(12)?,
                    width: r.get(13)?,
                    height: r.get(14)?,
                    thumbnail: r
                        .get::<_, Option<Vec<u8>>>(15)?
                        .as_deref()
                        .map(thumbnail_url),
                })
            });
            let quote = r
                .get::<_, Option<String>>(19)?
                .map(|id| -> rusqlite::Result<QuoteRef> {
                    Ok(QuoteRef {
                        id,
                        sender: r.get::<_, Option<String>>(20)?.unwrap_or_default(),
                        text: r.get::<_, Option<String>>(21)?.unwrap_or_default(),
                        chat: r.get(27)?,
                    })
                })
                .transpose()?;
            let preview = match r.get::<_, Option<String>>(23)? {
                Some(url) => Some(PreviewInfo {
                    url,
                    title: r.get(24)?,
                    description: r.get(25)?,
                    image: r.get(26)?,
                }),
                None => None,
            };
            Ok((
                r.get::<_, String>(3)?,
                quote,
                r.get::<_, Option<String>>(22)?,
                r.get::<_, bool>(30)?,
                MessageView {
                    id: r.get(0)?,
                    chat_id: r.get(1)?,
                    from_me: r.get(2)?,
                    sender_name: r.get(4)?,
                    sender_phone: None,
                    sender_id: None,
                    kind: kind_from(&r.get::<_, String>(5)?),
                    body: r.get(6)?,
                    timestamp: r.get(7)?,
                    media: media.transpose()?,
                    ack: r.get(16)?,
                    revoked_at: r.get(17)?,
                    edited_at: r.get(18)?,
                    edits: Vec::new(),
                    channel_reactions: Vec::new(),
                    reply_to: None,
                    status_mention: r.get(28)?,
                    album_id: None,
                    preview,
                    interactive: r
                        .get::<_, Option<String>>(29)?
                        .and_then(|s| serde_json::from_str::<Interactive>(&s).ok()),
                },
            ))
        })?;
        let mut senders: HashMap<String, Resolved> = HashMap::new();
        let mut messages = Vec::new();
        let mut edits = self.edits(chat_id)?;
        let mut channel_reactions = if chat_id.ends_with("@newsletter") {
            self.channel_reactions(chat_id)?
        } else {
            HashMap::new()
        };
        for row in rows {
            let (sender_id, quote, album_id, has_poll_key, mut view) = row?;
            view.album_id = album_id;
            if let Some(poll) = view.interactive.as_mut().and_then(|i| i.poll.as_mut()) {
                poll.results = Some(self.poll_results(&view.id, &poll.options, has_poll_key)?);
            }
            view.reply_to = quote.map(|q| self.reply_view(chat_id, q)).transpose()?;
            if let Some(reactions) = channel_reactions.remove(&view.id) {
                view.channel_reactions = reactions;
            }
            if view.edited_at.is_some() {
                view.edits = edits.remove(&view.id).unwrap_or_default();
            }
            if !sender_id.is_empty() {
                if !senders.contains_key(&sender_id) {
                    senders.insert(sender_id.clone(), self.resolve(&sender_id)?);
                }
                let who = &senders[&sender_id];
                if let Some((name, _)) = &who.name {
                    view.sender_name = name.clone();
                }
                view.sender_phone = who.phone.clone();
                view.sender_id = Some(sender_id);
            }
            messages.push(view);
        }
        Ok(messages)
    }

    /// What to show for a quoted message: the stored copy when there is one, else what
    /// the reply itself carried.
    pub(super) fn reply_view(&self, chat_id: &str, quote: QuoteRef) -> rusqlite::Result<ReplyView> {
        let find = |chat: &str| {
            self.conn
                .query_row(
                    "SELECT from_me, sender_id, sender_name, kind, body, media_kind
                     FROM messages WHERE chat_id = ?1 AND id = ?2",
                    params![chat, quote.id],
                    |r| {
                        Ok((
                            r.get::<_, bool>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, String>(3)?,
                            r.get::<_, String>(4)?,
                            r.get::<_, Option<String>>(5)?,
                        ))
                    },
                )
                .optional()
        };
        // A story reply quotes a status, which is stored under `status@broadcast`. Rows
        // from before `quote_chat` was kept are matched by looking there too.
        let source = quote
            .chat
            .as_deref()
            .filter(|c| *c != STATUS_CHAT && *c != chat_id);
        let mut status = quote.chat.as_deref() == Some(STATUS_CHAT);
        // A cross-chat quote (reply privately) names the chat that stores the quoted message.
        let mut stored = if status {
            None
        } else {
            find(source.unwrap_or(chat_id))?
        };
        if stored.is_none() && source.is_some() {
            stored = find(chat_id)?;
        }
        if stored.is_none() && chat_id != STATUS_CHAT {
            stored = find(STATUS_CHAT)?;
            status |= stored.is_some();
        }
        let (from_me, sender_id, push_name, text) = match stored {
            Some((from_me, sender_id, push_name, kind, body, media_kind)) => (
                from_me,
                sender_id,
                push_name,
                preview(kind_from(&kind), &body, media_kind.as_deref()),
            ),
            None => (false, quote.sender, String::new(), quote.text),
        };
        let sender_name = if from_me {
            String::new()
        } else if sender_id.is_empty() {
            push_name
        } else {
            let who = self.resolve(&sender_id)?;
            match who.name {
                Some((name, _)) => name,
                None if !push_name.is_empty() => push_name,
                // Unnamed: show the phone number a privacy id maps to, not the id itself.
                None => who.phone.unwrap_or_else(|| fallback_name(&sender_id)),
            }
        };
        Ok(ReplyView {
            id: quote.id,
            from_me,
            sender_name,
            text,
            status,
            chat: source.map(str::to_string),
        })
    }

    /// The encoded media message of one message, for downloading its attachment.
    pub fn media_proto(&self, chat_id: &str, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
        Ok(self
            .conn
            .query_row(
                "SELECT media_proto FROM messages WHERE chat_id = ?1 AND id = ?2",
                params![chat_id, id],
                |r| r.get::<_, Option<Vec<u8>>>(0),
            )
            .optional()?
            .flatten())
    }

    /// What a message action needs about one stored message.
    pub fn message_target(
        &self,
        chat_id: &str,
        id: &str,
    ) -> rusqlite::Result<Option<MessageTarget>> {
        self.conn
            .query_row(
                "SELECT from_me, sender_id, body, media_proto, server_id FROM messages WHERE chat_id = ?1 AND id = ?2",
                params![chat_id, id],
                |r| {
                    Ok(MessageTarget {
                        from_me: r.get(0)?,
                        sender_id: r.get(1)?,
                        body: r.get(2)?,
                        media_proto: r.get(3)?,
                        server_id: r.get(4)?,
                    })
                },
            )
            .optional()
    }

    /// Replaces a stored message's text after an edit, keeping the text it had before.
    /// Returns the chat the message is stored under, if it is stored. An edit may name the
    /// chat by the other id (phone number or privacy id), so it falls back to the id alone.
    pub fn edit_message(
        &self,
        chat_id: &str,
        id: &str,
        body: &str,
        at: i64,
    ) -> rusqlite::Result<Option<String>> {
        let Some((chat_id, old)) = self.locate(chat_id, id)? else {
            return Ok(None);
        };
        if old != body {
            self.conn.execute(
                "INSERT INTO message_edits (chat_id, id, body, replaced_at) VALUES (?1, ?2, ?3, ?4)",
                params![chat_id, id, old, at],
            )?;
            self.conn.execute(
                "UPDATE messages SET body = ?3, edited_at = ?4 WHERE chat_id = ?1 AND id = ?2",
                params![chat_id, id, body, at],
            )?;
            self.refresh_preview(&chat_id, id)?;
        }
        Ok(Some(chat_id))
    }

    /// Marks a message deleted for everyone, keeping what it said. Returns the chat it is
    /// stored under, if it is stored.
    pub fn revoke_message(
        &self,
        chat_id: &str,
        id: &str,
        at: i64,
    ) -> rusqlite::Result<Option<String>> {
        let Some((chat_id, _)) = self.locate(chat_id, id)? else {
            return Ok(None);
        };
        self.conn.execute(
            "UPDATE messages SET revoked_at = ?3 WHERE chat_id = ?1 AND id = ?2 AND revoked_at IS NULL",
            params![chat_id, id, at],
        )?;
        self.refresh_preview(&chat_id, id)?;
        Ok(Some(chat_id))
    }

    /// The chat a message is stored under, and its text.
    pub(super) fn locate(
        &self,
        chat_id: &str,
        id: &str,
    ) -> rusqlite::Result<Option<(String, String)>> {
        let exact = self
            .conn
            .query_row(
                "SELECT chat_id, body FROM messages WHERE chat_id = ?1 AND id = ?2",
                params![chat_id, id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if exact.is_some() {
            return Ok(exact);
        }
        self.conn
            .query_row(
                "SELECT chat_id, body FROM messages WHERE id = ?1 LIMIT 1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
    }

    /// Rewrites the chat-list line when the message it shows was edited or deleted.
    pub(super) fn refresh_preview(&self, chat_id: &str, id: &str) -> rusqlite::Result<()> {
        let row = self
            .conn
            .query_row(
                "SELECT kind, body, media_kind, timestamp, revoked_at IS NOT NULL FROM messages
                 WHERE chat_id = ?1 AND id = ?2",
                params![chat_id, id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?,
                        r.get::<_, i64>(3)?,
                        r.get::<_, bool>(4)?,
                    ))
                },
            )
            .optional()?;
        let Some((kind, body, media_kind, timestamp, revoked)) = row else {
            return Ok(());
        };
        let text = if revoked {
            "🚫 This message was deleted".to_string()
        } else {
            preview(kind_from(&kind), &body, media_kind.as_deref())
        };
        self.conn.execute(
            "UPDATE chats SET last_text = ?2 WHERE id = ?1 AND last_timestamp = ?3",
            params![chat_id, text, timestamp],
        )?;
        Ok(())
    }

    /// Earlier texts of a chat's edited messages, by message id, oldest first.
    pub(super) fn edits(&self, chat_id: &str) -> rusqlite::Result<HashMap<String, Vec<EditView>>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, body, replaced_at FROM message_edits WHERE chat_id = ?1 ORDER BY replaced_at ASC",
        )?;
        let rows = stmt.query_map(params![chat_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                EditView {
                    body: r.get(1)?,
                    replaced_at: r.get(2)?,
                },
            ))
        })?;
        let mut out: HashMap<String, Vec<EditView>> = HashMap::new();
        for row in rows {
            let (id, edit) = row?;
            out.entry(id).or_default().push(edit);
        }
        Ok(out)
    }

    /// The oldest stored message of a chat: the anchor for asking the phone for more.
    pub fn oldest_message(&self, chat_id: &str) -> rusqlite::Result<Option<(String, bool, i64)>> {
        self.conn
            .query_row(
                "SELECT id, from_me, timestamp FROM messages WHERE chat_id = ?1 ORDER BY timestamp ASC LIMIT 1",
                params![chat_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
    }
}
