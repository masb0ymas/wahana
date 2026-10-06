//! Adding, starting, stopping and removing accounts, and the saved account list.

use super::*;

/// Writes the account list. Called with the accounts lock held so two concurrent changes
/// cannot land on disk in the opposite order from memory.
pub(super) fn save_accounts(
    app: &AppHandle,
    accounts: &HashMap<String, Arc<WaAccount>>,
) -> Result<(), String> {
    let unopened = app.state::<WaState>().unopened.lock().unwrap().clone();
    let list: Vec<StoredAccount> = accounts
        .values()
        .map(|a| StoredAccount {
            id: a.id.clone(),
            name: a.name.lock().unwrap().clone(),
        })
        .chain(unopened)
        .collect();
    let json = serde_json::to_vec_pretty(&list).map_err(|e| e.to_string())?;
    let dir = whatsapp_dir(app)?;
    // Write-then-rename, so a crash mid-write cannot leave a truncated list that would
    // orphan every session on the next launch.
    let tmp = dir.join(format!("{ACCOUNTS_FILE}.tmp"));
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dir.join(ACCOUNTS_FILE)).map_err(|e| e.to_string())
}

/// Loads the saved account list and starts every account, the way the WhatsApp Web panes
/// used to load on launch. A paired account reconnects from its session database without
/// a new QR.
pub fn restore(app: &AppHandle) {
    let Ok(dir) = whatsapp_dir(app) else { return };
    let Ok(bytes) = std::fs::read(dir.join(ACCOUNTS_FILE)) else {
        return;
    };
    let list: Vec<StoredAccount> = match serde_json::from_slice(&bytes) {
        Ok(list) => list,
        Err(e) => {
            eprintln!("ignoring unreadable WhatsApp account list: {e}");
            return;
        }
    };
    let state = app.state::<WaState>();
    let restored: Vec<Arc<WaAccount>> = {
        let mut accounts = state.accounts.lock().unwrap();
        let mut unopened = state.unopened.lock().unwrap();
        list.into_iter()
            .filter(|a| valid_id(&a.id))
            .filter_map(|stored| {
                match WaAccount::open(app, stored.id.clone(), stored.name.clone()) {
                    Ok(account) => {
                        let account = Arc::new(account);
                        accounts.insert(account.id.clone(), account.clone());
                        Some(account)
                    }
                    Err(e) => {
                        eprintln!("{e}");
                        unopened.push(stored);
                        None
                    }
                }
            })
            .collect()
    };
    for account in restored {
        if let Err(e) = start_account(app, account) {
            eprintln!("failed to start WhatsApp account: {e}");
        }
    }
}

#[tauri::command]
pub fn wa_native_accounts(state: State<'_, WaState>) -> Vec<AccountInfo> {
    let accounts = state.accounts.lock().unwrap();
    let mut list: Vec<AccountInfo> = accounts.values().map(|a| a.info()).collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    list
}

#[tauri::command]
pub fn wa_native_add(
    app: AppHandle,
    state: State<'_, WaState>,
    id: String,
    name: String,
) -> Result<AccountInfo, String> {
    if !valid_id(&id) {
        return Err("invalid WhatsApp account id".into());
    }
    let account = Arc::new(WaAccount::open(&app, id.clone(), name)?);
    {
        let mut accounts = state.accounts.lock().unwrap();
        if accounts.contains_key(&id) {
            return Err(format!("WhatsApp account already exists: {id}"));
        }
        accounts.insert(id.clone(), account.clone());
        if let Err(e) = save_accounts(&app, &accounts) {
            accounts.remove(&id);
            return Err(format!("failed to save WhatsApp accounts: {e}"));
        }
    }
    emit_account(&app, &account);
    Ok(account.info())
}

#[tauri::command]
pub async fn wa_native_start(
    app: AppHandle,
    state: State<'_, WaState>,
    id: String,
) -> Result<(), String> {
    start_account(&app, state.get(&id)?)
}

pub(super) fn start_account(app: &AppHandle, account: Arc<WaAccount>) -> Result<(), String> {
    let app = app.clone();
    let db_path = session_path(&app, &account.id)?;
    let (generation, reset_session) = {
        let mut inner = account.inner.lock().unwrap();
        // A second client on the same session would displace the first (the server closes
        // the older stream), so refuse rather than let the two fight.
        if inner.running {
            return Err("WhatsApp account is already running".into());
        }
        inner.running = true;
        inner.generation += 1;
        inner.status = WaStatus::Starting;
        inner.error = None;
        (inner.generation, std::mem::take(&mut inner.reset_session))
    };
    emit_account(&app, &account);
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_account(
            app.clone(),
            account.clone(),
            db_path,
            generation,
            reset_session,
        )
        .await
        {
            set_error(&app, &account, generation, error);
        }
    });
    Ok(())
}

#[tauri::command]
pub fn wa_native_rename(
    app: AppHandle,
    state: State<'_, WaState>,
    id: String,
    name: String,
) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("account name cannot be empty".into());
    }
    let account = {
        let accounts = state.accounts.lock().unwrap();
        let account = accounts
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("unknown WhatsApp account: {id}"))?;
        let previous = std::mem::replace(&mut *account.name.lock().unwrap(), name.to_string());
        if let Err(e) = save_accounts(&app, &accounts) {
            *account.name.lock().unwrap() = previous;
            return Err(format!("failed to save WhatsApp accounts: {e}"));
        }
        account
    };
    emit_account(&app, &account);
    Ok(())
}

#[tauri::command]
pub async fn wa_native_stop(
    app: AppHandle,
    state: State<'_, WaState>,
    id: String,
) -> Result<(), String> {
    let account = state.get(&id)?;
    let handle = {
        let mut inner = account.inner.lock().unwrap();
        inner.generation += 1;
        inner.running = false;
        inner.client = None;
        inner.status = WaStatus::Stopped;
        inner.me = None;
        inner.syncing = None;
        inner.handle.take()
    };
    if let Some(handle) = handle {
        tauri::async_runtime::spawn(async move {
            handle.shutdown().await;
        });
    }
    emit_account(&app, &account);
    Ok(())
}

/// Unlinks this device from the phone. The resulting `LoggedOut` event does the cleanup.
#[tauri::command]
pub async fn wa_native_logout(state: State<'_, WaState>, id: String) -> Result<(), String> {
    let account = state.get(&id)?;
    let client = account
        .inner
        .lock()
        .unwrap()
        .client
        .clone()
        .ok_or("start the WhatsApp account before logging it out")?;
    client.logout().await;
    Ok(())
}

#[tauri::command]
pub async fn wa_native_remove(
    app: AppHandle,
    state: State<'_, WaState>,
    id: String,
) -> Result<(), String> {
    let account = {
        let mut accounts = state.accounts.lock().unwrap();
        let account = accounts
            .remove(&id)
            .ok_or_else(|| format!("unknown WhatsApp account: {id}"))?;
        save_accounts(&app, &accounts)?;
        account
    };
    let handle = {
        let mut inner = account.inner.lock().unwrap();
        inner.generation += 1;
        inner.running = false;
        inner.client = None;
        inner.handle.take()
    };
    if let Some(handle) = handle {
        handle.shutdown().await;
    }
    remove_sqlite_files(&session_path(&app, &id)?);
    remove_sqlite_files(&chats_path(&app, &id)?);
    Ok(())
}
