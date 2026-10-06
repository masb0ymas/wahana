//! The chat list and chat-wide actions: read state, delete, pin, archive and mute.

use super::*;

#[tauri::command]
pub async fn wa_native_chats(
    state: State<'_, WaState>,
    id: String,
) -> Result<Vec<ChatInfo>, String> {
    let account = state.get(&id)?;
    // Channels only carry their name in their metadata, so fetch it once per channel we
    // don't have a name for yet (list_subscribed on connect covers most of them already).
    let (client, generation) = {
        let inner = account.inner.lock().unwrap();
        (inner.client.clone(), inner.generation)
    };
    if let Some(client) = client {
        let ids = account
            .db
            .lock()
            .unwrap()
            .newsletter_ids()
            .unwrap_or_default();
        for chat_id in ids {
            // Ask for each channel's metadata once per run, however it turns out.
            {
                let mut inner = account.inner.lock().unwrap();
                if inner.channels_gen != generation {
                    inner.channels_gen = generation;
                    inner.channels_probed.clear();
                }
                if inner.channels_probed.contains(&chat_id) {
                    continue;
                }
                inner.channels_probed.insert(chat_id.clone());
            }
            let missing = account
                .db
                .lock()
                .unwrap()
                .who(&chat_id)
                .map(|w| w.name.is_none())
                .unwrap_or(false);
            if !missing {
                continue;
            }
            let Ok(jid) = chat_id.parse::<Jid>() else {
                continue;
            };
            if let Ok(meta) = crate::channel_mex::get(&client, &jid).await {
                if !account.is_current(generation) {
                    break;
                }
                let _ = account.db.lock().unwrap().set_name(
                    &chat_id,
                    &meta.name,
                    NameSource::GroupSubject,
                );
            }
        }
    }
    let chats = account.db.lock().unwrap().chats();
    chats.map_err(|e| e.to_string())
}

/// Clears the unread count of every chat, or only of `chat_ids` when given. Local only,
/// like `wa_native_mark_read`; the phone is told best-effort so its badges clear too.
#[tauri::command]
pub async fn wa_native_mark_all_read(
    app: AppHandle,
    state: State<'_, WaState>,
    id: String,
    chat_ids: Option<Vec<String>>,
) -> Result<(), String> {
    let account = state.get(&id)?;
    let targets: Vec<String> = match &chat_ids {
        Some(ids) => ids.clone(),
        None => account
            .db
            .lock()
            .unwrap()
            .chats()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|c| c.unread > 0)
            .map(|c| c.id)
            .collect(),
    };
    {
        let db = account.db.lock().unwrap();
        let result = match &chat_ids {
            Some(ids) => ids.iter().try_for_each(|c| db.mark_read(c)),
            None => db.mark_all_read(),
        };
        result.map_err(|e| e.to_string())?;
    }
    emit_account(&app, &account);
    emit_chats(&app, &account);
    let client = account.inner.lock().unwrap().client.clone();
    if let Some(client) = client {
        for chat in targets {
            if let Ok(jid) = chat.parse::<Jid>() {
                let _ = client
                    .chat_actions()
                    .mark_chat_as_read(&jid, true, None)
                    .await;
            }
        }
    }
    Ok(())
}

/// Deletes chats from this device and, when the account is online, from the linked phone.
#[tauri::command]
pub async fn wa_native_delete_chats(
    app: AppHandle,
    state: State<'_, WaState>,
    id: String,
    chat_ids: Vec<String>,
) -> Result<(), String> {
    let account = state.get(&id)?;
    let client = account.inner.lock().unwrap().client.clone();
    if let Some(client) = client {
        for chat in &chat_ids {
            if let Ok(jid) = chat.parse::<Jid>() {
                let _ = client.chat_actions().delete_chat(&jid, true, None).await;
            }
        }
    }
    {
        let db = account.db.lock().unwrap();
        for chat in &chat_ids {
            db.delete_chat(chat).map_err(|e| e.to_string())?;
        }
    }
    emit_account(&app, &account);
    emit_chats(&app, &account);
    Ok(())
}

/// Pins or unpins a chat. With `sync` it goes to WhatsApp (app state, so it syncs to the
/// phone); without, the pin is kept only in this app, past WhatsApp's limit of three, and any
/// record of the chat's WhatsApp pin is dropped so it does not override the local one.
#[tauri::command]
pub async fn wa_native_pin_chat(
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
    on: bool,
    sync: bool,
) -> Result<(), String> {
    let account = state.get(&id)?;
    if !sync {
        return account
            .db
            .lock()
            .unwrap()
            .forget_pin(&bare_jid(&chat_id))
            .map_err(|e| e.to_string());
    }
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
    let actions = client.chat_actions();
    let result = if on {
        actions.pin_chat(&jid).await
    } else {
        actions.unpin_chat(&jid).await
    };
    result.map_err(|e| e.to_string())?;
    let pinned_at = if on { now_millis().max(1) } else { 0 };
    let _ = account
        .db
        .lock()
        .unwrap()
        .set_pin(&bare_jid(&chat_id), pinned_at);
    Ok(())
}

/// Archives or unarchives a chat (WhatsApp app state, so it syncs to the phone).
#[tauri::command]
pub async fn wa_native_archive_chat(
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
    let actions = client.chat_actions();
    let result = if on {
        actions.archive_chat(&jid, None).await
    } else {
        actions.unarchive_chat(&jid, None).await
    };
    result.map_err(|e| e.to_string())
}

/// Mutes a chat until `until` (epoch ms, or -1 for good), or unmutes it when `until` is `None`
/// (WhatsApp app state, so it syncs to the phone).
#[tauri::command]
pub async fn wa_native_mute_chat(
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
    until: Option<i64>,
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
    let actions = client.chat_actions();
    let result = match until {
        None => actions.unmute_chat(&jid).await,
        Some(end) if end <= 0 => actions.mute_chat(&jid).await,
        Some(end) => actions.mute_chat_until(&jid, end).await,
    };
    result.map_err(|e| e.to_string())?;
    let _ = account
        .db
        .lock()
        .unwrap()
        .set_mute(&bare_jid(&chat_id), until.map_or(0, |e| e.max(-1)));
    Ok(())
}
