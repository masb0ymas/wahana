//! Statuses (stories), stored as messages under `status@broadcast`.

use super::*;

impl ChatDb {
    /// The newest `limit` statuses, newest first, each with the poster's bare id.
    pub fn statuses(&self, limit: u32) -> rusqlite::Result<Vec<(String, MessageView)>> {
        let mut stmt = self.conn.prepare(
            "SELECT sender_id, id, chat_id, from_me, sender_name, kind, body, timestamp,
                    media_kind, mimetype, file_name, file_size, seconds, width, height, thumbnail,
                    interactive
             FROM messages WHERE chat_id = 'status@broadcast' ORDER BY timestamp DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |r| {
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
            Ok((
                r.get::<_, String>(0)?,
                MessageView {
                    id: r.get(1)?,
                    chat_id: r.get(2)?,
                    from_me: r.get(3)?,
                    sender_name: r.get(4)?,
                    sender_phone: None,
                    sender_id: None,
                    kind: kind_from(&r.get::<_, String>(5)?),
                    body: r.get(6)?,
                    timestamp: r.get(7)?,
                    media: media.transpose()?,
                    ack: 0,
                    revoked_at: None,
                    edited_at: None,
                    edits: Vec::new(),
                    channel_reactions: Vec::new(),
                    reply_to: None,
                    status_mention: None,
                    album_id: None,
                    preview: None,
                    interactive: r
                        .get::<_, Option<String>>(16)?
                        .and_then(|s| serde_json::from_str::<Interactive>(&s).ok()),
                },
            ))
        })?;
        let mut statuses: Vec<(String, MessageView)> = rows.collect::<rusqlite::Result<_>>()?;
        // Prefer the name saved for the poster over the push name the status carried.
        let mut seen: HashMap<String, Resolved> = HashMap::new();
        for (sender, view) in &mut statuses {
            if view.from_me || sender.is_empty() {
                continue;
            }
            if !seen.contains_key(sender.as_str()) {
                seen.insert(sender.clone(), self.resolve(sender)?);
            }
            let who = &seen[sender.as_str()];
            if let Some((name, _)) = &who.name {
                view.sender_name = name.clone();
            }
            if view.sender_name.is_empty() {
                view.sender_name = who.phone.clone().unwrap_or_default();
            }
            view.sender_phone = who.phone.clone();
        }
        Ok(statuses)
    }

    /// Phone-number ids of saved contacts, deduped and without `me`: who a status is posted to.
    pub fn status_recipients(&self, me: Option<&str>) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM names WHERE source >= ?1")?;
        let rows = stmt.query_map(params![NameSource::Contact as i64], |r| {
            r.get::<_, String>(0)
        })?;
        let mut out: Vec<String> = Vec::new();
        for id in rows {
            let id = id?;
            let pn = if id.ends_with("@s.whatsapp.net") {
                Some(id)
            } else if id.ends_with("@lid") {
                self.pn_for(&id)?
            } else {
                None
            };
            if let Some(pn) = pn {
                if Some(pn.as_str()) != me {
                    out.push(pn);
                }
            }
        }
        out.sort();
        out.dedup();
        Ok(out)
    }
}
