//! The contact, group and channel info panels.

use super::*;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactDetails {
    pub id: String,
    pub name: Option<String>,
    pub saved: bool,
    pub phone: Option<String>,
    /// Their "About" text, unless their privacy settings hide it.
    pub about: Option<String>,
    pub business: bool,
    pub verified_name: Option<String>,
    /// Full-size profile picture URL.
    pub picture: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupMember {
    pub id: String,
    pub name: Option<String>,
    pub saved: bool,
    pub phone: Option<String>,
    pub admin: bool,
    pub super_admin: bool,
    pub is_me: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupDetails {
    pub id: String,
    pub subject: String,
    pub description: Option<String>,
    /// Unix milliseconds.
    pub created_at: Option<i64>,
    pub creator: Option<GroupMember>,
    /// Only admins can send messages.
    pub announce: bool,
    /// Only admins can edit the group info.
    pub locked: bool,
    /// New members need an admin's approval.
    pub approval: bool,
    pub members: Vec<GroupMember>,
    pub picture: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelDetails {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    /// `None` when the server would not give the details (see `warning`).
    pub subscribers: Option<u64>,
    pub verified: bool,
    /// Why the details are partial, with the underlying error.
    pub warning: Option<String>,
    pub invite_link: Option<String>,
    /// "owner", "admin", "subscriber" or "guest".
    pub role: Option<String>,
    /// Unix milliseconds.
    pub created_at: Option<i64>,
    pub picture: Option<String>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ChatDetails {
    Contact(ContactDetails),
    Group(GroupDetails),
    Channel(ChannelDetails),
}

pub(super) fn channel_details(meta: &NewsletterMetadata) -> ChannelDetails {
    ChannelDetails {
        id: bare_jid(&meta.jid.to_string()),
        name: meta.name.clone(),
        description: meta.description.clone().filter(|d| !d.trim().is_empty()),
        subscribers: Some(meta.subscriber_count),
        verified: meta.verification == NewsletterVerification::Verified,
        warning: None,
        invite_link: meta
            .invite_code
            .as_ref()
            .map(|code| format!("https://whatsapp.com/channel/{code}")),
        role: meta.role.as_ref().map(|r| {
            match r {
                NewsletterRole::Owner => "owner",
                NewsletterRole::Admin => "admin",
                NewsletterRole::Subscriber => "subscriber",
                NewsletterRole::Guest => "guest",
                _ => "guest",
            }
            .to_string()
        }),
        created_at: meta.creation_time.map(|t| t as i64 * 1000),
        picture: meta
            .picture_url
            .clone()
            .or_else(|| meta.preview_url.clone()),
    }
}

/// Full-size profile picture; `None` when there is none or it is hidden.
pub(super) async fn full_picture(client: &Client, jid: &Jid) -> Option<String> {
    client
        .contacts()
        .get_profile_picture(jid, false)
        .await
        .ok()
        .flatten()
        .map(|p| p.url)
}

/// Contact or group details for the info panel, fetched live from the server and named
/// from this account's contacts.
#[tauri::command]
pub async fn wa_native_chat_info(
    app: AppHandle,
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
) -> Result<ChatDetails, String> {
    let account = state.get(&id)?;
    let (client, me) = {
        let inner = account.inner.lock().unwrap();
        let client = inner
            .client
            .clone()
            .ok_or("WhatsApp account is not running")?;
        (client, inner.me.as_ref().map(|me| bare_jid(&me.id)))
    };
    let jid: Jid = chat_id
        .parse()
        .map_err(|_| format!("invalid chat id: {chat_id}"))?;

    if chat_id.ends_with("@newsletter") {
        // The single-channel lookup is sometimes refused; the subscribed list carries the same
        // data. If both fail, still open the panel (unfollow and mute work) and say why.
        let meta = match crate::channel_mex::get(&client, &jid).await {
            Ok(meta) => Ok(meta),
            Err(first) => match crate::channel_mex::subscribed(&client).await {
                Ok(all) => all
                    .into_iter()
                    .find(|m| bare_jid(&m.jid.to_string()) == chat_id)
                    .ok_or(first),
                Err(second) => Err(format!("{first}; subscribed list: {second}")),
            },
        };
        let meta = match meta {
            Ok(meta) => meta,
            Err(why) => {
                let name = account
                    .db
                    .lock()
                    .unwrap()
                    .who(&chat_id)
                    .ok()
                    .and_then(|w| w.name)
                    .unwrap_or_else(|| chat_id.split('@').next().unwrap_or_default().to_string());
                return Ok(ChatDetails::Channel(ChannelDetails {
                    id: chat_id,
                    name,
                    description: None,
                    subscribers: None,
                    verified: false,
                    warning: Some(why),
                    invite_link: None,
                    role: None,
                    created_at: None,
                    picture: None,
                }));
            }
        };
        let _ = account
            .db
            .lock()
            .unwrap()
            .set_name(&chat_id, &meta.name, NameSource::GroupSubject);
        emit_chats(&app, &account);
        return Ok(ChatDetails::Channel(channel_details(&meta)));
    }

    if chat_id.ends_with("@g.us") {
        let meta = client
            .groups()
            .get_metadata(&jid)
            .await
            .map_err(|e| e.to_string())?;
        // Participant lists carry both ids of each member; keep the mapping for names.
        {
            let db = account.db.lock().unwrap();
            for p in &meta.participants {
                if let (Some(pn), Some(lid)) = (&p.phone_number, &p.lid) {
                    let _ = db.set_lid_pn(&bare_jid(&lid.to_string()), &bare_jid(&pn.to_string()));
                }
            }
            let _ = db.set_name(&chat_id, &meta.subject, NameSource::GroupSubject);
        }
        let member = |id: String, admin: bool, super_admin: bool| -> GroupMember {
            let db = account.db.lock().unwrap();
            let who = db.who(&id).ok();
            let pn = db.pn_for(&id).ok().flatten();
            GroupMember {
                is_me: me.is_some() && (me.as_deref() == Some(id.as_str()) || me == pn),
                name: who.as_ref().and_then(|w| w.name.clone()),
                saved: who.as_ref().is_some_and(|w| w.saved),
                phone: who.and_then(|w| w.phone),
                id,
                admin,
                super_admin,
            }
        };
        let mut members: Vec<GroupMember> = meta
            .participants
            .iter()
            .map(|p| {
                member(
                    bare_jid(&p.jid.to_string()),
                    p.is_admin(),
                    p.is_super_admin(),
                )
            })
            .collect();
        // You first, then admins, then everyone by name — the order WhatsApp uses.
        members.sort_by(|a, b| {
            b.is_me
                .cmp(&a.is_me)
                .then(b.admin.cmp(&a.admin))
                .then(b.saved.cmp(&a.saved))
                .then(
                    a.name
                        .as_deref()
                        .unwrap_or("~")
                        .to_lowercase()
                        .cmp(&b.name.as_deref().unwrap_or("~").to_lowercase()),
                )
        });
        let creator = meta
            .creator
            .as_ref()
            .map(|c| member(bare_jid(&c.to_string()), false, false));
        emit_chats(&app, &account);
        return Ok(ChatDetails::Group(GroupDetails {
            id: chat_id,
            subject: meta.subject,
            description: meta.description.filter(|d| !d.trim().is_empty()),
            created_at: meta.creation_time.map(|t| t as i64 * 1000),
            creator,
            announce: meta.is_announcement,
            locked: meta.is_locked,
            approval: meta.membership_approval,
            members,
            picture: full_picture(&client, &jid).await,
        }));
    }

    let (who, pn) = {
        let db = account.db.lock().unwrap();
        (
            db.who(&chat_id).map_err(|e| e.to_string())?,
            db.pn_for(&chat_id).map_err(|e| e.to_string())?,
        )
    };
    // Profile queries answer for phone-number ids most reliably.
    let query_jid: Jid = pn
        .as_deref()
        .and_then(|pn| pn.parse().ok())
        .unwrap_or_else(|| jid.clone());
    let info = client
        .contacts()
        .get_user_info(std::slice::from_ref(&query_jid))
        .await
        .ok()
        .and_then(|mut found| {
            found
                .remove(&query_jid)
                .or_else(|| found.into_values().next())
        });
    Ok(ChatDetails::Contact(ContactDetails {
        id: chat_id,
        name: who.name,
        saved: who.saved,
        phone: who.phone,
        about: info
            .as_ref()
            .and_then(|i| i.status.clone())
            .filter(|s| !s.trim().is_empty()),
        business: info.as_ref().is_some_and(|i| i.is_business),
        verified_name: info
            .as_ref()
            .and_then(|i| i.verified_name.as_ref())
            .and_then(|v| v.name.clone()),
        picture: full_picture(&client, &query_jid).await,
    }))
}
