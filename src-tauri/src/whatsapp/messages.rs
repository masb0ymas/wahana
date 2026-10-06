//! Sending and acting on messages: text, reactions, edits, deletes, pins, forwards, receipts and typing.

use super::*;

/// The reply context quoting stored message `quote_id` of `quote_chat` — the chat that
/// stores it, which is `chat_id` except for a status reply (the story lives in
/// `status@broadcast` while the reply goes to the poster). `to` is the target chat.
pub(super) fn quote_context(
    account: &WaAccount,
    quote_chat: &str,
    to: &Jid,
    quote_id: &str,
) -> Result<wa::ContextInfo, String> {
    let target = account
        .db
        .lock()
        .unwrap()
        .message_target(quote_chat, quote_id)
        .map_err(|e| e.to_string())?
        .ok_or("quoted message not found")?;
    let sender = message_author(account, quote_chat, &target)
        .parse::<Jid>()
        .map_err(|_| "invalid quoted sender".to_string())?;
    let quoted = stored_message(&target)?;
    let quoted_chat_jid = quote_chat
        .parse::<Jid>()
        .map_err(|_| format!("invalid quoted chat id: {quote_chat}"))?;
    Ok(
        whatsapp_rust::wacore::proto_helpers::build_quote_context_with_info(
            quote_id,
            &sender,
            &quoted_chat_jid,
            to,
            &quoted,
        ),
    )
}

/// Sends a text message and records it in the chat, since the server does not echo a
/// device's own sends back to it. When `quote_id` is set, the text quotes that stored
/// message (Reply); `quote_chat` names the chat that stores the quoted message when it
/// differs from `chat_id` (a status reply quotes the story in `status@broadcast` while
/// sending to the poster). `mentions` lists the jids the text tags (group @mention).
#[tauri::command]
#[allow(clippy::too_many_arguments)] // each argument is a field of the JS invoke payload
pub async fn wa_native_send_text(
    app: AppHandle,
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
    text: String,
    quote_id: Option<String>,
    mentions: Option<Vec<String>>,
    quote_chat: Option<String>,
) -> Result<(), String> {
    let account = state.get(&id)?;
    let (client, generation, sender_name) = {
        let inner = account.inner.lock().unwrap();
        let client = inner
            .client
            .clone()
            .ok_or("WhatsApp account is not running")?;
        let sender_name = inner
            .me
            .as_ref()
            .map(|me| me.push_name.clone())
            .unwrap_or_default();
        (client, inner.generation, sender_name)
    };
    let to: Jid = chat_id
        .parse()
        .map_err(|_| format!("invalid chat id: {chat_id}"))?;
    let outgoing = {
        let mut context = match &quote_id {
            Some(qid) => quote_context(
                &account,
                quote_chat.as_deref().unwrap_or(&chat_id),
                &to,
                qid,
            )?,
            None => wa::ContextInfo::default(),
        };
        if let Some(list) = mentions {
            context.mentioned_jid = list;
        }
        if quote_id.is_some() || !context.mentioned_jid.is_empty() {
            wa::Message::text_with_context(text.clone(), context)
        } else {
            wa::Message::text(text.clone())
        }
    };
    let sent = client
        .send_message(to, outgoing)
        .await
        .map_err(|e| e.to_string())?;
    let message = IncomingMessage {
        view: MessageView {
            id: sent.message_id,
            chat_id,
            from_me: true,
            sender_name,
            sender_phone: None,
            sender_id: None,
            kind: MessageKind::Text,
            body: text,
            timestamp: now_millis(),
            media: None,
            ack: ACK_SENT,
            revoked_at: None,
            edited_at: None,
            edits: Vec::new(),
            channel_reactions: Vec::new(),
            reply_to: None,
            status_mention: None,
            album_id: None,
            preview: None,
            interactive: None,
        },
        sender_id: String::new(),
        media: None,
        quote: quote_id.map(|qid| match quote_chat {
            Some(chat) => QuoteRef {
                id: qid,
                sender: String::new(),
                text: String::new(),
                chat: Some(chat),
            },
            None => QuoteRef::by_id(&qid),
        }),
        album: None,
        poll_key: None,
    };
    record_message(&app, &account, generation, message);
    Ok(())
}

/// Reacts to (or, with an empty `emoji`, un-reacts from) a stored message.
#[tauri::command]
pub async fn wa_native_react(
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
    message_id: String,
    emoji: String,
) -> Result<(), String> {
    let account = state.get(&id)?;
    let client = {
        let inner = account.inner.lock().unwrap();
        inner
            .client
            .clone()
            .ok_or("WhatsApp account is not running")?
    };
    let target = account
        .db
        .lock()
        .unwrap()
        .message_target(&chat_id, &message_id)
        .map_err(|e| e.to_string())?
        .ok_or("message not found")?;
    let to: Jid = chat_id
        .parse()
        .map_err(|_| format!("invalid chat id: {chat_id}"))?;
    if chat_id.ends_with("@newsletter") {
        let server_id = target
            .server_id
            .ok_or("this channel message can't be reacted to (no server id; only messages received while connected can)")?;
        return client
            .newsletter()
            .send_reaction(&to, server_id as u64, &emoji)
            .await
            .map_err(|e| e.to_string());
    }
    let key = target_key(&account, &chat_id, &message_id, &target);
    client
        .send_reaction(to, key, &emoji)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Edits one of our own messages and updates the stored copy.
#[tauri::command]
pub async fn wa_native_edit(
    app: AppHandle,
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
    message_id: String,
    text: String,
) -> Result<(), String> {
    let account = state.get(&id)?;
    let client = {
        let inner = account.inner.lock().unwrap();
        inner
            .client
            .clone()
            .ok_or("WhatsApp account is not running")?
    };
    let to: Jid = chat_id
        .parse()
        .map_err(|_| format!("invalid chat id: {chat_id}"))?;
    if chat_id.ends_with("@newsletter") {
        client
            .newsletter()
            .edit_message(&to, message_id.clone(), wa::Message::text(text.clone()))
            .await
            .map_err(|e| e.to_string())?;
    } else {
        client
            .edit_message(to, message_id.clone(), wa::Message::text(text.clone()))
            .await
            .map_err(|e| e.to_string())?;
    }
    let updated = account
        .db
        .lock()
        .unwrap()
        .edit_message(&chat_id, &message_id, &text, now_millis())
        .map_err(|e| e.to_string())?;
    if updated.is_some() {
        emit_chats(&app, &account);
    }
    Ok(())
}

/// Deletes a message for everyone (revoke): our own, or, as a group admin, someone else's.
#[tauri::command]
pub async fn wa_native_delete(
    app: AppHandle,
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
    message_id: String,
) -> Result<(), String> {
    let account = state.get(&id)?;
    let client = {
        let inner = account.inner.lock().unwrap();
        inner
            .client
            .clone()
            .ok_or("WhatsApp account is not running")?
    };
    let to: Jid = chat_id
        .parse()
        .map_err(|_| format!("invalid chat id: {chat_id}"))?;
    if chat_id.ends_with("@newsletter") {
        client
            .newsletter()
            .revoke_message(&to, message_id.clone())
            .await
            .map_err(|e| e.to_string())?;
    } else {
        let target = account
            .db
            .lock()
            .unwrap()
            .message_target(&chat_id, &message_id)
            .map_err(|e| e.to_string())?;
        let revoke_type = match target {
            Some(t) if !t.from_me && !t.sender_id.is_empty() => RevokeType::Admin {
                original_sender: t
                    .sender_id
                    .parse()
                    .map_err(|_| format!("invalid sender id: {}", t.sender_id))?,
            },
            _ => RevokeType::Sender,
        };
        client
            .revoke_message(to, message_id.clone(), revoke_type)
            .await
            .map_err(|e| e.to_string())?;
    }
    let stored = account
        .db
        .lock()
        .unwrap()
        .revoke_message(&chat_id, &message_id, now_millis())
        .map_err(|e| e.to_string())?;
    if stored.is_some() {
        emit_chats(&app, &account);
    }
    Ok(())
}

/// Removes a message from this device only.
#[tauri::command]
pub async fn wa_native_delete_local(
    app: AppHandle,
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
    message_id: String,
) -> Result<(), String> {
    let account = state.get(&id)?;
    account
        .db
        .lock()
        .unwrap()
        .delete_message(&chat_id, &message_id)
        .map_err(|e| e.to_string())?;
    emit_chats(&app, &account);
    Ok(())
}

/// Pins a message for everyone for `duration_secs` (24 hours, 7 days or 30 days; default 7 days)
/// or unpins it.
#[tauri::command]
pub async fn wa_native_pin_message(
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
    message_id: String,
    on: bool,
    duration_secs: Option<u32>,
) -> Result<(), String> {
    let duration = match duration_secs {
        None | Some(604_800) => PinDuration::Days7,
        Some(86_400) => PinDuration::Hours24,
        Some(2_592_000) => PinDuration::Days30,
        Some(other) => return Err(format!("unsupported pin duration: {other}s")),
    };
    let account = state.get(&id)?;
    let client = {
        let inner = account.inner.lock().unwrap();
        inner
            .client
            .clone()
            .ok_or("WhatsApp account is not running")?
    };
    let target = account
        .db
        .lock()
        .unwrap()
        .message_target(&chat_id, &message_id)
        .map_err(|e| e.to_string())?
        .ok_or("message not found")?;
    let key = target_key(&account, &chat_id, &message_id, &target);
    let to: Jid = chat_id
        .parse()
        .map_err(|_| format!("invalid chat id: {chat_id}"))?;
    let result = if on {
        client.pin_message(to, key, duration).await
    } else {
        client.unpin_message(to, key).await
    };
    result.map_err(|e| e.to_string())?;
    Ok(())
}

/// Forwards a stored message to another chat and records the copy in the target chat.
#[tauri::command]
pub async fn wa_native_forward(
    app: AppHandle,
    state: State<'_, WaState>,
    id: String,
    from_chat_id: String,
    message_id: String,
    to_chat_id: String,
) -> Result<(), String> {
    let account = state.get(&id)?;
    let (client, generation, sender_name) = {
        let inner = account.inner.lock().unwrap();
        let client = inner
            .client
            .clone()
            .ok_or("WhatsApp account is not running")?;
        let sender_name = inner
            .me
            .as_ref()
            .map(|me| me.push_name.clone())
            .unwrap_or_default();
        (client, inner.generation, sender_name)
    };
    let target = account
        .db
        .lock()
        .unwrap()
        .message_target(&from_chat_id, &message_id)
        .map_err(|e| e.to_string())?
        .ok_or("message not found")?;
    let message = stored_message(&target)?;
    let media = extract_media(&message);
    let content = message_content(&message);
    if matches!(content, Some((MessageKind::Poll, ..))) {
        return Err("Polls can't be forwarded".to_string());
    }
    // A location or contact keeps its card; anything else is media or text as before.
    let (kind, interactive) = match content {
        Some((kind @ (MessageKind::Location | MessageKind::Contact), _, _, interactive)) => {
            (kind, interactive)
        }
        _ if media.is_some() => (MessageKind::Media, None),
        _ => (MessageKind::Text, None),
    };
    let to: Jid = to_chat_id
        .parse()
        .map_err(|_| format!("invalid chat id: {to_chat_id}"))?;
    let sent = client
        .forward_message(to, &message)
        .await
        .map_err(|e| e.to_string())?;
    let view = MessageView {
        id: sent.message_id,
        chat_id: to_chat_id,
        from_me: true,
        sender_name,
        sender_phone: None,
        sender_id: None,
        kind,
        body: target.body,
        timestamp: now_millis(),
        media: None,
        ack: ACK_SENT,
        revoked_at: None,
        edited_at: None,
        edits: Vec::new(),
        channel_reactions: Vec::new(),
        reply_to: None,
        status_mention: None,
        album_id: None,
        preview: link_preview(&message),
        interactive,
    };
    record_message(
        &app,
        &account,
        generation,
        IncomingMessage {
            view,
            sender_id: String::new(),
            media,
            quote: None,
            album: None,
            poll_key: None,
        },
    );
    Ok(())
}

/// The newest `limit` stored messages of a chat, oldest first.
#[tauri::command]
pub fn wa_native_messages(
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
    limit: u32,
) -> Result<Vec<MessageView>, String> {
    let account = state.get(&id)?;
    let messages = account.db.lock().unwrap().messages(&chat_id, limit);
    messages.map_err(|e| e.to_string())
}

/// Asks the phone for messages older than the oldest one stored. They arrive later as an
/// on-demand history transfer, announced with `wa_native:chats`.
#[tauri::command]
pub async fn wa_native_load_older(
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
) -> Result<(), String> {
    let account = state.get(&id)?;
    let client = account
        .inner
        .lock()
        .unwrap()
        .client
        .clone()
        .ok_or("WhatsApp account is not running")?;
    let oldest = account.db.lock().unwrap().oldest_message(&chat_id);
    let (oldest_id, from_me, timestamp) = oldest
        .map_err(|e| e.to_string())?
        .ok_or("this chat has no messages to continue from")?;
    let jid: Jid = chat_id
        .parse()
        .map_err(|_| format!("invalid chat id: {chat_id}"))?;
    client
        .fetch_message_history(&jid, &oldest_id, from_me, timestamp, OLDER_PAGE)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Clears the local unread count (its own command; see `wa_native_send_receipt` for blue ticks).
#[tauri::command]
pub fn wa_native_mark_read(
    app: AppHandle,
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
) -> Result<(), String> {
    let account = state.get(&id)?;
    let result = account.db.lock().unwrap().mark_read(&chat_id);
    result.map_err(|e| e.to_string())?;
    emit_account(&app, &account);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageReceipt {
    pub id: String,
    pub name: String,
    /// "+62…" when the number is known; null when it is not.
    pub phone: Option<String>,
    pub delivered_at: Option<i64>,
    pub read_at: Option<i64>,
    pub played_at: Option<i64>,
}

/// Who got, read or played a message of mine, and when (as far as receipts were seen).
#[tauri::command]
pub fn wa_native_message_info(
    state: State<'_, WaState>,
    id: String,
    message_id: String,
) -> Result<Vec<MessageReceipt>, String> {
    let account = state.get(&id)?;
    let db = account.db.lock().unwrap();
    let rows = db.receipts_for(&message_id).map_err(|e| e.to_string())?;
    let mut out: Vec<MessageReceipt> = Vec::new();
    for (who, ack, at) in rows {
        // A status receipt can name the broadcast list instead of the viewer.
        if who.ends_with("@broadcast") {
            continue;
        }
        let i = match out.iter().position(|r| r.id == who) {
            Some(i) => i,
            None => {
                let w = db.who(&who).ok();
                let phone = w.as_ref().and_then(|w| w.phone.clone());
                let name = w
                    .as_ref()
                    .and_then(|w| w.name.clone())
                    .or_else(|| phone.clone())
                    .unwrap_or_else(|| format!("+{}", who.split('@').next().unwrap_or(&who)));
                out.push(MessageReceipt {
                    id: who.clone(),
                    name,
                    phone,
                    delivered_at: None,
                    read_at: None,
                    played_at: None,
                });
                out.len() - 1
            }
        };
        match ack {
            2 => out[i].delivered_at = Some(at),
            3 => out[i].read_at = Some(at),
            4 => out[i].played_at = Some(at),
            _ => {}
        }
    }
    Ok(out)
}

/// Tells WhatsApp the chat's newest incoming messages were read (blue ticks). Unlike
/// `wa_native_mark_read`, which only clears the local unread count. Best-effort.
#[tauri::command]
pub async fn wa_native_send_receipt(
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
) -> Result<(), String> {
    let account = state.get(&id)?;
    let client = account
        .inner
        .lock()
        .unwrap()
        .client
        .clone()
        .ok_or("WhatsApp account is not running")?;
    let jid: Jid = chat_id
        .parse()
        .map_err(|_| format!("invalid chat id: {chat_id}"))?;
    let incoming = account
        .db
        .lock()
        .unwrap()
        .incoming_ids(&chat_id, 100)
        .unwrap_or_default();
    if incoming.is_empty() {
        return Ok(());
    }
    if chat_id.ends_with("@g.us") {
        // A group receipt carries the original sender, so one call per participant.
        let mut by_sender: HashMap<String, Vec<String>> = HashMap::new();
        for (sender, message) in incoming {
            if !sender.is_empty() {
                by_sender.entry(sender).or_default().push(message);
            }
        }
        for (sender, messages) in by_sender {
            let Ok(sender) = sender.parse::<Jid>() else {
                continue;
            };
            let ids: Vec<&str> = messages.iter().map(String::as_str).collect();
            let _ = client.mark_as_read(&jid, Some(&sender), &ids).await;
        }
    } else {
        let ids: Vec<&str> = incoming.iter().map(|(_, m)| m.as_str()).collect();
        client
            .mark_as_read(&jid, None, &ids)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Sends (or stops) the "typing…" chat state. Best-effort: a dropped update is harmless.
#[tauri::command]
pub async fn wa_native_set_typing(
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
    on: bool,
) -> Result<(), String> {
    let account = state.get(&id)?;
    let client = account
        .inner
        .lock()
        .unwrap()
        .client
        .clone()
        .ok_or("WhatsApp account is not running")?;
    let jid: Jid = chat_id
        .parse()
        .map_err(|_| format!("invalid chat id: {chat_id}"))?;
    let result = if on {
        client.chatstate().send_composing(&jid).await
    } else {
        client.chatstate().send_paused(&jid).await
    };
    result.map_err(|e| e.to_string())
}

/// Starts or stops listening for typing in the open chat. WhatsApp only forwards chat
/// states to a client that is online and, for a direct chat, subscribed to the contact's
/// presence, so this marks the account available while a chat is open.
#[tauri::command]
pub async fn wa_native_watch_typing(
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
    on: bool,
) -> Result<(), String> {
    let account = state.get(&id)?;
    let jid: Jid = chat_id
        .parse()
        .map_err(|_| format!("invalid chat id: {chat_id}"))?;
    let (client, toggle) = {
        let mut inner = account.inner.lock().unwrap();
        let client = inner
            .client
            .clone()
            .ok_or("WhatsApp account is not running")?;
        let before = inner.typing_watchers;
        inner.typing_watchers = if on {
            before + 1
        } else {
            before.saturating_sub(1)
        };
        (client, (before == 0) != (inner.typing_watchers == 0))
    };
    let direct = !chat_id.ends_with("@g.us");
    let presence = client.presence();
    if on {
        if toggle {
            presence.set_available().await.map_err(|e| e.to_string())?;
        }
        if direct {
            presence.subscribe(jid).await.map_err(|e| e.to_string())?;
        }
    } else {
        if direct {
            let _ = presence.unsubscribe(&jid).await;
        }
        if toggle {
            presence
                .set_unavailable()
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
