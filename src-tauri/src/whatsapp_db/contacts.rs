//! Who someone is: names by source, and the privacy id (LID) to phone number mapping.

use super::*;

impl ChatDb {
    pub fn set_name(&self, id: &str, name: &str, source: NameSource) -> rusqlite::Result<()> {
        if id.is_empty() || name.trim().is_empty() {
            return Ok(());
        }
        self.conn.execute(
            "INSERT INTO names (id, name, source) VALUES (?1, ?2, ?3)
             ON CONFLICT (id) DO UPDATE SET name = excluded.name, source = excluded.source
             WHERE excluded.source >= names.source",
            params![id, name.trim(), source as i64],
        )?;
        Ok(())
    }

    /// Records that a privacy id (`…@lid`) belongs to a phone number (`…@s.whatsapp.net`).
    pub fn set_lid_pn(&self, lid: &str, pn: &str) -> rusqlite::Result<()> {
        if !lid.ends_with("@lid") || !pn.ends_with("@s.whatsapp.net") {
            return Ok(());
        }
        self.conn.execute(
            "INSERT INTO lid_pn (lid, pn) VALUES (?1, ?2) ON CONFLICT (lid) DO UPDATE SET pn = excluded.pn",
            params![lid, pn],
        )?;
        Ok(())
    }

    /// Records which community a group belongs to (see the `communities` table); `None`
    /// forgets it.
    pub fn set_community(&self, id: &str, parent: Option<&str>) -> rusqlite::Result<()> {
        match parent {
            Some(parent) => self.conn.execute(
                "INSERT INTO communities (id, parent) VALUES (?1, ?2)
                 ON CONFLICT (id) DO UPDATE SET parent = excluded.parent",
                params![id, parent],
            )?,
            None => self
                .conn
                .execute("DELETE FROM communities WHERE id = ?1", params![id])?,
        };
        Ok(())
    }

    /// Direct chats on a privacy id whose phone number is not known yet.
    pub fn unmapped_lids(&self) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM chats WHERE id LIKE '%@lid' AND id NOT IN (SELECT lid FROM lid_pn)",
        )?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect()
    }

    /// The best known name and the phone number of a user or group id. A contact may be
    /// known under its phone number while its chat runs on its privacy id, or the other
    /// way round, so both sides of the mapping are consulted.
    pub(super) fn resolve(&self, id: &str) -> rusqlite::Result<Resolved> {
        let pn = if id.ends_with("@s.whatsapp.net") {
            Some(id.to_string())
        } else if id.ends_with("@lid") {
            self.conn
                .query_row("SELECT pn FROM lid_pn WHERE lid = ?1", params![id], |r| {
                    r.get::<_, String>(0)
                })
                .optional()?
        } else {
            None
        };
        let lid = if id.ends_with("@lid") {
            Some(id.to_string())
        } else if let Some(pn) = &pn {
            self.conn
                .query_row(
                    "SELECT lid FROM lid_pn WHERE pn = ?1 LIMIT 1",
                    params![pn],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
        } else {
            None
        };
        let mut best: Option<(String, i64)> = None;
        for candidate in [Some(id), pn.as_deref(), lid.as_deref()]
            .into_iter()
            .flatten()
        {
            let found = self
                .conn
                .query_row(
                    "SELECT name, source FROM names WHERE id = ?1",
                    params![candidate],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
                )
                .optional()?;
            if let Some(found) = found {
                if best.as_ref().is_none_or(|b| found.1 > b.1) {
                    best = Some(found);
                }
            }
        }
        Ok(Resolved {
            name: best,
            phone: pn.as_deref().map(phone_of),
        })
    }

    pub fn who(&self, id: &str) -> rusqlite::Result<Who> {
        let r = self.resolve(id)?;
        Ok(Who {
            saved: r
                .name
                .as_ref()
                .is_some_and(|(_, source)| *source >= NameSource::Contact as i64),
            name: r.name.map(|(n, _)| n),
            phone: r.phone,
        })
    }

    /// The phone-number id of a user, if known: itself, or what its privacy id maps to.
    pub fn pn_for(&self, id: &str) -> rusqlite::Result<Option<String>> {
        if id.ends_with("@s.whatsapp.net") {
            return Ok(Some(id.to_string()));
        }
        self.conn
            .query_row("SELECT pn FROM lid_pn WHERE lid = ?1", params![id], |r| {
                r.get(0)
            })
            .optional()
    }

    /// The privacy id of a user, if known: itself, or what its phone number maps to.
    pub fn lid_for(&self, id: &str) -> rusqlite::Result<Option<String>> {
        if id.ends_with("@lid") {
            return Ok(Some(id.to_string()));
        }
        self.conn
            .query_row(
                "SELECT lid FROM lid_pn WHERE pn = ?1 LIMIT 1",
                params![id],
                |r| r.get(0),
            )
            .optional()
    }
}
