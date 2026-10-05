//! Clickable desktop notifications.
//!
//! `tauri-plugin-notification` only supports click handling on mobile, and on desktop its
//! transport discards the notification handle. Notifications that must return to a chat
//! therefore go straight through `notify-rust` here, which keeps the handle and reports the
//! click on both Windows (WinRT toast) and macOS (`NSUserNotification`).

use notify_rust::{Notification, NotificationResponse};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Payload of `wahana:notification-click`, naming the chat to bring up.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationClick {
    account_id: String,
    chat_id: String,
}

/// Shows a desktop notification that, when clicked, focuses the window and emits
/// `wahana:notification-click` so the UI opens the chat it came from.
#[tauri::command]
pub fn notify_chat(
    app: AppHandle,
    title: String,
    body: String,
    account_id: String,
    chat_id: String,
) {
    // Waiting for a click blocks until the user acts (or the toast auto-dismisses), so keep
    // it off the command thread.
    std::thread::spawn(move || {
        let click = NotificationClick {
            account_id,
            chat_id,
        };
        // macOS only reports the interaction when the notification carries an action; clicking
        // the body works the same way there. The action is harmless on Windows (an "Open" button).
        let mut notification = Notification::new();
        notification
            .summary(&title)
            .body(&body)
            .action("open", "Open");
        // The toast must be attributed to the app's AppUserModelID for activation to route back.
        #[cfg(target_os = "windows")]
        notification.app_id(&tauri::Manager::config(&app).identifier);
        let Ok(handle) = notification.show() else {
            return;
        };
        let _ = handle.wait_for_response(|response: &NotificationResponse| {
            if matches!(
                response,
                NotificationResponse::Default | NotificationResponse::Action(_)
            ) {
                crate::show_main(&app);
                let _ = app.emit("wahana:notification-click", click.clone());
            }
        });
    });
}
