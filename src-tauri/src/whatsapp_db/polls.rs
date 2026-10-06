//! Poll keys and the votes cast on them.

use super::*;

impl ChatDb {
    /// A stored poll, found under the chat it names or, failing that (a vote may name the chat
    /// by the other id), by its id alone.
    pub fn poll_target(&self, chat_id: &str, id: &str) -> rusqlite::Result<Option<PollTarget>> {
        let Some((chat_id, _)) = self.locate(chat_id, id)? else {
            return Ok(None);
        };
        self.conn
            .query_row(
                "SELECT from_me, sender_id, poll_secret, poll_creator, interactive
                 FROM messages WHERE chat_id = ?1 AND id = ?2 AND kind = 'poll'",
                params![chat_id, id],
                |r| {
                    let options = r
                        .get::<_, Option<String>>(4)?
                        .and_then(|s| serde_json::from_str::<Interactive>(&s).ok())
                        .and_then(|i| i.poll)
                        .map(|p| p.options)
                        .unwrap_or_default();
                    Ok(PollTarget {
                        chat_id: chat_id.clone(),
                        from_me: r.get(0)?,
                        sender_id: r.get(1)?,
                        secret: r.get(2)?,
                        creator: r.get(3)?,
                        options,
                    })
                },
            )
            .optional()
    }

    /// Records someone's choice on a poll (`me` for our own), replacing an earlier one unless
    /// it is newer than this. An empty choice is a withdrawn vote. A voter is kept under their
    /// privacy id when it is known, so a vote cast under either id replaces the other.
    /// Returns whether anything changed.
    pub fn set_poll_vote(
        &self,
        poll_id: &str,
        voter: &str,
        options: &[String],
        at: i64,
    ) -> rusqlite::Result<bool> {
        let voter = match voter {
            "me" => voter.to_string(),
            _ => self.lid_for(voter)?.unwrap_or_else(|| voter.to_string()),
        };
        let options = serde_json::to_string(options).unwrap_or_else(|_| "[]".into());
        let changed = self.conn.execute(
            "INSERT INTO poll_votes (poll_id, voter, options, at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (poll_id, voter) DO UPDATE SET options = excluded.options, at = excluded.at
             WHERE excluded.at >= poll_votes.at",
            params![poll_id, voter, options, at],
        )?;
        Ok(changed > 0)
    }

    /// Who picked what on a poll, by option in the poll's order.
    pub(super) fn poll_results(
        &self,
        poll_id: &str,
        options: &[String],
        can_vote: bool,
    ) -> rusqlite::Result<PollResults> {
        let mut stmt = self
            .conn
            .prepare("SELECT voter, options FROM poll_votes WHERE poll_id = ?1 ORDER BY at")?;
        let rows = stmt.query_map(params![poll_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut results = PollResults {
            voters: vec![Vec::new(); options.len()],
            mine: Vec::new(),
            can_vote,
        };
        for row in rows {
            let (voter, picked) = row?;
            let picked: Vec<String> = serde_json::from_str(&picked).unwrap_or_default();
            if voter == "me" {
                results.mine = picked;
                continue;
            }
            let name = {
                let Resolved { name, phone } = self.resolve(&voter)?;
                name.map(|(n, _)| n)
                    .or(phone)
                    .unwrap_or_else(|| fallback_name(&voter))
            };
            for option in &picked {
                if let Some(i) = options.iter().position(|o| o == option) {
                    results.voters[i].push(name.clone());
                }
            }
        }
        Ok(results)
    }
}
