//! Group membership and settings, and join requests.

use super::*;

/// A change to a group, as sent by the info panel.
#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum GroupAction {
    SetSubject {
        subject: String,
    },
    SetDescription {
        description: String,
    },
    /// Only admins can send messages.
    SetAnnounce {
        on: bool,
    },
    /// Only admins can edit the group info.
    SetLocked {
        on: bool,
    },
    /// New members need an admin's approval.
    SetApproval {
        on: bool,
    },
    /// JPEG bytes, base64; the frontend crops and encodes it.
    SetPicture {
        jpeg: String,
    },
    RemovePicture,
    /// Phone numbers, any formatting.
    Add {
        phones: Vec<String>,
    },
    Remove {
        members: Vec<String>,
    },
    Promote {
        members: Vec<String>,
    },
    Demote {
        members: Vec<String>,
    },
    Approve {
        members: Vec<String>,
    },
    Reject {
        members: Vec<String>,
    },
    InviteLink {
        reset: bool,
    },
    Leave,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GroupActionResult {
    pub invite_link: Option<String>,
    /// People the change did not apply to, with the server's reason.
    pub failed: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinRequest {
    pub id: String,
    pub name: Option<String>,
    /// Whether `name` is from your contacts.
    pub saved: bool,
    pub phone: Option<String>,
    /// Unix milliseconds.
    pub requested_at: Option<i64>,
}

pub(super) fn parse_jids(ids: &[String]) -> Result<Vec<Jid>, String> {
    ids.iter()
        .map(|id| id.parse().map_err(|_| format!("invalid member id: {id}")))
        .collect()
}

/// Why a participant change did not go through, in words, or `None` when it did.
pub(super) fn participant_failure(r: &ParticipantChangeResponse) -> Option<String> {
    let status = r.status.as_deref().unwrap_or("200");
    if status == "200" && r.error.is_none() {
        return None;
    }
    let reason = match status {
        "403" => "their privacy settings don't allow it; send them an invite link instead",
        "408" => "they recently left the group",
        "409" => "already in the group",
        "404" => "not on WhatsApp",
        "401" => "you are not an admin",
        _ => r.error.as_deref().unwrap_or(status),
    };
    let who = r
        .phone_number
        .as_ref()
        .map(|pn| format!("+{}", pn.user))
        .unwrap_or_else(|| r.jid.user.to_string());
    Some(format!("{who}: {reason}"))
}

pub(super) fn failures(responses: &[ParticipantChangeResponse]) -> Vec<String> {
    responses.iter().filter_map(participant_failure).collect()
}

/// Applies a change to a group. Errors come back as text for the panel to show.
#[tauri::command]
pub async fn wa_native_group_action(
    app: AppHandle,
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
    action: GroupAction,
) -> Result<GroupActionResult, String> {
    use whatsapp_rust::features::{
        GroupDescription, GroupSubject, MembershipApprovalMode, PreviousDescription,
    };

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
    let groups = client.groups();
    let err = |e: whatsapp_rust::features::GroupError| e.to_string();
    let mut result = GroupActionResult::default();
    match action {
        GroupAction::SetSubject { subject } => {
            let subject = GroupSubject::new(subject.trim()).map_err(|e| e.to_string())?;
            groups.set_subject(jid, subject).await.map_err(err)?;
        }
        GroupAction::SetDescription { description } => {
            let description = description.trim();
            let description = if description.is_empty() {
                None
            } else {
                Some(GroupDescription::new(description).map_err(|e| e.to_string())?)
            };
            groups
                .set_description(jid, description, PreviousDescription::Resolve)
                .await
                .map_err(err)?;
        }
        GroupAction::SetAnnounce { on } => groups.set_announce(jid, on).await.map_err(err)?,
        GroupAction::SetLocked { on } => groups.set_locked(jid, on).await.map_err(err)?,
        GroupAction::SetApproval { on } => {
            let mode = if on {
                MembershipApprovalMode::On
            } else {
                MembershipApprovalMode::Off
            };
            groups
                .set_membership_approval(jid, mode)
                .await
                .map_err(err)?;
        }
        GroupAction::SetPicture { jpeg } => {
            use base64::Engine as _;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(jpeg)
                .map_err(|e| e.to_string())?;
            groups.set_profile_picture(jid, bytes).await.map_err(err)?;
        }
        GroupAction::RemovePicture => {
            groups.remove_profile_picture(jid).await.map_err(err)?;
        }
        GroupAction::Add { phones } => {
            let jids: Vec<Jid> = phones
                .iter()
                .map(|p| p.chars().filter(|c| c.is_ascii_digit()).collect::<String>())
                .filter(|digits| digits.len() >= 8)
                .filter_map(|digits| format!("{digits}@s.whatsapp.net").parse().ok())
                .collect();
            if jids.is_empty() {
                return Err("enter a phone number with country code".into());
            }
            result.failed = failures(&groups.add_participants(jid, &jids).await.map_err(err)?);
        }
        GroupAction::Remove { members } => {
            let jids = parse_jids(&members)?;
            result.failed = failures(&groups.remove_participants(jid, &jids).await.map_err(err)?);
        }
        GroupAction::Promote { members } => {
            let jids = parse_jids(&members)?;
            result.failed = failures(&groups.promote_participants(jid, &jids).await.map_err(err)?);
        }
        GroupAction::Demote { members } => {
            let jids = parse_jids(&members)?;
            result.failed = failures(&groups.demote_participants(jid, &jids).await.map_err(err)?);
        }
        GroupAction::Approve { members } => {
            let jids = parse_jids(&members)?;
            result.failed = failures(
                &groups
                    .approve_membership_requests(jid, &jids)
                    .await
                    .map_err(err)?,
            );
        }
        GroupAction::Reject { members } => {
            let jids = parse_jids(&members)?;
            result.failed = failures(
                &groups
                    .reject_membership_requests(jid, &jids)
                    .await
                    .map_err(err)?,
            );
        }
        GroupAction::InviteLink { reset } => {
            result.invite_link = Some(groups.get_invite_link(jid, reset).await.map_err(err)?);
        }
        GroupAction::Leave => groups.leave(jid).await.map_err(err)?,
    }
    emit_chats(&app, &account);
    Ok(result)
}

/// People waiting for an admin to let them into a group.
#[tauri::command]
pub async fn wa_native_group_requests(
    state: State<'_, WaState>,
    id: String,
    chat_id: String,
) -> Result<Vec<JoinRequest>, String> {
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
    let requests = client
        .groups()
        .get_membership_requests(jid)
        .await
        .map_err(|e| e.to_string())?;

    // Requests arrive as privacy ids, which alone carry neither a phone number nor a
    // name. Resolve the ones not mapped yet to phone-number ids in one batch so the panel
    // can show who is asking; the mapping is remembered, so later calls skip the lookup.
    let unmapped: Vec<Jid> = {
        let db = account.db.lock().unwrap();
        requests
            .iter()
            .filter(|r| {
                let id = bare_jid(&r.jid.to_string());
                id.ends_with("@lid") && db.pn_for(&id).ok().flatten().is_none()
            })
            .map(|r| r.jid.to_non_ad())
            .collect()
    };
    if !unmapped.is_empty() {
        if let Ok(found) = client.contacts().is_on_whatsapp(&unmapped).await {
            let db = account.db.lock().unwrap();
            for r in &found {
                if let Some(pn) = &r.pn_jid {
                    let _ =
                        db.set_lid_pn(&bare_jid(&r.jid.to_string()), &bare_jid(&pn.to_string()));
                }
            }
        }
    }

    let db = account.db.lock().unwrap();
    Ok(requests
        .into_iter()
        .map(|r| {
            let id = bare_jid(&r.jid.to_string());
            let who = db.who(&id).ok();
            JoinRequest {
                name: who.as_ref().and_then(|w| w.name.clone()),
                saved: who.as_ref().is_some_and(|w| w.saved),
                phone: who.and_then(|w| w.phone),
                requested_at: r.request_time.map(|t| t as i64 * 1000),
                id,
            }
        })
        .collect())
}

// ── Status (stories) ─────────────────────────────────────────────────────
