//! The history transfer the phone sends after pairing.

use super::*;

/// A history message's delivery state, as the phone last knew it.
pub(super) fn history_ack(info: &wa::WebMessageInfo) -> u8 {
    use wa::web_message_info::Status;
    match info.status {
        Some(Status::DeliveryAck) => ACK_DELIVERED,
        Some(Status::Read) => ACK_READ,
        Some(Status::Played) => ACK_PLAYED,
        Some(Status::Error) | Some(Status::Pending) => 0,
        _ => ACK_SENT,
    }
}

/// Converts one message from a history transfer.
pub(super) fn history_message(chat_id: &str, info: &wa::WebMessageInfo) -> Option<IncomingMessage> {
    let key = info.key.as_option()?;
    let id = key.id.clone()?;
    let (kind, body, media, interactive) = message_content(info.message.as_option()?)?;
    let from_me = key.from_me.unwrap_or(false);
    let sender_id = if from_me {
        String::new()
    } else {
        bare_jid(
            key.participant
                .as_deref()
                .or(info.participant.as_deref())
                .unwrap_or(chat_id),
        )
    };
    Some(IncomingMessage {
        view: MessageView {
            id,
            chat_id: chat_id.to_string(),
            from_me,
            sender_name: info.push_name.clone().unwrap_or_default(),
            sender_phone: None,
            sender_id: None,
            kind,
            body,
            timestamp: info.message_timestamp.unwrap_or_default() as i64 * 1000,
            media: None,
            ack: history_ack(info),
            revoked_at: None,
            edited_at: None,
            edits: Vec::new(),
            channel_reactions: Vec::new(),
            reply_to: None,
            status_mention: info.message.as_option().and_then(status_mention_of),
            album_id: None,
            preview: info.message.as_option().and_then(link_preview),
            interactive,
        },
        sender_id,
        media,
        quote: info.message.as_option().and_then(quote_of),
        album: info.message.as_option().and_then(album_of),
        poll_key: history_poll_key(info),
    })
}

/// The latest pin state for one message, read out of history.
pub(super) struct HistoryPin {
    pub(super) chat_id: String,
    pub(super) message_id: String,
    pub(super) on: bool,
    /// When the pin or unpin happened (unix ms); the newest one wins.
    pub(super) at: i64,
    pub(super) secs: u32,
}

/// Pins and unpins a history entry carries. The phone sends them three ways: as a pin
/// notice (`pin_in_chat`), as a plain pin message, or as an add-on on the pinned
/// message itself.
pub(super) fn history_pins(chat_id: &str, info: &wa::WebMessageInfo) -> Vec<HistoryPin> {
    const DEFAULT_SECS: u32 = 7 * 24 * 3600;
    let sent_at = info.message_timestamp.unwrap_or_default() as i64 * 1000;
    let mut pins = Vec::new();
    let mut push = |message_id: Option<&String>, on: bool, at: Option<i64>, secs: Option<u32>| {
        if let Some(message_id) = message_id.filter(|id| !id.is_empty()) {
            pins.push(HistoryPin {
                chat_id: chat_id.to_string(),
                message_id: message_id.clone(),
                on,
                at: at.filter(|ms| *ms > 0).unwrap_or(sent_at),
                secs: secs.filter(|s| *s > 0).unwrap_or(DEFAULT_SECS),
            });
        }
    };
    if let Some(pin) = info.pin_in_chat.as_option() {
        push(
            pin.key.as_option().and_then(|k| k.id.as_ref()),
            pin.r#type == Some(wa::pin_in_chat::Type::PinForAll),
            pin.sender_timestamp_ms.or(pin.server_timestamp_ms),
            pin.message_add_on_context_info
                .as_option()
                .and_then(|c| c.message_add_on_duration_in_secs),
        );
    }
    if let Some(message) = info.message.as_option() {
        if let Some(pin) = message.pin_in_chat_message.as_option() {
            push(
                pin.key.as_option().and_then(|k| k.id.as_ref()),
                pin.r#type == Some(wa::message::pin_in_chat_message::Type::PinForAll),
                pin.sender_timestamp_ms,
                message
                    .message_context_info
                    .as_option()
                    .and_then(|c| c.message_add_on_duration_in_secs),
            );
        }
    }
    let own_id = info.key.as_option().and_then(|k| k.id.as_ref());
    for add_on in &info.message_add_ons {
        if add_on.message_add_on_type != Some(wa::message_add_on::MessageAddOnType::PinInChat) {
            continue;
        }
        let Some(pin) = add_on
            .message_add_on
            .as_option()
            .and_then(|m| m.pin_in_chat_message.as_option())
        else {
            continue;
        };
        push(
            own_id,
            pin.r#type == Some(wa::message::pin_in_chat_message::Type::PinForAll),
            add_on.sender_timestamp_ms.or(add_on.server_timestamp_ms),
            add_on
                .add_on_context_info
                .as_option()
                .and_then(|c| c.message_add_on_duration_in_secs),
        );
    }
    pins
}

/// Writes a decoded history chunk: conversations with their unread counts, their
/// messages, and the push names that came with them. Returns the pins it found, newest
/// state per message.
pub(super) fn store_history(
    db: &ChatDb,
    sync: &wa::HistorySync,
) -> rusqlite::Result<Vec<HistoryPin>> {
    let mut pins: HashMap<(String, String), HistoryPin> = HashMap::new();
    for mapping in &sync.phone_number_to_lid_mappings {
        if let (Some(pn), Some(lid)) = (&mapping.pn_jid, &mapping.lid_jid) {
            db.set_lid_pn(&bare_jid(lid), &bare_jid(pn))?;
        }
    }
    for pushname in &sync.pushnames {
        if let (Some(id), Some(name)) = (&pushname.id, &pushname.pushname) {
            db.set_name(&bare_jid(id), name, NameSource::PushName)?;
        }
    }
    for conversation in &sync.conversations {
        let chat_id = conversation.id.as_str();
        if chat_id.is_empty() || chat_id == "status@broadcast" {
            continue;
        }
        let name = conversation
            .name
            .as_deref()
            .or(conversation.display_name.as_deref())
            .unwrap_or_default();
        db.set_name(chat_id, name, NameSource::History)?;
        if let (Some(pn), Some(lid)) = (&conversation.pn_jid, &conversation.lid_jid) {
            db.set_lid_pn(&bare_jid(lid), &bare_jid(pn))?;
        } else if let Some(pn) = &conversation.pn_jid {
            db.set_lid_pn(chat_id, &bare_jid(pn))?;
        }
        // The phone reports each conversation's last activity as Unix seconds. A corrupt
        // value far in the future would pin the chat to the top and — because a preview
        // only ever moves forward — freeze its last message for good, so drop times that
        // are not plausibly in the past. The chat's own messages set the real time below.
        let now = (now_millis() / 1000).max(0) as u64;
        let timestamp = conversation
            .conversation_timestamp
            .or(conversation.last_msg_timestamp)
            .filter(|secs| *secs > 0 && *secs <= now + 86_400)
            .map(|secs| secs as i64 * 1000)
            .unwrap_or_default();
        db.ensure_chat(chat_id, timestamp, conversation.unread_count.unwrap_or(0))?;
        // `mute_end_time` is in seconds; the phone uses a far-future value for "always".
        if let Some(end) = conversation.mute_end_time.filter(|e| *e > 0) {
            let until = if end > 4_000_000_000 {
                -1
            } else {
                (end as i64) * 1000
            };
            db.set_mute(&bare_jid(chat_id), until)?;
        }
        for entry in &conversation.messages {
            let Some(info) = entry.message.as_option() else {
                continue;
            };
            if let Some(message) = history_message(chat_id, info) {
                db.insert_message(&message, false)?;
                store_history_votes(db, &message, info)?;
            }
            for pin in history_pins(chat_id, info) {
                let key = (pin.chat_id.clone(), pin.message_id.clone());
                if pins.get(&key).is_none_or(|seen| seen.at <= pin.at) {
                    pins.insert(key, pin);
                }
            }
        }
    }
    Ok(pins.into_values().collect())
}
