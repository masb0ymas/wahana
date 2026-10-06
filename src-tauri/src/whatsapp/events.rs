//! A running account: the client, its connection, and the messages and events it delivers.

use super::*;

/// Names group chats by their subject and records the community each belongs to. Groups are
/// only listed by the server on request, so this runs on every connect; a failure just leaves
/// groups named by their id.
pub(super) async fn load_group_names(
    app: &AppHandle,
    account: &WaAccount,
    generation: u64,
    client: &Client,
) {
    let groups = match client.groups().get_participating().await {
        Ok(groups) => groups,
        Err(e) => {
            eprintln!("failed to list WhatsApp groups: {e}");
            return;
        }
    };
    if !account.is_current(generation) {
        return;
    }
    let result = account.db.lock().unwrap().batch(|db| {
        for (jid, meta) in &groups {
            db.set_name(&jid.to_string(), &meta.subject, NameSource::GroupSubject)?;
            db.set_community(
                &jid.to_string(),
                meta.parent_group_jid
                    .as_ref()
                    .map(|p| p.to_string())
                    .as_deref(),
            )?;
        }
        Ok(())
    });
    if let Err(e) = result {
        eprintln!("failed to store WhatsApp group names: {e}");
    }
    emit_chats(app, account);
}

/// Names channel (newsletter) chats from the list of channels this account follows; the
/// server only sends their names on request, so this runs on every connect.
pub(super) async fn load_channel_names(
    app: &AppHandle,
    account: &WaAccount,
    generation: u64,
    client: &Client,
) {
    let channels = match crate::channel_mex::subscribed(client).await {
        Ok(channels) => channels,
        Err(e) => {
            eprintln!("failed to list WhatsApp channels: {e}");
            return;
        }
    };
    if !account.is_current(generation) {
        return;
    }
    let result = account.db.lock().unwrap().batch(|db| {
        for meta in &channels {
            db.set_name(
                &bare_jid(&meta.jid.to_string()),
                &meta.name,
                NameSource::GroupSubject,
            )?;
        }
        Ok(())
    });
    if let Err(e) = result {
        eprintln!("failed to store WhatsApp channel names: {e}");
    }
    emit_chats(app, account);
}

/// Fills in phone numbers for direct chats that run on a privacy id, from the mappings
/// the protocol layer has learned.
pub(super) async fn map_lids(
    app: &AppHandle,
    account: &WaAccount,
    generation: u64,
    client: &Client,
) {
    let lids = account
        .db
        .lock()
        .unwrap()
        .unmapped_lids()
        .unwrap_or_default();
    let mut found = false;
    for lid in lids {
        let Ok(jid) = lid.parse::<Jid>() else {
            continue;
        };
        if let Ok(Some(entry)) = client.get_lid_pn_entry(&jid).await {
            if !account.is_current(generation) {
                return;
            }
            let pn = format!("{}@s.whatsapp.net", entry.phone_number);
            let _ = account.db.lock().unwrap().set_lid_pn(&lid, &pn);
            found = true;
        }
    }
    if found {
        emit_chats(app, account);
    }
}

/// Handles the events the per-kind `Bot` callbacks do not cover: history transfers and
/// contact names from the phone's address book.
pub(super) async fn handle_event(
    app: AppHandle,
    account: Arc<WaAccount>,
    generation: u64,
    event: Arc<Event>,
) {
    if !account.is_current(generation) {
        return;
    }
    match &*event {
        // Delivery and read receipts for messages I sent. Receipts from my own other
        // devices (`is_from_me`) say nothing about the recipient. In a group, the first
        // participant to receive or read a message moves it on.
        Event::Receipt(receipt) => {
            use whatsapp_rust::types::presence::ReceiptType;
            if receipt.source.is_from_me {
                return;
            }
            let ack = match receipt.r#type {
                ReceiptType::Delivered => ACK_DELIVERED,
                ReceiptType::Read => ACK_READ,
                ReceiptType::Played => ACK_PLAYED,
                _ => return,
            };
            let ids: Vec<String> = receipt
                .message_ids
                .iter()
                .map(|id| id.to_string())
                .collect();
            let sender = bare_jid(&receipt.source.sender.to_string());
            let at = receipt.timestamp.timestamp_millis();
            let changed = {
                let db = account.db.lock().unwrap();
                let _ = db.add_receipts(&ids, &sender, ack, at);
                db.raise_ack(&ids, ack).unwrap_or(false)
            };
            if changed {
                emit_chats(&app, &account);
            }
        }
        Event::ChatPresence(update) => {
            use whatsapp_rust::types::presence::{ChatPresence, ChatPresenceMedia};
            if update.source.is_from_me {
                return;
            }
            let state = match (update.state, update.media) {
                (ChatPresence::Paused, _) => "paused",
                (ChatPresence::Composing, ChatPresenceMedia::Audio) => "recording",
                (ChatPresence::Composing, ChatPresenceMedia::Text) => "typing",
            };
            let chat = bare_jid(&update.source.chat.to_string());
            let sender = bare_jid(&update.source.sender.to_string());
            let mut chat_ids = vec![chat.clone()];
            let db = account.db.lock().unwrap();
            if !update.source.is_group {
                let alt = if chat.ends_with("@lid") {
                    db.pn_for(&chat)
                } else {
                    db.lid_for(&chat)
                };
                chat_ids.extend(alt.ok().flatten().filter(|a| *a != chat));
            }
            let sender_name = db.who(&sender).ok().and_then(|w| w.name.or(w.phone));
            drop(db);
            let _ = app.emit_to(
                "main",
                "wa_native:typing",
                TypingPayload {
                    id: account.id.clone(),
                    chat_ids,
                    sender,
                    sender_name,
                    state,
                },
            );
        }
        Event::HistorySync(sync) => {
            let progress = sync.progress();
            let sync = (**sync).clone();
            let db_account = account.clone();
            // A chunk can be megabytes of compressed protobuf: inflate and write it off
            // the async runtime.
            let result = tauri::async_runtime::spawn_blocking(move || {
                let Some(decoded) = sync.get() else {
                    return Ok(Vec::new());
                };
                db_account
                    .db
                    .lock()
                    .unwrap()
                    .batch(|db| store_history(db, decoded))
            })
            .await;
            match result {
                Ok(Ok(pins)) => {
                    // Only the current state matters: skip pins that have already lapsed.
                    let now = now_millis();
                    for pin in pins {
                        let expires = if pin.on {
                            pin.at + i64::from(pin.secs) * 1000
                        } else {
                            0
                        };
                        if pin.on && expires <= now {
                            continue;
                        }
                        let _ = app.emit_to(
                            "main",
                            "wa_native:pin",
                            PinPayload {
                                id: account.id.clone(),
                                chat_id: pin.chat_id,
                                message_id: pin.message_id,
                                on: pin.on,
                                expires,
                            },
                        );
                    }
                }
                Ok(Err(e)) => eprintln!("failed to store WhatsApp history: {e}"),
                Err(e) => eprintln!("WhatsApp history task failed: {e}"),
            }
            account.inner.lock().unwrap().syncing = progress.filter(|p| *p < 100);
            emit_chats(&app, &account);
            emit_account(&app, &account);
        }
        Event::ContactUpdate(update) => {
            let action = &update.action;
            let Some(name) = action.full_name.as_deref().or(action.first_name.as_deref()) else {
                return;
            };
            let ids = [
                Some(update.jid.to_string()),
                action.pn_jid.clone(),
                action.lid_jid.clone(),
            ];
            let db = account.db.lock().unwrap();
            if let (Some(pn), Some(lid)) = (&action.pn_jid, &action.lid_jid) {
                let _ = db.set_lid_pn(&bare_jid(lid), &bare_jid(pn));
            }
            for id in ids.into_iter().flatten() {
                let _ = db.set_name(&bare_jid(&id), name, NameSource::Contact);
            }
            drop(db);
            emit_chats(&app, &account);
        }
        Event::PushNameUpdate(update) => {
            let _ = account.db.lock().unwrap().set_name(
                &bare_jid(&update.jid.to_string()),
                &update.new_push_name,
                NameSource::PushName,
            );
            emit_chats(&app, &account);
        }
        Event::MuteUpdate(update) => {
            // `mute_end_timestamp` is in ms; -1 (or unset) means muted indefinitely.
            let action = &update.action;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as i64);
            let until = match action.mute_end_timestamp {
                _ if !action.muted.unwrap_or(false) => 0,
                None => -1,
                Some(end) if end <= 0 => -1,
                Some(end) if end > now => end,
                Some(_) => 0,
            };
            let _ = account
                .db
                .lock()
                .unwrap()
                .set_mute(&bare_jid(&update.jid.to_string()), until);
            emit_chats(&app, &account);
        }
        Event::PinUpdate(update) => {
            let pinned_at = if update.action.pinned.unwrap_or(false) {
                update.timestamp.timestamp_millis().max(1)
            } else {
                0
            };
            let _ = account
                .db
                .lock()
                .unwrap()
                .set_pin(&bare_jid(&update.jid.to_string()), pinned_at);
            emit_chats(&app, &account);
        }
        Event::MarkChatAsReadUpdate(update) => {
            // Read (or marked unread) on the phone or another linked device.
            let read = update.action.read.unwrap_or(true);
            let changed = account
                .db
                .lock()
                .unwrap()
                .set_read_from_device(&update.jid.to_string(), read)
                .unwrap_or(false);
            if changed {
                emit_account(&app, &account);
                emit_chats(&app, &account);
            }
        }
        Event::LabelEditUpdate(update) => {
            let action = &update.action;
            if action.deleted.unwrap_or(false) {
                let _ = account.db.lock().unwrap().delete_label(&update.label_id);
            } else if let Some(name) = action.name.as_deref().filter(|n| !n.is_empty()) {
                let color = action.color.unwrap_or(0) as i64;
                let _ =
                    account
                        .db
                        .lock()
                        .unwrap()
                        .upsert_label(&update.label_id, name, color, false);
            }
            emit_labels(&app, &account);
        }
        Event::LabelAssociationUpdate(update) => {
            let labeled = update.action.labeled.unwrap_or(false);
            let _ = account.db.lock().unwrap().set_chat_label(
                &update.label_id,
                &bare_jid(&update.chat_jid.to_string()),
                labeled,
            );
            emit_labels(&app, &account);
        }
        _ => {}
    }
}

/// Builds and starts the client for one account. Runs on the async runtime so the Tauri
/// command returns immediately and the QR code can arrive whenever the server sends it.
pub(super) async fn run_account(
    app: AppHandle,
    account: Arc<WaAccount>,
    db_path: PathBuf,
    generation: u64,
    reset_session: bool,
) -> Result<(), String> {
    if reset_session {
        remove_sqlite_files(&db_path);
        // The next pairing may be a different phone; it sends its own history anyway.
        if let Err(e) = account.db.lock().unwrap().clear() {
            eprintln!("failed to clear WhatsApp chat history: {e}");
        }
        emit_chats(&app, &account);
    }
    let database_url = db_path.to_string_lossy().to_string();
    let store = SqliteStore::new(&database_url)
        .await
        .map_err(|e| format!("failed to open WhatsApp session store: {e}"))?;

    let qr_app = app.clone();
    let qr_account = account.clone();
    let on_qr = move |code: String, timeout: std::time::Duration| {
        let app = qr_app.clone();
        let account = qr_account.clone();
        async move {
            {
                let mut inner = account.inner.lock().unwrap();
                if inner.generation != generation {
                    return;
                }
                inner.status = WaStatus::Qr;
                inner.error = None;
            }
            let _ = app.emit_to(
                "main",
                "wa_native:qr",
                QrPayload {
                    id: account.id.clone(),
                    code,
                    timeout_ms: timeout.as_millis() as u64,
                },
            );
            emit_account(&app, &account);
        }
    };

    let connected_app = app.clone();
    let connected_account = account.clone();
    let on_connected = move |client: Arc<Client>| {
        let app = connected_app.clone();
        let account = connected_account.clone();
        async move {
            let device = client.persistence_manager().get_device_snapshot();
            {
                let mut inner = account.inner.lock().unwrap();
                if inner.generation != generation {
                    return;
                }
                inner.status = WaStatus::Working;
                inner.error = None;
                inner.me = Some(AccountMe {
                    // `Jid`'s Display appends the device suffix (`628…:16@s.whatsapp.net`), which
                    // the profile-picture lookup does not answer for and which leaks into the
                    // number shown in the UI, so store the bare JID like every other consumer.
                    // Accounts without a phone number fall back to their LID.
                    id: device
                        .pn
                        .as_ref()
                        .or(device.lid.as_ref())
                        .map(|jid| bare_jid(&jid.to_string()))
                        .unwrap_or_default(),
                    push_name: device.push_name.clone(),
                });
            }
            emit_account(&app, &account);
            // Off the event handler: it waits on a server round trip.
            tauri::async_runtime::spawn(async move {
                load_group_names(&app, &account, generation, &client).await;
                load_channel_names(&app, &account, generation, &client).await;
                // Labels live in the "regular" app-state collection; a full sync populates
                // them for accounts whose state predates the local label cache.
                let _ = client
                    .clone()
                    .process_sync_task(MajorSyncTask::AppStateSync {
                        name: WAPatchName::Regular,
                        full_sync: true,
                    })
                    .await;
                map_lids(&app, &account, generation, &client).await;
            });
        }
    };

    let message_app = app.clone();
    let message_account = account.clone();
    let on_message = move |ctx: MessageContext| {
        let app = message_app.clone();
        let account = message_account.clone();
        async move {
            let source = &ctx.info.source;
            let chat_id = source.chat.to_string();
            let is_group = source.is_group || chat_id.ends_with("@g.us");
            let base = ctx.message.get_base_message();

            // A reaction rides the message channel but is not a message of its own.
            if let Some(rm) = base.reaction_message.as_option() {
                if let Some(target_id) = rm.key.as_option().and_then(|k| k.id.clone()) {
                    let from_me = source.is_from_me;
                    let participant = (is_group || chat_id == "status@broadcast")
                        .then(|| source.sender.to_string());
                    let _ = app.emit_to(
                        "main",
                        "wa_native:reaction",
                        ReactionPayload {
                            id: account.id.clone(),
                            message_id: target_id,
                            from: if from_me {
                                String::new()
                            } else {
                                bare_jid(&source.sender.to_string())
                            },
                            from_me,
                            participant,
                            text: rm.text.clone().unwrap_or_default(),
                        },
                    );
                }
                return;
            }

            // A pin or unpin for everyone, from another member or our own phone.
            if let Some(pin) = base.pin_in_chat_message.as_option() {
                if let Some(target_id) = pin.key.as_option().and_then(|k| k.id.clone()) {
                    let on = pin.r#type == Some(wa::message::pin_in_chat_message::Type::PinForAll);
                    let secs = base
                        .message_context_info
                        .as_option()
                        .and_then(|c| c.message_add_on_duration_in_secs)
                        .filter(|s| *s > 0)
                        .unwrap_or(7 * 24 * 3600);
                    let at = pin
                        .sender_timestamp_ms
                        .unwrap_or_else(|| ctx.info.timestamp.timestamp_millis());
                    let _ = app.emit_to(
                        "main",
                        "wa_native:pin",
                        PinPayload {
                            id: account.id.clone(),
                            chat_id: chat_id.clone(),
                            message_id: target_id,
                            on,
                            expires: if on { at + i64::from(secs) * 1000 } else { 0 },
                        },
                    );
                }
                return;
            }

            // A vote on a poll, encrypted to the poll's key.
            if let Some(update) = base.poll_update_message.as_option() {
                record_poll_vote(
                    &app,
                    &account,
                    &ctx.client,
                    &chat_id,
                    &source.sender,
                    source.is_from_me,
                    update,
                    ctx.info.timestamp.timestamp_millis(),
                )
                .await;
                return;
            }

            // Revokes and edits arrive as protocol messages.
            if let Some(pm) = base.protocol_message.as_option() {
                if pm.r#type == Some(wa::message::protocol_message::Type::Revoke) {
                    if let Some(target_id) = pm.key.as_option().and_then(|k| k.id.clone()) {
                        let stored = account
                            .db
                            .lock()
                            .unwrap()
                            .revoke_message(
                                &chat_id,
                                &target_id,
                                ctx.info.timestamp.timestamp_millis(),
                            )
                            .unwrap_or(None);
                        if stored.is_some() {
                            emit_chats(&app, &account);
                        }
                        let from_me = source.is_from_me;
                        let participant = (is_group || chat_id == "status@broadcast")
                            .then(|| source.sender.to_string());
                        let _ = app.emit_to(
                            "main",
                            "wa_native:revoked",
                            RevokedPayload {
                                id: account.id.clone(),
                                chat_id: chat_id.clone(),
                                message_id: target_id,
                                from_me,
                                participant,
                                timestamp: ctx.info.timestamp.timestamp_millis(),
                            },
                        );
                    }
                    return;
                }
                if let Some(edited) = pm.edited_message.as_option() {
                    if let Some(target_id) = pm.key.as_option().and_then(|k| k.id.clone()) {
                        let body = edited.text_content().unwrap_or_default().to_string();
                        let updated = account
                            .db
                            .lock()
                            .unwrap()
                            .edit_message(
                                &chat_id,
                                &target_id,
                                &body,
                                ctx.info.timestamp.timestamp_millis(),
                            )
                            .unwrap_or(None);
                        if updated.is_some() {
                            emit_chats(&app, &account);
                        }
                    }
                    return;
                }
            }

            let Some((kind, body, media, interactive)) = message_content(&ctx.message) else {
                return;
            };
            let from_me = source.is_from_me;
            let sender = bare_jid(&source.sender.to_string());
            // A message from a privacy id usually carries the sender's phone number too.
            if let Some(alt) = &source.sender_alt {
                let alt = bare_jid(&alt.to_string());
                let _ = account.db.lock().unwrap().set_lid_pn(&sender, &alt);
                let _ = account.db.lock().unwrap().set_lid_pn(&alt, &sender);
            }
            let message = IncomingMessage {
                view: MessageView {
                    id: ctx.info.id.to_string(),
                    chat_id: source.chat.to_string(),
                    from_me,
                    sender_name: ctx.info.push_name.clone(),
                    sender_phone: None,
                    sender_id: None,
                    kind,
                    body,
                    timestamp: ctx.info.timestamp.timestamp_millis(),
                    media: None,
                    ack: ACK_SENT,
                    revoked_at: None,
                    edited_at: None,
                    edits: Vec::new(),
                    channel_reactions: Vec::new(),
                    reply_to: None,
                    status_mention: status_mention_of(&ctx.message),
                    album_id: None,
                    preview: link_preview(&ctx.message),
                    interactive,
                },
                sender_id: if from_me { String::new() } else { sender },
                media,
                quote: quote_of(&ctx.message),
                album: album_of(&ctx.message),
                poll_key: poll_key_of(&ctx.message, &source.sender),
            };
            let channel_ref = (message.view.chat_id.ends_with("@newsletter")
                && ctx.info.server_id > 0)
                .then(|| (message.view.chat_id.clone(), message.view.id.clone()));
            record_message(&app, &account, generation, message);
            if let Some((chat, id)) = channel_ref {
                let _ =
                    account
                        .db
                        .lock()
                        .unwrap()
                        .set_server_id(&chat, &id, ctx.info.server_id as i64);
            }
        }
    };

    let event_app = app.clone();
    let event_account = account.clone();
    let on_event = move |event: Arc<Event>, _client: Arc<Client>| {
        handle_event(event_app.clone(), event_account.clone(), generation, event)
    };

    let logout_app = app.clone();
    let logout_account = account.clone();
    let on_logged_out = move |_info: whatsapp_rust::types::events::LoggedOut| {
        let app = logout_app.clone();
        let account = logout_account.clone();
        async move {
            let (handle, retired) = {
                let mut inner = account.inner.lock().unwrap();
                if inner.generation != generation {
                    return;
                }
                inner.generation += 1;
                inner.status = WaStatus::LoggedOut;
                inner.me = None;
                inner.client = None;
                inner.syncing = None;
                inner.reset_session = true;
                (inner.handle.take(), inner.generation)
            };
            emit_account(&app, &account);
            // This handler runs on the client's own event dispatch, so the shutdown it
            // waits on is moved off it.
            tauri::async_runtime::spawn(async move {
                if let Some(handle) = handle {
                    handle.shutdown().await;
                }
                let mut inner = account.inner.lock().unwrap();
                if inner.generation == retired {
                    inner.running = false;
                }
            });
        }
    };

    let bot = Bot::builder()
        .with_backend(store)
        .on_qr_code(on_qr)
        .on_connected(on_connected)
        .on_message(on_message)
        .on_event_for(
            &[
                EventKind::Receipt,
                EventKind::HistorySync,
                EventKind::ContactUpdate,
                EventKind::PushNameUpdate,
                EventKind::LabelEditUpdate,
                EventKind::LabelAssociationUpdate,
                EventKind::ChatPresence,
            ],
            on_event,
        )
        .on_logged_out(on_logged_out)
        .build()
        .await
        .map_err(|e| format!("failed to start WhatsApp client: {e}"))?;

    let handle = bot.spawn();
    let client = handle.client();
    let orphaned = {
        let mut inner = account.inner.lock().unwrap();
        if inner.generation == generation {
            inner.handle = Some(handle);
            inner.client = Some(client);
            None
        } else {
            Some(handle)
        }
    };
    // Stopped while the client was being built: nothing else will ever reach this handle,
    // so shut it down here instead of leaving a live connection behind.
    if let Some(handle) = orphaned {
        handle.shutdown().await;
    }
    // A blocked network leaves the client at `Starting` with no hint of why. Report it after a
    // grace period instead of spinning forever; a connection that still succeeds later clears
    // this, since the QR and Connected handlers set the status and drop the error.
    let watchdog_app = app.clone();
    let watchdog_account = account.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(CONNECT_TIMEOUT).await;
        let mut inner = watchdog_account.inner.lock().unwrap();
        if inner.generation != generation || !inner.running || inner.status != WaStatus::Starting {
            return;
        }
        inner.status = WaStatus::Failed;
        inner.error = Some(
            "Couldn't connect to WhatsApp. Check your internet, VPN or firewall, then try again."
                .into(),
        );
        drop(inner);
        emit_account(&watchdog_app, &watchdog_account);
    });
    Ok(())
}
