//! Channels (newsletters): following, syncing and managing them.

use super::*;

/// Fetches a page of a channel's own history from the server: the latest messages, or with
/// `older` the page before the oldest one stored. Stores them with their server ids and
/// reaction totals, and returns how many the server sent (0 means the start of the channel).
#[tauri::command]
pub async fn wa_native_channel_sync(
    app: AppHandle,
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
    older: bool,
) -> Result<usize, String> {
    let account = state.get(&id)?;
    let (client, generation) = {
        let inner = account.inner.lock().unwrap();
        (
            inner
                .client
                .clone()
                .ok_or("WhatsApp account is not running")?,
            inner.generation,
        )
    };
    let jid: Jid = chat_id
        .parse()
        .map_err(|_| format!("invalid chat id: {chat_id}"))?;
    let before = if older {
        account
            .db
            .lock()
            .unwrap()
            .oldest_server_id(&chat_id)
            .map_err(|e| e.to_string())?
            .map(|id| id as u64)
    } else {
        None
    };
    let page = client
        .newsletter()
        .get_messages(jid, OLDER_PAGE as u32, before)
        .await
        .map_err(|e| e.to_string())?;
    if !account.is_current(generation) {
        return Ok(0);
    }
    let count = page.len();
    let result = account.db.lock().unwrap().batch(|db| {
        for m in &page {
            if m.message_id.is_empty() {
                continue;
            }
            let reactions: Vec<(String, u64)> = m
                .reactions
                .iter()
                .map(|r| (r.code.clone(), r.count))
                .collect();
            if let Some((kind, body, media, interactive)) =
                m.message.as_ref().and_then(message_content)
            {
                let message = IncomingMessage {
                    view: MessageView {
                        id: m.message_id.clone(),
                        chat_id: chat_id.clone(),
                        from_me: m.is_sender,
                        sender_name: String::new(),
                        sender_phone: None,
                        sender_id: None,
                        kind,
                        body,
                        timestamp: m.timestamp as i64 * 1000,
                        media: None,
                        ack: ACK_SENT,
                        revoked_at: None,
                        edited_at: None,
                        edits: Vec::new(),
                        channel_reactions: Vec::new(),
                        reply_to: None,
                        status_mention: m.message.as_ref().and_then(status_mention_of),
                        album_id: None,
                        preview: m.message.as_ref().and_then(link_preview),
                        interactive,
                    },
                    sender_id: String::new(),
                    media,
                    quote: None,
                    album: None,
                    poll_key: None,
                };
                db.insert_message(&message, false)?;
            }
            db.set_server_id(&chat_id, &m.message_id, m.server_id as i64)?;
            db.set_channel_reactions(&chat_id, &m.message_id, &reactions)?;
        }
        Ok(())
    });
    result.map_err(|e| e.to_string())?;
    emit_chats(&app, &account);
    Ok(count)
}

/// Follows a channel from its invite link (or bare code) and returns its chat id.
#[tauri::command]
pub async fn wa_native_channel_follow(
    app: AppHandle,
    state: State<'_, WaState>,
    id: String,
    invite: String,
) -> Result<String, String> {
    let account = state.get(&id)?;
    let client = running_client(&account)?;
    let code = invite
        .trim()
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string();
    if code.is_empty() {
        return Err("paste a channel link, like whatsapp.com/channel/…".into());
    }
    let found = crate::channel_mex::by_invite(&client, &code).await?;
    let meta = client
        .newsletter()
        .join(&found.jid)
        .await
        .map_err(|e| e.to_string())?;
    let chat_id = bare_jid(&meta.jid.to_string());
    {
        let db = account.db.lock().unwrap();
        db.set_name(&chat_id, &meta.name, NameSource::GroupSubject)
            .map_err(|e| e.to_string())?;
        db.ensure_chat(&chat_id, now_millis(), 0)
            .map_err(|e| e.to_string())?;
    }
    emit_account(&app, &account);
    emit_chats(&app, &account);
    Ok(chat_id)
}

/// Stops following a channel and drops its stored posts.
#[tauri::command]
pub async fn wa_native_channel_leave(
    app: AppHandle,
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
) -> Result<(), String> {
    let account = state.get(&id)?;
    let client = running_client(&account)?;
    let jid: Jid = chat_id
        .parse()
        .map_err(|_| format!("invalid chat id: {chat_id}"))?;
    client
        .newsletter()
        .leave(&jid)
        .await
        .map_err(|e| e.to_string())?;
    account
        .db
        .lock()
        .unwrap()
        .delete_chat(&chat_id)
        .map_err(|e| e.to_string())?;
    emit_account(&app, &account);
    emit_chats(&app, &account);
    Ok(())
}

/// Renames a channel or changes its description (owners and admins only).
#[tauri::command]
pub async fn wa_native_channel_update(
    app: AppHandle,
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
    name: Option<String>,
    description: Option<String>,
) -> Result<(), String> {
    let account = state.get(&id)?;
    let client = running_client(&account)?;
    let jid: Jid = chat_id
        .parse()
        .map_err(|_| format!("invalid chat id: {chat_id}"))?;
    let meta = client
        .newsletter()
        .update(&jid, name.as_deref(), description.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    let _ = account
        .db
        .lock()
        .unwrap()
        .set_name(&chat_id, &meta.name, NameSource::GroupSubject);
    emit_chats(&app, &account);
    Ok(())
}

/// Mutes or unmutes a channel's notifications on the account.
#[tauri::command]
pub async fn wa_native_channel_mute(
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
    muted: bool,
) -> Result<(), String> {
    let account = state.get(&id)?;
    let client = running_client(&account)?;
    let jid: Jid = chat_id
        .parse()
        .map_err(|_| format!("invalid chat id: {chat_id}"))?;
    client
        .newsletter()
        .set_follower_mute(&jid, muted)
        .await
        .map_err(|e| e.to_string())
}
