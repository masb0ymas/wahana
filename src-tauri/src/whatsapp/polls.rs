//! Polls: their keys, opening and recording votes, and sending polls and votes.

use super::*;

/// The secret a poll creation message carries, which keys every vote on it. `None` for any
/// other message. It rides on the message's context info, outside or inside its wrappers.
pub(super) fn poll_secret(message: &wa::Message) -> Option<Vec<u8>> {
    if !matches!(
        interactive_content(message),
        Some((MessageKind::Poll, _, _))
    ) {
        return None;
    }
    let base = message.get_base_message();
    let v4 = base
        .poll_creation_message_v4
        .as_option()
        .and_then(|w| w.message.as_option());
    [Some(message), Some(base), v4]
        .into_iter()
        .flatten()
        .filter_map(|m| m.message_context_info.as_option())
        .find_map(|c| c.message_secret.clone())
        .filter(|s| !s.is_empty())
}

/// The key of a poll a live message creates, with its creator as the chat addressed them.
pub(super) fn poll_key_of(message: &wa::Message, sender: &Jid) -> Option<PollKey> {
    Some(PollKey {
        secret: poll_secret(message)?,
        creator: Some(bare_jid(&sender.to_string())),
    })
}

/// The key of a poll from a history transfer. The phone may keep the secret on the message
/// info rather than in the message. Our own polls name no creator; it is resolved when used.
pub(super) fn history_poll_key(info: &wa::WebMessageInfo) -> Option<PollKey> {
    let message = info.message.as_option()?;
    if !matches!(
        interactive_content(message),
        Some((MessageKind::Poll, _, _))
    ) {
        return None;
    }
    let secret =
        poll_secret(message).or_else(|| info.message_secret.clone().filter(|s| !s.is_empty()))?;
    let key = info.key.as_option()?;
    let creator = if key.from_me.unwrap_or(false) {
        None
    } else {
        key.participant
            .as_deref()
            .or(info.participant.as_deref())
            .or(key.remote_jid.as_deref())
            .map(bare_jid)
    };
    Some(PollKey { secret, creator })
}

/// Records the votes a history transfer carries on a poll. The phone already opened them, so
/// they hold the picked options' hashes in the clear.
pub(super) fn store_history_votes(
    db: &ChatDb,
    message: &IncomingMessage,
    info: &wa::WebMessageInfo,
) -> rusqlite::Result<()> {
    let Some(poll) = message
        .view
        .interactive
        .as_ref()
        .and_then(|i| i.poll.as_ref())
    else {
        return Ok(());
    };
    for update in &info.poll_updates {
        let Some(key) = update.poll_update_message_key.as_option() else {
            continue;
        };
        let voter = if key.from_me.unwrap_or(false) {
            "me".to_string()
        } else {
            match key.participant.as_deref().or(key.remote_jid.as_deref()) {
                Some(id) => bare_jid(id),
                None => continue,
            }
        };
        let hashes = update
            .vote
            .as_option()
            .map(|v| v.selected_options.as_slice())
            .unwrap_or_default();
        let at = update
            .sender_timestamp_ms
            .or(update.server_timestamp_ms)
            .unwrap_or(message.view.timestamp);
        db.set_poll_vote(
            &message.view.id,
            &voter,
            &option_names(&poll.options, hashes),
            at,
        )?;
    }
    Ok(())
}

/// The options a vote picked, from the hashes it carries. Hashes matching no option (one
/// added after the poll was stored) are dropped.
pub(super) fn option_names(options: &[String], hashes: &[Vec<u8>]) -> Vec<String> {
    use whatsapp_rust::wacore::poll::compute_option_hash;
    options
        .iter()
        .filter(|o| {
            let hash = compute_option_hash(o);
            hashes.iter().any(|h| h.as_slice() == hash)
        })
        .cloned()
        .collect()
}

/// Who created a stored poll: the id its creation message named, else (history, or a poll
/// stored without one) the stored author.
pub(super) fn poll_creator(account: &WaAccount, poll: &PollTarget) -> String {
    if let Some(creator) = &poll.creator {
        return creator.clone();
    }
    let target = MessageTarget {
        from_me: poll.from_me,
        sender_id: poll.sender_id.clone(),
        body: String::new(),
        media_proto: None,
        server_id: None,
    };
    message_author(account, &poll.chat_id, &target)
}

/// `id` in the namespace (privacy id or phone number) of `like`, when the counterpart is known.
pub(super) fn same_namespace(
    account: &WaAccount,
    client: &Client,
    id: &str,
    like: &str,
) -> Option<String> {
    let want_lid = like.ends_with("@lid");
    if id.ends_with("@lid") == want_lid {
        return Some(id.to_string());
    }
    let own_pn = client.pn().map(|j| bare_jid(&j.to_string()));
    let own_lid = client.lid().map(|j| bare_jid(&j.to_string()));
    if own_pn.as_deref() == Some(id) || own_lid.as_deref() == Some(id) {
        return if want_lid { own_lid } else { own_pn };
    }
    let db = account.db.lock().unwrap();
    if want_lid {
        db.lid_for(id).ok().flatten()
    } else {
        db.pn_for(id).ok().flatten()
    }
}

/// Opens a vote into the hashes of the options it picked. The creator and voter ids key it in
/// one namespace, the voter's; a poll learned under the other one (a chat that moved to
/// privacy ids, or our own poll) gets its creator moved over before a second try.
pub(super) async fn open_vote(
    account: &WaAccount,
    client: &Client,
    ciphertext: PollVoteCiphertext<'_>,
    secret: &[u8],
    poll_id: &str,
    creator: &str,
    voter: &str,
) -> Result<Vec<Vec<u8>>, String> {
    let parse = |id: &str| id.parse::<Jid>().map_err(|_| format!("invalid id: {id}"));
    let voter_jid = parse(voter)?;
    let first = client
        .polls()
        .decrypt_vote(ciphertext, secret, poll_id, &parse(creator)?, &voter_jid)
        .await;
    let Err(e) = first else {
        return first.map_err(|e| e.to_string());
    };
    let Some(moved) = same_namespace(account, client, creator, voter).filter(|c| c != creator)
    else {
        return Err(e.to_string());
    };
    client
        .polls()
        .decrypt_vote(ciphertext, secret, poll_id, &parse(&moved)?, &voter_jid)
        .await
        .map_err(|e| e.to_string())
}

/// Opens and records a live vote on a stored poll. A vote on a poll this account never
/// stored, or stored without its key, can't be opened and is dropped.
#[allow(clippy::too_many_arguments)]
pub(super) async fn record_poll_vote(
    app: &AppHandle,
    account: &Arc<WaAccount>,
    client: &Client,
    chat_id: &str,
    sender: &Jid,
    from_me: bool,
    update: &wa::message::PollUpdateMessage,
    received_at: i64,
) {
    let Some(poll_id) = update
        .poll_creation_message_key
        .as_option()
        .and_then(|k| k.id.clone())
    else {
        return;
    };
    let Some((payload, iv)) = update
        .vote
        .as_option()
        .and_then(|v| Some((v.enc_payload.as_deref()?, v.enc_iv.as_deref()?)))
    else {
        return;
    };
    let poll = account
        .db
        .lock()
        .unwrap()
        .poll_target(chat_id, &poll_id)
        .ok()
        .flatten();
    let Some((poll, secret)) = poll.and_then(|p| {
        let secret = p.secret.clone()?;
        Some((p, secret))
    }) else {
        return;
    };
    let creator = poll_creator(account, &poll);
    let voter = bare_jid(&sender.to_string());
    let ciphertext = PollVoteCiphertext {
        enc_payload: payload,
        enc_iv: iv,
    };
    let hashes = match open_vote(
        account, client, ciphertext, &secret, &poll_id, &creator, &voter,
    )
    .await
    {
        Ok(hashes) => hashes,
        Err(e) => {
            eprintln!("failed to open a WhatsApp poll vote: {e}");
            return;
        }
    };
    let picked = option_names(&poll.options, &hashes);
    let who = if from_me { "me" } else { voter.as_str() };
    let at = update.sender_timestamp_ms.unwrap_or(received_at);
    let changed = account
        .db
        .lock()
        .unwrap()
        .set_poll_vote(&poll_id, who, &picked, at)
        .unwrap_or(false);
    if changed {
        emit_chats(app, account);
    }
}

/// Sends a poll. `multiple` lets voters pick any number of options; otherwise one.
#[tauri::command]
pub async fn wa_native_send_poll(
    app: AppHandle,
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
    question: String,
    options: Vec<String>,
    multiple: bool,
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
    let selectable = if multiple { options.len() as u32 } else { 1 };
    let (sent, secret) = client
        .polls()
        .create(to, &question, &options, selectable)
        .await
        .map_err(|e| e.to_string())?;
    // Voters key their votes to us under the id this chat knows us by.
    let creator = if chat_id.ends_with("@lid") {
        client.lid()
    } else {
        client.pn()
    }
    .map(|j| bare_jid(&j.to_string()));
    let message = IncomingMessage {
        view: MessageView {
            id: sent.message_id,
            chat_id,
            from_me: true,
            sender_name,
            sender_phone: None,
            sender_id: None,
            kind: MessageKind::Poll,
            body: poll_body(&question, &options),
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
            interactive: Some(Interactive {
                poll: Some(PollInfo {
                    question,
                    options,
                    multiple,
                    results: None,
                }),
                ..Default::default()
            }),
        },
        sender_id: String::new(),
        media: None,
        quote: None,
        album: None,
        poll_key: Some(PollKey { secret, creator }),
    };
    record_message(&app, &account, generation, message);
    Ok(())
}

/// Votes on a stored poll, replacing any earlier vote; an empty `options` withdraws it.
#[tauri::command]
pub async fn wa_native_poll_vote(
    app: AppHandle,
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
    message_id: String,
    options: Vec<String>,
) -> Result<(), String> {
    let account = state.get(&id)?;
    let client = running_client(&account)?;
    let poll = account
        .db
        .lock()
        .unwrap()
        .poll_target(&chat_id, &message_id)
        .map_err(|e| e.to_string())?
        .ok_or("poll not found")?;
    let secret = poll.secret.clone().ok_or(
        "This poll arrived before Wahana kept poll keys, so it can't be voted on here. Vote from your phone instead.",
    )?;
    if let Some(unknown) = options.iter().find(|o| !poll.options.contains(o)) {
        return Err(format!("not an option of this poll: {unknown}"));
    }
    let creator: Jid = poll_creator(&account, &poll)
        .parse()
        .map_err(|_| "invalid poll creator".to_string())?;
    let chat: Jid = poll
        .chat_id
        .parse()
        .map_err(|_| format!("invalid chat id: {}", poll.chat_id))?;
    client
        .polls()
        .vote(chat, &message_id, &creator, &secret, &options)
        .await
        .map_err(|e| e.to_string())?;
    account
        .db
        .lock()
        .unwrap()
        .set_poll_vote(&message_id, "me", &options, now_millis())
        .map_err(|e| e.to_string())?;
    emit_chats(&app, &account);
    Ok(())
}
