//! Chat labels, synced with WhatsApp.

use super::*;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub id: String,
    pub name: String,
    /// WhatsApp color index, not a hex value.
    pub color: i64,
}

/// Labels cached from app-state sync, by name.
#[tauri::command]
pub fn wa_native_labels(state: State<'_, WaState>, id: String) -> Result<Vec<Label>, String> {
    let account = state.get(&id)?;
    let labels = account
        .db
        .lock()
        .unwrap()
        .labels()
        .map_err(|e| e.to_string())?;
    Ok(labels
        .into_iter()
        .map(|l| Label {
            id: l.id,
            name: l.name,
            color: l.color,
        })
        .collect())
}

/// Ids of the labels on one chat.
#[tauri::command]
pub fn wa_native_chat_labels(
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
) -> Result<Vec<String>, String> {
    let account = state.get(&id)?;
    let labels = account
        .db
        .lock()
        .unwrap()
        .chat_labels(&chat_id)
        .map_err(|e| e.to_string())?;
    Ok(labels)
}

/// Every chat's label ids, for the label chips in the chat list.
#[tauri::command]
pub fn wa_native_label_map(
    state: State<'_, WaState>,
    id: String,
) -> Result<HashMap<String, Vec<String>>, String> {
    let account = state.get(&id)?;
    let pairs = account
        .db
        .lock()
        .unwrap()
        .all_chat_labels()
        .map_err(|e| e.to_string())?;
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for (chat_id, label_id) in pairs {
        map.entry(chat_id).or_default().push(label_id);
    }
    Ok(map)
}

/// A fresh label id (WhatsApp uses numeric strings).
pub(super) fn new_label_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}", nanos)
}

/// Creates a label (or renames/recovers an existing one) and caches it.
#[tauri::command]
pub async fn wa_native_label_create(
    state: State<'_, WaState>,
    id: String,
    label_id: Option<String>,
    name: String,
    color: i64,
) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("label name is empty".into());
    }
    let account = state.get(&id)?;
    let client = account
        .inner
        .lock()
        .unwrap()
        .client
        .clone()
        .ok_or("WhatsApp account is not running")?;
    let label_id = label_id
        .filter(|s| !s.is_empty())
        .unwrap_or_else(new_label_id);
    client
        .labels()
        .create_label(&label_id, name, color as i32)
        .await
        .map_err(|e| e.to_string())?;
    let _ = account
        .db
        .lock()
        .unwrap()
        .upsert_label(&label_id, name, color, false);
    Ok(label_id)
}

/// Deletes a label and its associations.
#[tauri::command]
pub async fn wa_native_label_delete(
    state: State<'_, WaState>,
    id: String,
    label_id: String,
) -> Result<(), String> {
    let account = state.get(&id)?;
    let client = account
        .inner
        .lock()
        .unwrap()
        .client
        .clone()
        .ok_or("WhatsApp account is not running")?;
    client
        .labels()
        .delete_label(&label_id)
        .await
        .map_err(|e| e.to_string())?;
    let _ = account.db.lock().unwrap().delete_label(&label_id);
    Ok(())
}

/// Adds or removes a label on a chat.
#[tauri::command]
pub async fn wa_native_label_link(
    state: State<'_, WaState>,
    id: String,
    label_id: String,
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
    let labels = client.labels();
    let result = if on {
        labels.add_chat_label(&label_id, &jid).await
    } else {
        labels.remove_chat_label(&label_id, &jid).await
    };
    result.map_err(|e| e.to_string())?;
    let _ = account
        .db
        .lock()
        .unwrap()
        .set_chat_label(&label_id, &bare_jid(&chat_id), on);
    Ok(())
}
