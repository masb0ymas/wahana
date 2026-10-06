//! Profile pictures, and downloading and sending media.

use super::*;

/// One best-effort profile-picture lookup; errors (including 404/401) become `None`.
pub(super) async fn picture_lookup(client: &Client, jid: &Jid, preview: bool) -> Option<String> {
    client
        .contacts()
        .get_profile_picture(jid, preview)
        .await
        .ok()
        .flatten()
        .map(|picture| picture.url)
}

/// Looks up a chat's profile picture (the small preview), once per chat per run.
#[tauri::command]
pub async fn wa_native_picture(
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
) -> Result<Option<String>, String> {
    let account = state.get(&id)?;
    let client = {
        let inner = account.inner.lock().unwrap();
        if let Some(cached) = inner.pictures.get(&chat_id) {
            return Ok(cached.clone());
        }
        inner
            .client
            .clone()
            .ok_or("WhatsApp account is not running")?
    };
    let jid: Jid = chat_id
        .parse()
        .map_err(|_| format!("invalid chat id: {chat_id}"))?;
    // Channels carry their picture in their metadata; profile pictures are for contacts.
    let url = if chat_id.ends_with("@newsletter") {
        crate::channel_mex::get(&client, &jid)
            .await
            .ok()
            .and_then(|meta| meta.picture_url)
    } else {
        let snapshot = client.persistence_manager().get_device_snapshot();
        let pn_match = snapshot.pn.as_ref().is_some_and(|pn| pn.user == jid.user);
        let lid_match = snapshot
            .lid
            .as_ref()
            .is_some_and(|lid| lid.user == jid.user);
        if pn_match || lid_match {
            // The account's own picture is served inconsistently: try preview, then the full
            // image, then the other (LID/PN) form of the JID. Missing results are tolerated.
            let alt = if pn_match {
                snapshot.lid.clone()
            } else {
                snapshot.pn.clone()
            };
            let mut url = picture_lookup(&client, &jid, true).await;
            if url.is_none() {
                url = picture_lookup(&client, &jid, false).await;
            }
            if url.is_none() {
                if let Some(alt) = alt.as_ref() {
                    url = picture_lookup(&client, alt, true).await;
                    if url.is_none() {
                        url = picture_lookup(&client, alt, false).await;
                    }
                }
            }
            url
        } else {
            client
                .contacts()
                .get_profile_picture(&jid, true)
                .await
                .map_err(|e| e.to_string())?
                .map(|picture| picture.url)
        }
    };
    account
        .inner
        .lock()
        .unwrap()
        .pictures
        .insert(chat_id, url.clone());
    Ok(url)
}

/// whatsapp-rust 0.7.0 treats a message as encrypted from the mere presence of a media
/// key, then demands `file_enc_sha256` to build the URL token. Channel (newsletter) posts
/// are not E2E-encrypted, but some still carry a media key without a `file_enc_sha256`,
/// which made the download fail with "Missing file_enc_sha256". Requiring the encrypted
/// hash as well matches WhatsApp Web; genuinely encrypted media always carries both.
pub(super) struct MediaWithEncHash<'a>(&'a dyn Downloadable);

impl Downloadable for MediaWithEncHash<'_> {
    fn direct_path(&self) -> Option<&str> {
        self.0.direct_path()
    }
    fn media_key(&self) -> Option<&[u8]> {
        self.0.media_key()
    }
    fn file_enc_sha256(&self) -> Option<&[u8]> {
        self.0.file_enc_sha256()
    }
    fn file_sha256(&self) -> Option<&[u8]> {
        self.0.file_sha256()
    }
    fn file_length(&self) -> Option<u64> {
        self.0.file_length()
    }
    fn app_info(&self) -> MediaType {
        self.0.app_info()
    }
    fn static_url(&self) -> Option<&str> {
        self.0.static_url()
    }
    fn is_encrypted(&self) -> bool {
        self.0.media_key().is_some() && self.0.file_enc_sha256().is_some()
    }
}

/// Downloads and decrypts a message's attachment. Returned as raw bytes; the frontend
/// keeps them in the shared media cache.
#[tauri::command]
pub async fn wa_native_media(
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
    message_id: String,
) -> Result<Response, String> {
    let account = state.get(&id)?;
    let client = account
        .inner
        .lock()
        .unwrap()
        .client
        .clone()
        .ok_or("WhatsApp account is not running")?;
    let proto = account
        .db
        .lock()
        .unwrap()
        .media_proto(&chat_id, &message_id)
        .map_err(|e| e.to_string())?
        .ok_or("this message has no media")?;
    let message = wa::Message::decode(&mut proto.as_slice()).map_err(|e| e.to_string())?;
    let downloadable: &dyn Downloadable = if let Some(m) = message.image_message.as_option() {
        m
    } else if let Some(m) = message.video_message.as_option() {
        m
    } else if let Some(m) = message.audio_message.as_option() {
        m
    } else if let Some(m) = message.document_message.as_option() {
        m
    } else if let Some(m) = message.sticker_message.as_option() {
        m
    } else {
        return Err("unsupported media".into());
    };
    let bytes = client
        .download(&MediaWithEncHash(downloadable))
        .await
        .map_err(|e| format!("download failed: {e}"))?;
    Ok(Response::new(bytes))
}

/// Header values are ASCII; the frontend percent-encodes file names and captions.
pub(super) fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&value[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Sends a file. The body is the raw file; headers name the account (`x-account`), chat
/// (`x-chat`), mimetype (`x-mime`), and percent-encoded file name and caption (`x-name`,
/// `x-caption`). `x-mentions` lists comma-separated jids the caption tags. Returns the
/// sent message so the frontend can cache the bytes under it.
#[tauri::command]
pub async fn wa_native_send_media(
    app: AppHandle,
    state: State<'_, WaState>,
    request: Request<'_>,
) -> Result<MessageView, String> {
    let header = |name: &str| {
        request
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(percent_decode)
    };
    let id = header("x-account").ok_or("missing x-account")?;
    let chat_id = header("x-chat").ok_or("missing x-chat")?;
    let mimetype = header("x-mime").unwrap_or_else(|| "application/octet-stream".into());
    let file_name = header("x-name").filter(|n| !n.is_empty());
    let caption = header("x-caption").filter(|c| !c.trim().is_empty());
    let quote_id = header("x-quote").filter(|q| !q.is_empty());
    let quote_chat = header("x-quote-chat").filter(|q| !q.is_empty());
    let as_sticker = header("x-kind").as_deref() == Some("sticker");
    let mentions: Vec<String> = header("x-mentions")
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|jid| !jid.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let data = match request.body() {
        InvokeBody::Raw(bytes) => bytes.clone(),
        InvokeBody::Json(_) => return Err("expected raw body".into()),
    };

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

    let media_type = if as_sticker {
        MediaType::Sticker
    } else if mimetype.starts_with("image/") && mimetype != "image/gif" {
        MediaType::Image
    } else if mimetype.starts_with("video/") {
        MediaType::Video
    } else if mimetype.starts_with("audio/") {
        MediaType::Audio
    } else {
        MediaType::Document
    };
    let context_info = {
        let mut context = match &quote_id {
            Some(qid) => quote_context(
                &account,
                quote_chat.as_deref().unwrap_or(&chat_id),
                &to,
                qid,
            )?,
            None => wa::ContextInfo::default(),
        };
        if !mentions.is_empty() {
            context.mentioned_jid = mentions;
        }
        if quote_id.is_some() || !context.mentioned_jid.is_empty() {
            Some(Box::new(context))
        } else {
            None
        }
    };
    let upload = client
        .upload(data, media_type, whatsapp_rust::UploadOptions::new())
        .await
        .map_err(|e| format!("upload failed: {e}"))?;
    let message = match media_type {
        MediaType::Sticker => wa::Message {
            sticker_message: buffa::MessageField::some(wa::message::StickerMessage {
                url: Some(upload.url),
                direct_path: Some(upload.direct_path),
                media_key: Some(upload.media_key.to_vec()),
                file_sha256: Some(upload.file_sha256.to_vec()),
                file_enc_sha256: Some(upload.file_enc_sha256.to_vec()),
                file_length: Some(upload.file_length),
                media_key_timestamp: Some(upload.media_key_timestamp),
                mimetype: Some("image/webp".into()),
                width: Some(512),
                height: Some(512),
                context_info: context_info
                    .clone()
                    .map(|ci| buffa::MessageField::some(*ci))
                    .unwrap_or_default(),
                ..Default::default()
            }),
            ..Default::default()
        },
        MediaType::Image => whatsapp_rust::media::image_message(
            upload,
            whatsapp_rust::media::ImageOptions {
                caption: caption.clone(),
                mimetype: Some(mimetype.clone()),
                context_info: context_info.clone(),
                ..Default::default()
            },
        ),
        MediaType::Video => whatsapp_rust::media::video_message(
            upload,
            whatsapp_rust::media::VideoOptions {
                caption: caption.clone(),
                mimetype: Some(mimetype.clone()),
                context_info: context_info.clone(),
                ..Default::default()
            },
        ),
        MediaType::Audio => whatsapp_rust::media::audio_message(
            upload,
            whatsapp_rust::media::AudioOptions {
                mimetype: Some(mimetype.clone()),
                context_info: context_info.clone(),
                ..Default::default()
            },
        ),
        _ => whatsapp_rust::media::document_message(
            upload,
            whatsapp_rust::media::DocumentOptions {
                mimetype: Some(mimetype.clone()),
                file_name: file_name.clone(),
                title: file_name.clone(),
                caption: caption.clone(),
                context_info: context_info.clone(),
                ..Default::default()
            },
        ),
    };
    let media = extract_media(&message);
    let sent = client
        .send_message(to, message)
        .await
        .map_err(|e| e.to_string())?;
    let body = caption.unwrap_or_default();
    let message = IncomingMessage {
        view: MessageView {
            id: sent.message_id,
            chat_id,
            from_me: true,
            sender_name,
            sender_phone: None,
            sender_id: None,
            kind: MessageKind::Media,
            body,
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
        media,
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
    let mut view = message.view.clone();
    view.media = message.media.as_ref().map(StoredMedia::info);
    record_message(&app, &account, generation, message);
    Ok(view)
}

/// The newest attachments of a chat, for the "Media, links and docs" view.
#[tauri::command]
pub fn wa_native_chat_media(
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
) -> Result<Vec<MessageView>, String> {
    let account = state.get(&id)?;
    let media = account.db.lock().unwrap().media_messages(&chat_id, 300);
    media.map_err(|e| e.to_string())
}
