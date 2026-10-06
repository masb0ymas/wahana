//! Per-account chat history for the native WhatsApp client.
//!
//! The protocol store (`<id>.db`, owned by `whatsapp-rust`) keeps keys and sessions, not
//! messages: WhatsApp sends a linked device its history once, right after pairing, and
//! after that only new traffic. This database (`<id>.chats.db`) is where that history and
//! everything since is kept, so chats survive restarts.

use std::collections::HashMap;
use std::path::Path;

use base64::Engine as _;
use rusqlite::{params, Connection, OptionalExtension};

use crate::whatsapp::{
    ChannelReaction, ChatInfo, EditView, Interactive, MediaInfo, MessageKind, MessageView,
    PollResults, PreviewInfo, ReplyView,
};

mod channels;
mod chats;
mod contacts;
mod labels;
mod messages;
mod polls;
mod status;
#[cfg(test)]
mod tests;

/// Bumped with every schema change; `open` migrates older files up to it.
const SCHEMA_VERSION: i64 = 18;
const STATUS_CHAT: &str = "status@broadcast";

/// How much a name source is trusted. A name only replaces one from an equal or lower
/// source, so a push name never overwrites a contact's saved name.
#[derive(Clone, Copy)]
pub enum NameSource {
    PushName = 1,
    History = 2,
    Contact = 3,
    GroupSubject = 4,
}

pub struct ChatDb {
    conn: Connection,
}

/// A media attachment as stored: what the bubble needs to draw it, plus the encoded
/// media message, which holds the keys to download and decrypt it later.
pub struct StoredMedia {
    pub kind: &'static str,
    pub mimetype: String,
    pub file_name: Option<String>,
    pub size: Option<u64>,
    pub seconds: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub thumbnail: Option<Vec<u8>>,
    pub proto: Vec<u8>,
}

impl StoredMedia {
    pub fn info(&self) -> MediaInfo {
        MediaInfo {
            kind: self.kind.to_string(),
            mimetype: self.mimetype.clone(),
            file_name: self.file_name.clone(),
            size: self.size,
            seconds: self.seconds,
            width: self.width,
            height: self.height,
            thumbnail: self.thumbnail.as_deref().map(thumbnail_url),
        }
    }
}

/// A message plus the bare id of its sender (empty for your own), whose push name it
/// also records.
pub struct IncomingMessage {
    pub view: MessageView,
    pub sender_id: String,
    pub media: Option<StoredMedia>,
    pub quote: Option<QuoteRef>,
    pub album: Option<String>,
    /// The key of a poll this message creates, when it carries one.
    pub poll_key: Option<PollKey>,
}

/// What it takes to vote on a poll and to open the votes cast on it: the secret its creation
/// message carried and, when the message said, the creator's id in the namespace (phone
/// number or privacy id) the chat used for it.
pub struct PollKey {
    pub secret: Vec<u8>,
    pub creator: Option<String>,
}

/// A stored poll, as voting and opening votes need it.
pub struct PollTarget {
    /// The chat the poll is stored under.
    pub chat_id: String,
    pub from_me: bool,
    pub sender_id: String,
    pub secret: Option<Vec<u8>>,
    pub creator: Option<String>,
    pub options: Vec<String>,
}

/// The message a reply quotes. `sender` and `text` are what the reply itself carried, used
/// when the quoted message isn't stored here.
pub struct QuoteRef {
    pub id: String,
    pub sender: String,
    pub text: String,
    /// The chat the quoted message lives in, when the reply says (`status@broadcast` for a story).
    pub chat: Option<String>,
}

impl QuoteRef {
    /// A quote of one of my own stored messages, resolved from the database when read.
    pub fn by_id(id: &str) -> Self {
        Self {
            id: id.to_string(),
            sender: String::new(),
            text: String::new(),
            chat: None,
        }
    }
}

/// What a message action (react, pin, quote, forward) needs about its target.
pub struct MessageTarget {
    pub from_me: bool,
    pub sender_id: String,
    pub body: String,
    /// The encoded media message, when the target has an attachment.
    pub media_proto: Option<Vec<u8>>,
    /// The server-assigned id of a channel message, when it was seen live.
    pub server_id: Option<i64>,
}

/// A chat label (WhatsApp "etiqueta"), as cached from app-state sync.
pub struct LabelRow {
    pub id: String,
    pub name: String,
    /// WhatsApp color index, not a hex value.
    pub color: i64,
}

/// Who someone is, for the contact and group info panels.
pub struct Who {
    /// Their name from your contacts, or failing that the name they gave themselves.
    pub name: Option<String>,
    /// Whether `name` is from your contacts.
    pub saved: bool,
    pub phone: Option<String>,
}

/// How a chat is labelled for the chat list and notifications: the saved contact name (or a
/// group's subject), else the phone number, else a fallback.
pub struct ChatLabel {
    pub name: String,
    /// Whether `name` came from your contacts (or the chat is a group).
    pub saved: bool,
    pub phone: Option<String>,
}

/// Who an id is, as far as this account knows.
#[derive(Default)]
struct Resolved {
    name: Option<(String, i64)>,
    phone: Option<String>,
}

impl ChatDb {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS chats (
                 id TEXT PRIMARY KEY,
                 fallback_name TEXT NOT NULL,
                 last_text TEXT NOT NULL DEFAULT '',
                 last_timestamp INTEGER NOT NULL DEFAULT 0,
                 last_from_me INTEGER NOT NULL DEFAULT 0,
                 last_sender_id TEXT NOT NULL DEFAULT '',
                 last_sender TEXT NOT NULL DEFAULT '',
                 unread INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS messages (
                 chat_id TEXT NOT NULL,
                 id TEXT NOT NULL,
                 from_me INTEGER NOT NULL,
                 sender_id TEXT NOT NULL,
                 sender_name TEXT NOT NULL,
                 kind TEXT NOT NULL,
                 body TEXT NOT NULL,
                 timestamp INTEGER NOT NULL,
                 PRIMARY KEY (chat_id, id)
             );
             CREATE INDEX IF NOT EXISTS messages_by_time ON messages (chat_id, timestamp);
             CREATE TABLE IF NOT EXISTS names (
                 id TEXT PRIMARY KEY,
                 name TEXT NOT NULL,
                 source INTEGER NOT NULL
             );",
        )?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version < 2 {
            migrate(
                &conn,
                "ALTER TABLE messages ADD COLUMN media_kind TEXT;
                 ALTER TABLE messages ADD COLUMN mimetype TEXT;
                 ALTER TABLE messages ADD COLUMN file_name TEXT;
                 ALTER TABLE messages ADD COLUMN file_size INTEGER;
                 ALTER TABLE messages ADD COLUMN seconds INTEGER;
                 ALTER TABLE messages ADD COLUMN width INTEGER;
                 ALTER TABLE messages ADD COLUMN height INTEGER;
                 ALTER TABLE messages ADD COLUMN thumbnail BLOB;
                 ALTER TABLE messages ADD COLUMN media_proto BLOB;
                 CREATE TABLE IF NOT EXISTS lid_pn (
                     lid TEXT PRIMARY KEY,
                     pn TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS lid_pn_by_pn ON lid_pn (pn);",
            )?;
        }
        if version < 3 {
            migrate(
                &conn,
                "CREATE TABLE IF NOT EXISTS labels (
                     id TEXT PRIMARY KEY,
                     name TEXT NOT NULL,
                     color INTEGER NOT NULL DEFAULT 0,
                     deleted INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE TABLE IF NOT EXISTS chat_labels (
                     label_id TEXT NOT NULL,
                     chat_id TEXT NOT NULL,
                     labeled INTEGER NOT NULL,
                     PRIMARY KEY (label_id, chat_id)
                 );
                 CREATE INDEX IF NOT EXISTS chat_labels_by_chat ON chat_labels (chat_id);",
            )?;
        }
        if version < 4 {
            // Messages stored before receipts were tracked count as sent.
            migrate(
                &conn,
                "ALTER TABLE messages ADD COLUMN ack INTEGER NOT NULL DEFAULT 1;
                 CREATE INDEX IF NOT EXISTS messages_by_id ON messages (id);",
            )?;
        }
        if version < 5 {
            // A message deleted for everyone keeps its content, marked with when it went;
            // an edited one keeps every earlier text in `message_edits`.
            migrate(
                &conn,
                "ALTER TABLE messages ADD COLUMN revoked_at INTEGER;
                 ALTER TABLE messages ADD COLUMN edited_at INTEGER;
                 CREATE TABLE IF NOT EXISTS message_edits (
                     chat_id TEXT NOT NULL,
                     id TEXT NOT NULL,
                     body TEXT NOT NULL,
                     replaced_at INTEGER NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS message_edits_by_msg ON message_edits (chat_id, id);",
            )?;
        }
        if version < 6 {
            // When each recipient got, read or played a message of mine (one row per
            // recipient and level). Only receipts seen from now on are recorded.
            migrate(
                &conn,
                "CREATE TABLE IF NOT EXISTS receipts (
                     message_id TEXT NOT NULL,
                     recipient TEXT NOT NULL,
                     ack INTEGER NOT NULL,
                     at INTEGER NOT NULL,
                     PRIMARY KEY (message_id, recipient, ack)
                 );",
            )?;
        }
        if version < 7 {
            // A channel message is reacted to by the id the server gave it, not its message id.
            migrate(&conn, "ALTER TABLE messages ADD COLUMN server_id INTEGER;")?;
        }
        if version < 8 {
            // Reaction totals on channel messages, as the server last reported them.
            migrate(
                &conn,
                "CREATE TABLE IF NOT EXISTS channel_reactions (
                     chat_id TEXT NOT NULL,
                     id TEXT NOT NULL,
                     emoji TEXT NOT NULL,
                     count INTEGER NOT NULL,
                     PRIMARY KEY (chat_id, id, emoji)
                 );",
            )?;
        }
        if version < 9 {
            migrate(
                &conn,
                "ALTER TABLE messages ADD COLUMN quote_id TEXT;
                 ALTER TABLE messages ADD COLUMN quote_sender TEXT;
                 ALTER TABLE messages ADD COLUMN quote_text TEXT;",
            )?;
        }
        if version < 10 {
            migrate(&conn, "ALTER TABLE messages ADD COLUMN album_id TEXT;")?;
        }
        if version < 11 {
            // The link preview WhatsApp embedded in a message: the first URL plus the title,
            // description and a small JPEG thumbnail, all copied from the message proto.
            migrate(
                &conn,
                "ALTER TABLE messages ADD COLUMN preview_url TEXT;
                 ALTER TABLE messages ADD COLUMN preview_title TEXT;
                 ALTER TABLE messages ADD COLUMN preview_description TEXT;
                 ALTER TABLE messages ADD COLUMN preview_image TEXT;",
            )?;
        }
        if version < 12 {
            // Chat mutes mirrored from the phone: `until` is epoch ms, -1 for good, 0 unmuted.
            migrate(
                &conn,
                "CREATE TABLE IF NOT EXISTS mutes (id TEXT PRIMARY KEY, until INTEGER NOT NULL);",
            )?;
        }
        if version < 13 {
            // The chat a quote came from when it isn't this one: `status@broadcast` for a
            // reply or reaction to a story.
            migrate(&conn, "ALTER TABLE messages ADD COLUMN quote_chat TEXT;")?;
        }
        if version < 14 {
            // A community's subgroups (including its announcement group) by parent group.
            migrate(&conn,
                "CREATE TABLE IF NOT EXISTS communities (id TEXT PRIMARY KEY, parent TEXT NOT NULL);",
            )?;
        }
        if version < 15 {
            // The status a story mention points at, so the bubble can open its story.
            migrate(
                &conn,
                "ALTER TABLE messages ADD COLUMN status_mention_id TEXT;",
            )?;
        }
        if version < 16 {
            // The structured content of a poll, location or contact message (JSON); `body`
            // keeps the same as plain text for the chat list, notifications and quotes.
            migrate(&conn, "ALTER TABLE messages ADD COLUMN interactive TEXT;")?;
        }
        if version < 17 {
            // Chat pins mirrored from the phone: `pinned_at` is epoch ms, 0 when unpinned there.
            migrate(
                &conn,
                "CREATE TABLE IF NOT EXISTS pins (id TEXT PRIMARY KEY, pinned_at INTEGER NOT NULL);",
            )?;
        }
        if version < 18 {
            // A poll's key (`poll_secret`, from the creation message) and who created it, both
            // needed to vote on it and to open the votes others cast. Votes are stored opened:
            // the latest choice per voter, as option names (JSON), with `me` for our own.
            migrate(
                &conn,
                "ALTER TABLE messages ADD COLUMN poll_secret BLOB;
                 ALTER TABLE messages ADD COLUMN poll_creator TEXT;
                 CREATE TABLE IF NOT EXISTS poll_votes (
                     poll_id TEXT NOT NULL,
                     voter TEXT NOT NULL,
                     options TEXT NOT NULL,
                     at INTEGER NOT NULL,
                     PRIMARY KEY (poll_id, voter)
                 );",
            )?;
        }
        // Never stamp a lower version: an older build sharing this file (a previous release,
        // a dev build) would otherwise make the next newer one re-run migrations it already
        // applied.
        if version < SCHEMA_VERSION {
            conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))?;
        }
        // Statuses are stored as messages under `status@broadcast`, not as a chat; drop any
        // row an earlier build created for it.
        conn.execute("DELETE FROM chats WHERE id = 'status@broadcast'", [])?;
        Self::repair_implausible_chats(&conn)?;
        Ok(Self { conn })
    }

    /// Rebuilds chats whose position a bad history timestamp corrupted. The phone can
    /// report a conversation time far in the future, which pins the chat to the top and —
    /// since a preview only moves forward — freezes its last message for good. Recompute
    /// the summary from the messages actually stored; a chat with none drops back to 0.
    fn repair_implausible_chats(conn: &Connection) -> rusqlite::Result<()> {
        let cap = now_millis() + FUTURE_SLACK_MS;
        let mut stmt = conn.prepare("SELECT id FROM chats WHERE last_timestamp > ?1")?;
        let ids: Vec<String> = stmt
            .query_map(params![cap], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        drop(stmt);
        for id in ids {
            let newest = conn
                .query_row(
                    "SELECT kind, body, media_kind, timestamp, from_me, sender_id, sender_name,
                            revoked_at IS NOT NULL
                     FROM messages WHERE chat_id = ?1 AND timestamp <= ?2
                     ORDER BY timestamp DESC LIMIT 1",
                    params![id, cap],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, Option<String>>(2)?,
                            r.get::<_, i64>(3)?,
                            r.get::<_, bool>(4)?,
                            r.get::<_, String>(5)?,
                            r.get::<_, String>(6)?,
                            r.get::<_, bool>(7)?,
                        ))
                    },
                )
                .optional()?;
            match newest {
                Some((
                    kind,
                    body,
                    media_kind,
                    timestamp,
                    from_me,
                    sender_id,
                    sender_name,
                    revoked,
                )) => {
                    let text = if revoked {
                        "🚫 This message was deleted".to_string()
                    } else {
                        preview(kind_from(&kind), &body, media_kind.as_deref())
                    };
                    conn.execute(
                        "UPDATE chats SET last_text = ?2, last_timestamp = ?3, last_from_me = ?4,
                             last_sender_id = ?5, last_sender = ?6 WHERE id = ?1",
                        params![id, text, timestamp, from_me, sender_id, sender_name],
                    )?;
                }
                None => {
                    conn.execute(
                        "UPDATE chats SET last_timestamp = 0 WHERE id = ?1",
                        params![id],
                    )?;
                }
            }
        }
        Ok(())
    }

    /// Forgets everything, for a device that was logged out and will pair afresh.
    pub fn clear(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            "DELETE FROM chats; DELETE FROM messages; DELETE FROM message_edits; DELETE FROM names; DELETE FROM lid_pn; DELETE FROM mutes; DELETE FROM pins; DELETE FROM poll_votes;",
        )
    }

    /// Runs `f` in one transaction: a history chunk holds thousands of rows.
    pub fn batch<T>(
        &mut self,
        f: impl FnOnce(&ChatDb) -> rusqlite::Result<T>,
    ) -> rusqlite::Result<T> {
        self.conn.execute_batch("BEGIN")?;
        match f(self) {
            Ok(value) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(value)
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }
}

/// Runs one schema step statement by statement, skipping columns that are already there.
/// `user_version` can lag the schema (an older build stamped it down, or a step was cut
/// short), and a step that fails on a column it added before would lock the account out.
fn migrate(conn: &Connection, sql: &str) -> rusqlite::Result<()> {
    for stmt in sql.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        match conn.execute_batch(stmt) {
            Err(rusqlite::Error::SqliteFailure(_, Some(msg)))
                if msg.starts_with("duplicate column name") => {}
            other => other?,
        }
    }
    Ok(())
}

/// A conversation timestamp this far past the local clock is the phone's corruption, not a
/// real time (see `ChatDb::repair_implausible_chats`).
const FUTURE_SLACK_MS: i64 = 24 * 60 * 60 * 1000;

/// Epoch milliseconds.
fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

/// The chat-list line for a message.
pub fn preview(kind: MessageKind, body: &str, media_kind: Option<&str>) -> String {
    let label = match media_kind {
        Some("image") => "📷 Photo",
        Some("video") => "🎥 Video",
        Some("ptt") => "🎤 Voice message",
        Some("audio") => "🎵 Audio",
        Some("document") => "📄 Document",
        Some("sticker") => "Sticker",
        _ => "📎 Media",
    };
    match kind {
        MessageKind::Text => body.to_string(),
        MessageKind::Media if body.is_empty() => label.to_string(),
        MessageKind::Media => format!("{} {body}", label.split(' ').next().unwrap_or("")),
        MessageKind::Poll => format!("📊 {}", body.lines().next().unwrap_or("Poll")),
        MessageKind::Location => "📍 Location".to_string(),
        MessageKind::Contact => "👤 Contact".to_string(),
        MessageKind::Unsupported => "Unsupported message".to_string(),
    }
}

fn thumbnail_url(bytes: &[u8]) -> String {
    format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// "+62…" from a phone-number id.
fn phone_of(pn: &str) -> String {
    let user = pn.split('@').next().unwrap_or(pn);
    format!("+{}", user.split(':').next().unwrap_or(user))
}

/// What a chat is called until a name is known.
fn fallback_name(id: &str) -> String {
    if id.ends_with("@s.whatsapp.net") {
        phone_of(id)
    } else {
        id.split('@').next().unwrap_or(id).to_string()
    }
}

fn kind_str(kind: MessageKind) -> &'static str {
    match kind {
        MessageKind::Text => "text",
        MessageKind::Media => "media",
        MessageKind::Poll => "poll",
        MessageKind::Location => "location",
        MessageKind::Contact => "contact",
        MessageKind::Unsupported => "unsupported",
    }
}

fn kind_from(kind: &str) -> MessageKind {
    match kind {
        "text" => MessageKind::Text,
        "media" => MessageKind::Media,
        "poll" => MessageKind::Poll,
        "location" => MessageKind::Location,
        "contact" => MessageKind::Contact,
        _ => MessageKind::Unsupported,
    }
}
