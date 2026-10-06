//! Native WhatsApp accounts, backed by the `whatsapp-rust` multi-device client.
//!
//! This replaces the embedded WhatsApp Web child webviews: instead of rendering
//! `web.whatsapp.com` in a browser, the app speaks the protocol directly. Each account has
//! two SQLite files under the app's local data directory: `<id>.db`, the protocol session
//! owned by `whatsapp-rust`, and `<id>.chats.db`, the chat history (see `whatsapp_db`).
//!
//! History arrives from the phone right after pairing; older messages of a chat can be
//! asked for on demand. Media is downloaded on request and decrypted here. The open
//! direct chat subscribes to the contact's presence for typing and online / last seen.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{AppHandle, Emitter, Manager, State};
use whatsapp_rust::download::{Downloadable, MediaType};
use whatsapp_rust::features::{
    NewsletterMetadata, NewsletterRole, NewsletterVerification, ParticipantChangeResponse,
    PollVoteCiphertext,
};
use whatsapp_rust::prelude::*;
use whatsapp_rust::send::{PinDuration, RevokeType};
use whatsapp_rust::sync_task::MajorSyncTask;
use whatsapp_rust::types::events::{Event, EventKind};
use whatsapp_rust::wacore::appstate::patch_decode::WAPatchName;
use whatsapp_rust::waproto::buffa::{self, Message as _};
use whatsapp_rust::waproto::whatsapp as wa;
use whatsapp_rust_sqlite_storage::SqliteStore;

use crate::whatsapp_db::{
    preview, ChatDb, IncomingMessage, MessageTarget, NameSource, PollKey, PollTarget, QuoteRef,
    StoredMedia,
};

mod account;
mod channels;
mod chats;
mod content;
mod events;
mod groups;
mod history;
mod info;
mod labels;
mod media;
mod messages;
mod polls;
mod status;

pub use account::*;
pub use channels::*;
pub use chats::*;
use content::*;
use events::*;
pub use groups::*;
use history::*;
pub use info::*;
pub use labels::*;
pub use media::*;
pub use messages::*;
pub use polls::*;
pub use status::*;

/// The account list, next to the per-account session databases. The databases alone
/// cannot rebuild it: they hold no display name, and a half-written one is
/// indistinguishable from a paired one by its file name.
const ACCOUNTS_FILE: &str = "accounts.json";

/// Messages asked of the phone per "load older" request.
const OLDER_PAGE: i32 = 50;

/// How long a starting account may go without reaching a QR code or a connection before the
/// watchdog reports it as failed. A blocked network would otherwise look like an endless start.
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(40);

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WaStatus {
    #[default]
    Stopped,
    Starting,
    Qr,
    Working,
    LoggedOut,
    Failed,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountMe {
    pub id: String,
    pub push_name: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub id: String,
    pub name: String,
    pub status: WaStatus,
    pub me: Option<AccountMe>,
    pub error: Option<String>,
    /// Chats with unread messages, counted like WhatsApp's own badge.
    pub unread: u32,
    /// Percent of the history transfer from the phone, while one is running.
    pub syncing: Option<u32>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatInfo {
    pub id: String,
    pub name: String,
    pub last_text: String,
    pub last_timestamp: i64,
    pub last_from_me: bool,
    /// Delivery state of my newest message in the chat (see `MessageView::ack`).
    pub last_ack: u8,
    /// Name of whoever sent the last message, for "Name: text" group previews.
    pub last_sender: String,
    pub unread: u32,
    /// "+62…" for a direct chat whose number is known.
    pub phone: Option<String>,
    /// Named from your contacts (or a group); otherwise `name` is only what the other side
    /// calls themselves, and the number is what identifies them.
    pub saved: bool,
    /// What the phone says about muting: 0 = not muted, -1 = muted for good, otherwise the
    /// end time in epoch ms. `None` when nothing was ever recorded.
    pub muted_until: Option<i64>,
    /// What WhatsApp says about pinning: epoch ms of the pin, 0 when unpinned there. `None`
    /// when nothing was recorded (never pinned on WhatsApp, or pinned only in this app).
    pub pinned_at: Option<i64>,
    /// Name of the community this group belongs to, if any.
    pub community: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageView {
    pub id: String,
    pub chat_id: String,
    pub from_me: bool,
    pub sender_name: String,
    pub sender_phone: Option<String>,
    /// The sender's bare JID (`…@lid` or `…@s.whatsapp.net`) for a message from someone else.
    /// The same person's direct chat may run on their privacy id, so the UI needs the real id to
    /// open an existing chat instead of creating a second one under their phone number.
    pub sender_id: Option<String>,
    pub kind: MessageKind,
    /// The text, or a media message's caption.
    pub body: String,
    pub timestamp: i64,
    pub media: Option<MediaInfo>,
    /// Delivery state of a message I sent: 0 pending, 1 sent, 2 delivered, 3 read, 4 played.
    pub ack: u8,
    /// When it was deleted for everyone (unix ms). The content is kept to show what it was.
    pub revoked_at: Option<i64>,
    /// When it was last edited (unix ms).
    pub edited_at: Option<i64>,
    /// Earlier texts of an edited message, oldest first.
    pub edits: Vec<EditView>,
    /// Reaction totals on a channel message (the server reports counts, not who reacted).
    pub channel_reactions: Vec<ChannelReaction>,
    /// The message this one replies to.
    pub reply_to: Option<ReplyView>,
    /// The status a story mention points at: the mention carries no story of its own, just
    /// a reference, so the bubble opens the story viewer for this id.
    pub status_mention: Option<String>,
    /// The album this photo or video was sent in, shared by all its members.
    pub album_id: Option<String>,
    /// The link preview WhatsApp embedded in the message, when it has one.
    pub preview: Option<PreviewInfo>,
    /// The structured content of a poll, location or contact message; `body` carries the
    /// same as plain text for the chat list, notifications and quotes.
    pub interactive: Option<Interactive>,
}

/// A link preview as WhatsApp embedded it: the first URL in the text plus the title,
/// description and thumbnail its servers fetched. Absent for plain text and older messages.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewInfo {
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    /// The thumbnail as a data URL.
    pub image: Option<String>,
}

/// A quoted message as shown above a reply.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplyView {
    pub id: String,
    pub from_me: bool,
    pub sender_name: String,
    /// Its text, or a label for its attachment.
    pub text: String,
    /// It is a story (status), so opening it means the status viewer, not this chat.
    pub status: bool,
    /// The chat that stores the quoted message when it is not this one (a cross-chat reply such as
    /// "reply privately"); `None` when it lives in the same chat. Opening the quote goes there.
    pub chat: Option<String>,
}

/// How many times one emoji was used on a channel message.
#[derive(Clone, Serialize)]
pub struct ChannelReaction {
    pub emoji: String,
    pub count: u64,
}

/// One earlier text of an edited message.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditView {
    pub body: String,
    /// When this text was replaced (unix ms).
    pub replaced_at: i64,
}

/// `MessageView::ack` values.
const ACK_SENT: u8 = 1;

const ACK_DELIVERED: u8 = 2;

const ACK_READ: u8 = 3;

const ACK_PLAYED: u8 = 4;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    /// image, video, audio, ptt (voice note), document, or sticker.
    pub kind: String,
    pub mimetype: String,
    pub file_name: Option<String>,
    pub size: Option<u64>,
    pub seconds: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// The blurry inline preview, as a data URL.
    pub thumbnail: Option<String>,
}

/// The structured content of a poll, location or contact message. Only the part matching the
/// message's kind is set; the others stay absent.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Interactive {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll: Option<PollInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<LocationInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contacts: Option<Vec<ContactCard>>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PollInfo {
    pub question: String,
    pub options: Vec<String>,
    /// More than one answer may be picked.
    pub multiple: bool,
    /// The votes cast so far. Filled in when the poll is read back, never stored with it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub results: Option<PollResults>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PollResults {
    /// Who picked each option (display names), in the poll's option order. Our own choice is
    /// in `mine`, not here.
    pub voters: Vec<Vec<String>>,
    /// The options we picked.
    pub mine: Vec<String>,
    /// The poll's key is stored, so this client can vote on it and open others' votes. Polls
    /// stored before keys were kept have none.
    pub can_vote: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationInfo {
    pub latitude: f64,
    pub longitude: f64,
    pub name: Option<String>,
    pub address: Option<String>,
    /// A live location shared for a period rather than a fixed pin.
    pub live: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactCard {
    pub name: String,
    pub phone: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Text,
    Media,
    Poll,
    Location,
    Contact,
    /// A real message this client cannot show yet.
    Unsupported,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct QrPayload {
    id: String,
    code: String,
    timeout_ms: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatsPayload {
    id: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MessagesPayload {
    id: String,
    messages: Vec<MessageView>,
    /// How the chat is labelled, so a notification can name it like the chat list does:
    /// the saved contact name, else the phone number (a group's subject counts as a name).
    chat: Option<ChatLabelView>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatLabelView {
    name: String,
    saved: bool,
    phone: Option<String>,
}

/// Emitted with just an account id when that account's non-chat data changed (statuses, labels).
#[derive(Clone, Serialize)]
struct IdPayload {
    id: String,
}

/// An incoming reaction, so the bubble can show who reacted with what.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReactionPayload {
    id: String,
    message_id: String,
    from: String,
    from_me: bool,
    participant: Option<String>,
    text: String,
}

/// A message deleted for everyone, so the chat can draw the "deleted" tombstone in place.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RevokedPayload {
    id: String,
    chat_id: String,
    message_id: String,
    from_me: bool,
    participant: Option<String>,
    timestamp: i64,
}

/// A message pinned or unpinned for everyone (from any device), so the chat can mark it.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PinPayload {
    id: String,
    chat_id: String,
    message_id: String,
    on: bool,
    /// When the pin lapses (unix ms); 0 when unpinned.
    expires: i64,
}

/// A contact went online or offline. `last_seen` (unix ms) is only set when their privacy
/// settings share it; `chat_ids` holds the chat under both its phone-number and privacy id.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PresencePayload {
    id: String,
    chat_ids: Vec<String>,
    online: bool,
    last_seen: Option<i64>,
}

/// Someone started or stopped typing (or recording) in a chat. `chat_ids` holds the chat
/// under both its phone-number and privacy id, since the open chat may run on either.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TypingPayload {
    id: String,
    chat_ids: Vec<String>,
    sender: String,
    /// Best known name (or phone number) of the sender, for "Budi is typing" in a group.
    sender_name: Option<String>,
    /// "typing", "recording" or "paused".
    state: &'static str,
}

#[derive(Clone, Serialize, Deserialize)]
struct StoredAccount {
    id: String,
    name: String,
}

#[derive(Default)]
struct AccountInner {
    status: WaStatus,
    me: Option<AccountMe>,
    error: Option<String>,
    /// Claimed by `wa_native_start` before the client exists, so a second start is refused
    /// even while the first is still building. Released by stop, failure, or once a logged
    /// out client has finished shutting down.
    running: bool,
    /// Bumped by every start, stop, and logout. Each client's callbacks carry the
    /// generation they were started with and drop anything once it is no longer current,
    /// so a client that is still winding down cannot overwrite its successor's state.
    generation: u64,
    /// Set when the server logged this device out. The store still holds the revoked
    /// credentials, which would only be rejected again, so the next start pairs afresh.
    reset_session: bool,
    handle: Option<BotHandle>,
    client: Option<Arc<Client>>,
    /// Open chats watching for typing. The account stays online while any is open, so a
    /// chat switch (old chat off, new chat on, in either order) never leaves it offline.
    typing_watchers: usize,
    syncing: Option<u32>,
    /// Profile picture URLs by chat id; `None` once looked up and found to have none.
    pictures: HashMap<String, Option<String>>,
    /// Channel ids whose metadata was already asked for this run, so a failed lookup is not
    /// retried on every chat poll. Reset when `channels_gen` falls behind `generation`.
    channels_probed: HashSet<String>,
    channels_gen: u64,
}

pub struct WaAccount {
    id: String,
    name: Mutex<String>,
    inner: Mutex<AccountInner>,
    /// Locked after `inner` whenever both are needed, never the other way round.
    db: Mutex<ChatDb>,
    /// Serializes presence changes: switching chats closes one watch and opens the next at
    /// once, and an "unavailable" landing after the new "available" would leave the account
    /// offline, so the server stops sending the contact's presence.
    presence: tokio::sync::Mutex<()>,
}

impl WaAccount {
    fn open(app: &AppHandle, id: String, name: String) -> Result<Self, String> {
        let db = ChatDb::open(&chats_path(app, &id)?)
            .map_err(|e| format!("failed to open WhatsApp chat history: {e}"))?;
        Ok(Self {
            id,
            name: Mutex::new(name),
            inner: Mutex::new(AccountInner::default()),
            db: Mutex::new(db),
            presence: tokio::sync::Mutex::new(()),
        })
    }

    fn info(&self) -> AccountInfo {
        let inner = self.inner.lock().unwrap();
        AccountInfo {
            id: self.id.clone(),
            name: self.name.lock().unwrap().clone(),
            status: inner.status,
            me: inner.me.clone(),
            error: inner.error.clone(),
            unread: self.db.lock().unwrap().unread_chats(),
            syncing: inner.syncing,
        }
    }

    fn is_current(&self, generation: u64) -> bool {
        self.inner.lock().unwrap().generation == generation
    }
}

/// Accounts live in Tauri-managed state rather than a module static: unlike the webview
/// registry, this holds live connections that the commands must reach by name.
#[derive(Default)]
pub struct WaState {
    accounts: Mutex<HashMap<String, Arc<WaAccount>>>,
    /// Saved accounts whose chat history failed to open at launch. They are written back
    /// with the list, so one bad start never erases a paired session from it.
    unopened: Mutex<Vec<StoredAccount>>,
}

impl WaState {
    fn get(&self, id: &str) -> Result<Arc<WaAccount>, String> {
        self.accounts
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| format!("unknown WhatsApp account: {id}"))
    }
}

/// Account ids name the session's own database file, so they are validated rather than
/// trusted: 32 hex characters, exactly as the frontend generates them.
fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

fn whatsapp_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("whatsapp");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn session_path(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    Ok(whatsapp_dir(app)?.join(format!("{id}.db")))
}

fn chats_path(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    Ok(whatsapp_dir(app)?.join(format!("{id}.chats.db")))
}

fn remove_sqlite_files(db_path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let path = PathBuf::from(format!("{}{suffix}", db_path.to_string_lossy()));
        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn emit_account(app: &AppHandle, account: &WaAccount) {
    let _ = app.emit_to("main", "wa_native:account", account.info());
}

fn emit_chats(app: &AppHandle, account: &WaAccount) {
    let _ = app.emit_to(
        "main",
        "wa_native:chats",
        ChatsPayload {
            id: account.id.clone(),
        },
    );
}

fn set_error(app: &AppHandle, account: &WaAccount, generation: u64, message: String) {
    {
        let mut inner = account.inner.lock().unwrap();
        if inner.generation != generation {
            return;
        }
        inner.running = false;
        inner.status = WaStatus::Failed;
        inner.error = Some(message);
    }
    emit_account(app, account);
}

/// A user or group id without the device part ("628…:12@s.whatsapp.net" → "628…@s.whatsapp.net"),
/// so a sender's messages from different devices share one name.
fn bare_jid(id: &str) -> String {
    match id.split_once('@') {
        Some((user, server)) => format!("{}@{server}", user.split(':').next().unwrap_or(user)),
        None => id.to_string(),
    }
}

/// The bare JID that authored a stored message: our own id for our messages, otherwise
/// the recorded sender (or the chat itself for a direct message).
fn message_author(account: &WaAccount, chat_id: &str, target: &MessageTarget) -> String {
    if target.from_me {
        let me = account
            .inner
            .lock()
            .unwrap()
            .me
            .as_ref()
            .map(|me| me.id.clone())
            .unwrap_or_default();
        return bare_jid(&me);
    }
    if target.sender_id.is_empty() {
        return chat_id.to_string();
    }
    bare_jid(&target.sender_id)
}

/// The referential key of a stored message, for reactions and pins. Groups and status need
/// `participant` to attribute the action to the original sender.
fn target_key(
    account: &WaAccount,
    chat_id: &str,
    message_id: &str,
    target: &MessageTarget,
) -> wa::MessageKey {
    let participant = if chat_id.ends_with("@g.us") || chat_id == "status@broadcast" {
        Some(message_author(account, chat_id, target))
    } else {
        None
    };
    wa::MessageKey {
        remote_jid: Some(chat_id.to_string()),
        from_me: Some(target.from_me),
        id: Some(message_id.to_string()),
        participant,
    }
}

/// The message body (or, for media, the encoded media message) of a stored message, for
/// quoting and forwarding.
fn stored_message(target: &MessageTarget) -> Result<wa::Message, String> {
    match &target.media_proto {
        Some(bytes) => {
            let mut slice = bytes.as_slice();
            wa::Message::decode(&mut slice).map_err(|e| e.to_string())
        }
        None => Ok(wa::Message::text(target.body.clone())),
    }
}

/// Stores a live message, sent or received, and notifies the frontend.
fn record_message(
    app: &AppHandle,
    account: &Arc<WaAccount>,
    generation: u64,
    message: IncomingMessage,
) {
    if !account.is_current(generation) {
        return;
    }
    let inserted = match account.db.lock().unwrap().insert_message(&message, true) {
        Ok(inserted) => inserted,
        Err(e) => {
            eprintln!("failed to store WhatsApp message: {e}");
            return;
        }
    };
    if !inserted {
        return;
    }
    // Stories are stored but are not chat traffic: they get their own event and no notification.
    if message.view.chat_id == "status@broadcast" {
        let _ = app.emit_to(
            "main",
            "wa_native:status",
            IdPayload {
                id: account.id.clone(),
            },
        );
        emit_account(app, account);
        return;
    }
    let mut view = message.view;
    view.media = message.media.as_ref().map(StoredMedia::info);
    view.album_id = message.album.clone();
    let chat = account
        .db
        .lock()
        .unwrap()
        .chat_label(&view.chat_id)
        .ok()
        .map(|l| ChatLabelView {
            name: l.name,
            saved: l.saved,
            phone: l.phone,
        });
    let _ = app.emit_to(
        "main",
        "wa_native:messages",
        MessagesPayload {
            id: account.id.clone(),
            messages: vec![view],
            chat,
        },
    );
    emit_account(app, account);
}

/// Tells the frontend an account's labels changed, so the screens refetch them.
fn emit_labels(app: &AppHandle, account: &WaAccount) {
    let _ = app.emit_to(
        "main",
        "wa_native:labels",
        IdPayload {
            id: account.id.clone(),
        },
    );
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

fn running_client(account: &WaAccount) -> Result<Arc<Client>, String> {
    account
        .inner
        .lock()
        .unwrap()
        .client
        .clone()
        .ok_or_else(|| "WhatsApp account is not running".to_string())
}

#[cfg(test)]
mod tests {
    use super::{bare_jid, Jid};

    /// The device snapshot's `pn`/`lid` print with the device suffix (`628…:16@s.whatsapp.net`).
    /// The account's self id must be the bare JID: the profile-picture lookup is not answered for
    /// a device-scoped JID (so the tile fell back to initials), and the suffix leaked into the
    /// number shown in the UI.
    #[test]
    fn self_id_is_the_bare_jid() {
        // What `Device.pn` actually holds for a linked account: the raw Display keeps the device.
        let stored: Jid = "6287837554403:16@s.whatsapp.net".parse().unwrap();
        assert_eq!(stored.to_string(), "6287837554403:16@s.whatsapp.net");
        assert_eq!(
            bare_jid(&stored.to_string()),
            "6287837554403@s.whatsapp.net"
        );

        assert_eq!(bare_jid("155933300805837:16@lid"), "155933300805837@lid");
        // Already bare, and non-JID inputs, pass through.
        assert_eq!(
            bare_jid("6287837554403@s.whatsapp.net"),
            "6287837554403@s.whatsapp.net"
        );
        assert_eq!(bare_jid(""), "");
    }
}
