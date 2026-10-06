//! Statuses (stories): viewing, posting and deleting them.

use super::*;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusView {
    /// The poster's bare id (empty for your own status).
    pub sender: String,
    #[serde(flatten)]
    pub message: MessageView,
}

/// The newest statuses, newest first; the screen keeps the last 24 hours.
#[tauri::command]
pub fn wa_native_statuses(
    state: State<'_, WaState>,
    id: String,
) -> Result<Vec<StatusView>, String> {
    let account = state.get(&id)?;
    let list = account
        .db
        .lock()
        .unwrap()
        .statuses(500)
        .map_err(|e| e.to_string())?;
    Ok(list
        .into_iter()
        .map(|(sender, message)| StatusView { sender, message })
        .collect())
}

/// Tells WhatsApp a status was viewed (the poster sees you in "viewed by").
#[tauri::command]
pub async fn wa_native_status_viewed(
    state: State<'_, WaState>,
    id: String,
    sender: String,
    message_id: String,
) -> Result<(), String> {
    let account = state.get(&id)?;
    let client = account
        .inner
        .lock()
        .unwrap()
        .client
        .clone()
        .ok_or("WhatsApp account is not running")?;
    let sender: Jid = sender
        .parse()
        .map_err(|_| format!("invalid sender: {sender}"))?;
    client
        .mark_as_read(
            &Jid::status_broadcast(),
            Some(&sender),
            &[message_id.as_str()],
        )
        .await
        .map_err(|e| e.to_string())
}

/// Everyone a status is posted to: the saved contacts (phone-number ids).
pub(super) fn status_audience(account: &WaAccount) -> Result<Vec<Jid>, String> {
    let me = account
        .inner
        .lock()
        .unwrap()
        .me
        .as_ref()
        .map(|me| bare_jid(&me.id));
    let pns = account
        .db
        .lock()
        .unwrap()
        .status_recipients(me.as_deref())
        .map_err(|e| e.to_string())?;
    let jids: Vec<Jid> = pns.iter().filter_map(|p| p.parse::<Jid>().ok()).collect();
    if jids.is_empty() {
        return Err("no saved contacts to send the status to".into());
    }
    Ok(jids)
}

/// Posts a text status to every saved contact. `background_argb` is 0xAARRGGBB.
#[tauri::command]
pub async fn wa_native_post_status_text(
    state: State<'_, WaState>,
    id: String,
    text: String,
    background_argb: u32,
) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("status text is empty".into());
    }
    let account = state.get(&id)?;
    let client = account
        .inner
        .lock()
        .unwrap()
        .client
        .clone()
        .ok_or("WhatsApp account is not running")?;
    let recipients = status_audience(&account)?;
    client
        .status()
        .send_text(
            text,
            background_argb,
            wa::message::extended_text_message::FontType::SYSTEM,
            &recipients,
            Default::default(),
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Posts a photo or video status to every saved contact. Raw body, with the same headers as
/// `wa_native_send_media` plus an optional base64 JPEG thumbnail (`x-thumb`).
#[tauri::command]
pub async fn wa_native_post_status_media(
    state: State<'_, WaState>,
    request: Request<'_>,
) -> Result<(), String> {
    let header = |name: &str| {
        request
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(percent_decode)
    };
    let id = header("x-account").ok_or("missing x-account")?;
    let mimetype = header("x-mime").unwrap_or_else(|| "application/octet-stream".into());
    let caption = header("x-caption").filter(|c| !c.trim().is_empty());
    let thumbnail = header("x-thumb").and_then(|t| {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.decode(t).ok()
    });
    let data = match request.body() {
        InvokeBody::Raw(bytes) => bytes.clone(),
        InvokeBody::Json(_) => return Err("expected raw body".into()),
    };

    let media_type = if mimetype.starts_with("image/") && mimetype != "image/gif" {
        MediaType::Image
    } else if mimetype.starts_with("video/") {
        MediaType::Video
    } else {
        return Err("a status can only be a photo or a video".into());
    };
    let account = state.get(&id)?;
    let client = account
        .inner
        .lock()
        .unwrap()
        .client
        .clone()
        .ok_or("WhatsApp account is not running")?;
    let recipients = status_audience(&account)?;
    let upload = client
        .upload(data, media_type, whatsapp_rust::UploadOptions::new())
        .await
        .map_err(|e| format!("upload failed: {e}"))?;
    let message = match media_type {
        MediaType::Image => whatsapp_rust::media::image_message(
            upload,
            whatsapp_rust::media::ImageOptions {
                caption,
                mimetype: Some(mimetype),
                jpeg_thumbnail: thumbnail,
                ..Default::default()
            },
        ),
        _ => whatsapp_rust::media::video_message(
            upload,
            whatsapp_rust::media::VideoOptions {
                caption,
                mimetype: Some(mimetype),
                jpeg_thumbnail: thumbnail,
                ..Default::default()
            },
        ),
    };
    client
        .status()
        .send_raw(message, &recipients, Default::default())
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Deletes one of my statuses (for everyone), and the local copy.
#[tauri::command]
pub async fn wa_native_delete_status(
    state: State<'_, WaState>,
    id: String,
    message_id: String,
) -> Result<(), String> {
    let account = state.get(&id)?;
    let client = account
        .inner
        .lock()
        .unwrap()
        .client
        .clone()
        .ok_or("WhatsApp account is not running")?;
    let recipients = status_audience(&account)?;
    client
        .status()
        .revoke(message_id.clone(), &recipients, Default::default())
        .await
        .map_err(|e| e.to_string())?;
    let _ = account
        .db
        .lock()
        .unwrap()
        .delete_message("status@broadcast", &message_id);
    Ok(())
}

// ── Chat labels and pinning ──────────────────────────────────────────────
