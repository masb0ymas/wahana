//! Reading a message's content: text, media, polls, locations, contacts, quotes and previews.

use super::*;

/// The attachment of a message, if it has one: display details plus a message holding
/// only the media part, encoded, which keeps the keys needed to download it.
pub(super) fn extract_media(message: &wa::Message) -> Option<StoredMedia> {
    let base = message.get_base_message();
    let mut only = wa::Message::default();
    let mut media = if let Some(m) = base.image_message.as_option() {
        only.image_message = buffa::MessageField::some(m.clone());
        StoredMedia {
            kind: "image",
            mimetype: m.mimetype.clone().unwrap_or_else(|| "image/jpeg".into()),
            file_name: None,
            size: m.file_length,
            seconds: None,
            width: m.width,
            height: m.height,
            thumbnail: m.jpeg_thumbnail.clone(),
            proto: Vec::new(),
        }
    } else if let Some(m) = base
        .video_message
        .as_option()
        .or(base.ptv_message.as_option())
    {
        only.video_message = buffa::MessageField::some(m.clone());
        StoredMedia {
            kind: "video",
            mimetype: m.mimetype.clone().unwrap_or_else(|| "video/mp4".into()),
            file_name: None,
            size: m.file_length,
            seconds: m.seconds,
            width: m.width,
            height: m.height,
            thumbnail: m.jpeg_thumbnail.clone(),
            proto: Vec::new(),
        }
    } else if let Some(m) = base.audio_message.as_option() {
        only.audio_message = buffa::MessageField::some(m.clone());
        StoredMedia {
            kind: if m.ptt == Some(true) { "ptt" } else { "audio" },
            mimetype: m.mimetype.clone().unwrap_or_else(|| "audio/ogg".into()),
            file_name: None,
            size: m.file_length,
            seconds: m.seconds,
            width: None,
            height: None,
            thumbnail: None,
            proto: Vec::new(),
        }
    } else if let Some(m) = base.document_message.as_option().or_else(|| {
        base.document_with_caption_message
            .as_option()
            .and_then(|f| f.message.as_option())
            .and_then(|inner| inner.document_message.as_option())
    }) {
        only.document_message = buffa::MessageField::some(m.clone());
        StoredMedia {
            kind: "document",
            mimetype: m
                .mimetype
                .clone()
                .unwrap_or_else(|| "application/octet-stream".into()),
            file_name: m.file_name.clone().or_else(|| m.title.clone()),
            size: m.file_length,
            seconds: None,
            width: None,
            height: None,
            thumbnail: m.jpeg_thumbnail.clone(),
            proto: Vec::new(),
        }
    } else {
        let m = base.sticker_message.as_option()?;
        only.sticker_message = buffa::MessageField::some(m.clone());
        StoredMedia {
            kind: "sticker",
            mimetype: m.mimetype.clone().unwrap_or_else(|| "image/webp".into()),
            file_name: None,
            size: m.file_length,
            seconds: None,
            width: m.width,
            height: m.height,
            thumbnail: None,
            proto: Vec::new(),
        }
    };
    media.proto = only.encode_to_vec();
    Some(media)
}

/// What to show for a story mention whose story the sender did not include.
pub(super) fn story_mention_placeholder() -> (
    MessageKind,
    String,
    Option<StoredMedia>,
    Option<Interactive>,
) {
    (
        MessageKind::Text,
        "📣 Mentioned you in a story".to_string(),
        None,
        None,
    )
}

/// The text of a message plus its attachment, or `None` for protocol traffic (reactions,
/// revokes, key distribution) that rides on the message channel but is not a message of
/// its own. A poll, location or contact carries its structured content in the last element
/// and the same readable text in the second.
pub(super) fn message_content(
    message: &wa::Message,
) -> Option<(
    MessageKind,
    String,
    Option<StoredMedia>,
    Option<Interactive>,
)> {
    // An album member can arrive wrapped; the wrapper holds the photo or video itself.
    if let Some(child) = message
        .associated_child_message
        .as_option()
        .and_then(|f| f.message.as_option())
    {
        return message_content(child);
    }
    // A story mention carries no story of its own, just a reference to the status it names
    // (see `status_mention_of`); show the inner content if a sending client ever includes
    // it, else a label. The bubble opens the referenced story from `status_mention`.
    let mention = message
        .status_mention_message
        .as_option()
        .or(message.group_status_mention_message.as_option());
    if let Some(wrapper) = mention {
        let inner = wrapper
            .message
            .as_option()
            .and_then(message_content)
            .filter(|(kind, _, _, _)| !matches!(kind, MessageKind::Unsupported));
        return Some(inner.unwrap_or_else(story_mention_placeholder));
    }
    if let Some(media) = extract_media(message) {
        let caption = message.get_caption().unwrap_or_default().to_string();
        return Some((MessageKind::Media, caption, Some(media), None));
    }
    if let Some(text) = message.text_content() {
        return Some((MessageKind::Text, text.to_string(), None, None));
    }
    if let Some((kind, body, interactive)) = interactive_content(message) {
        return Some((kind, body, None, Some(interactive)));
    }
    let base = message.get_base_message();
    if base.protocol_message.is_set()
        || base.reaction_message.is_set()
        || base.sender_key_distribution_message.is_set()
        || base.album_message.is_set()
        // A vote updates its poll; it is not a message of its own.
        || base.poll_update_message.is_set()
    {
        return None;
    }
    Some((MessageKind::Unsupported, String::new(), None, None))
}

/// The structured content of a poll, location or contact message, with the same as readable
/// text. `None` for every other message. Wrappers (view-once, device-sent, …) are unwrapped
/// by `get_base_message`, the same way the protocol checks below rely on it.
pub(super) fn interactive_content(
    message: &wa::Message,
) -> Option<(MessageKind, String, Interactive)> {
    let base = message.get_base_message();
    // A v4 poll rides inside a `FutureProofMessage` wrapper, which `get_base_message` does not
    // unwrap. Peel it, then unwrap whatever it holds.
    let base = base
        .poll_creation_message_v4
        .as_option()
        .and_then(|w| w.message.as_option())
        .map(|inner| inner.get_base_message())
        .unwrap_or(base);
    if let Some(p) = base
        .poll_creation_message
        .as_option()
        .or(base.poll_creation_message_v2.as_option())
        .or(base.poll_creation_message_v3.as_option())
        .or(base.poll_creation_message_v5.as_option())
    {
        let question = p.name.clone().unwrap_or_default();
        let options: Vec<String> = p
            .options
            .iter()
            .filter_map(|o| o.option_name.clone())
            .filter(|o| !o.is_empty())
            .collect();
        // WhatsApp sends 1 for a single-choice poll and 0 when any number may be picked.
        let multiple = p.selectable_options_count.unwrap_or(0) != 1;
        let body = poll_body(&question, &options);
        return Some((
            MessageKind::Poll,
            body,
            Interactive {
                poll: Some(PollInfo {
                    question,
                    options,
                    multiple,
                    results: None,
                }),
                ..Default::default()
            },
        ));
    }
    if let Some(l) = base.location_message.as_option() {
        let (lat, lng) = (
            finite_degrees(l.degrees_latitude),
            finite_degrees(l.degrees_longitude),
        );
        let name = l.name.clone().filter(|s| !s.is_empty());
        let address = l.address.clone().filter(|s| !s.is_empty());
        let body = location_body(name.as_deref(), address.as_deref(), lat, lng);
        return Some((
            MessageKind::Location,
            body,
            Interactive {
                location: Some(LocationInfo {
                    latitude: lat,
                    longitude: lng,
                    name,
                    address,
                    live: l.is_live.unwrap_or(false),
                }),
                ..Default::default()
            },
        ));
    }
    if let Some(l) = base.live_location_message.as_option() {
        let (lat, lng) = (
            finite_degrees(l.degrees_latitude),
            finite_degrees(l.degrees_longitude),
        );
        let body = location_body(None, l.caption.as_deref(), lat, lng);
        return Some((
            MessageKind::Location,
            body,
            Interactive {
                location: Some(LocationInfo {
                    latitude: lat,
                    longitude: lng,
                    name: None,
                    address: None,
                    live: true,
                }),
                ..Default::default()
            },
        ));
    }
    if let Some(c) = base.contact_message.as_option() {
        let card = contact_card(c.display_name.as_deref(), c.vcard.as_deref());
        let body = contact_body(std::slice::from_ref(&card));
        return Some((
            MessageKind::Contact,
            body,
            Interactive {
                contacts: Some(vec![card]),
                ..Default::default()
            },
        ));
    }
    if let Some(ca) = base.contacts_array_message.as_option() {
        let cards: Vec<ContactCard> = ca
            .contacts
            .iter()
            .map(|c| contact_card(c.display_name.as_deref(), c.vcard.as_deref()))
            .collect();
        if !cards.is_empty() {
            return Some((
                MessageKind::Contact,
                contact_body(&cards),
                Interactive {
                    contacts: Some(cards),
                    ..Default::default()
                },
            ));
        }
    }
    None
}

/// The vCard property a line holds (`FN`, `TEL`, …) without its parameters or the group
/// prefix iOS adds (`item1.TEL;waid=…`), plus the line's parameters and value.
pub(super) fn vcard_line(line: &str) -> Option<(String, &str, &str)> {
    let (head, value) = line.trim().split_once(':')?;
    let (name, params) = head.split_once(';').unwrap_or((head, ""));
    let name = name.rsplit('.').next().unwrap_or(name).to_ascii_uppercase();
    Some((name, params, value))
}

/// One contact as a card: its display name, else the vCard's formatted name, plus the first
/// phone number the vCard carries.
pub(super) fn contact_card(display_name: Option<&str>, vcard: Option<&str>) -> ContactCard {
    let from_vcard = vcard.and_then(|v| {
        v.lines()
            .filter_map(vcard_line)
            .find(|(name, _, _)| name == "FN")
            .map(|(_, _, value)| value.trim())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    });
    let name = display_name
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or(from_vcard)
        .unwrap_or_else(|| "Contact".to_string());
    ContactCard {
        name,
        phone: vcard.and_then(phone_from_vcard),
    }
}

/// The first `TEL` number in a vCard, stripped of spacing. A `waid` parameter, when there, is
/// the WhatsApp number itself, so it wins over the formatted value.
pub(super) fn phone_from_vcard(vcard: &str) -> Option<String> {
    let (_, params, value) = vcard
        .lines()
        .filter_map(vcard_line)
        .find(|(name, _, _)| name == "TEL")?;
    let waid = params.split(';').find_map(|p| {
        let (key, value) = p.split_once('=')?;
        (key.eq_ignore_ascii_case("waid") && !value.is_empty()).then(|| format!("+{value}"))
    });
    let digits = waid.unwrap_or_else(|| value.trim().replace([' ', '-', '(', ')'], ""));
    // A number with no digits at all (`TEL:abc`) would open a chat with an empty JID.
    digits.chars().any(|c| c.is_ascii_digit()).then_some(digits)
}

/// A coordinate as WhatsApp sent it, or 0 when missing or not a finite number: NaN and
/// infinity serialize to JSON `null`, which the location card cannot show or open.
pub(super) fn finite_degrees(value: Option<f64>) -> f64 {
    value.filter(|v| v.is_finite()).unwrap_or_default()
}

/// A poll as readable text: its question, then one bullet per option.
pub(super) fn poll_body(question: &str, options: &[String]) -> String {
    let question = if question.is_empty() {
        "Poll"
    } else {
        question
    };
    let mut body = question.to_string();
    for option in options {
        body.push_str(&format!("\n• {option}"));
    }
    body
}

/// A location as readable text: its name and address, then the coordinates.
pub(super) fn location_body(
    name: Option<&str>,
    address: Option<&str>,
    lat: f64,
    lng: f64,
) -> String {
    let mut body = String::new();
    for line in [name, address].into_iter().flatten() {
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(line);
    }
    if !body.is_empty() {
        body.push('\n');
    }
    body.push_str(&format!("{lat}, {lng}"));
    body
}

/// Shared contacts as readable text: one line per contact.
pub(super) fn contact_body(cards: &[ContactCard]) -> String {
    cards
        .iter()
        .map(|c| match &c.phone {
            Some(phone) => format!("{} — {phone}", c.name),
            None => c.name.clone(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The album a photo or video was sent in: the id of its album message, which every member
/// names as its parent.
pub(super) fn album_of(message: &wa::Message) -> Option<String> {
    let association = message
        .message_context_info
        .as_option()
        .or_else(|| message.get_base_message().message_context_info.as_option())
        .and_then(|c| c.message_association.as_option())?;
    if association.association_type != Some(wa::message_association::AssociationType::MEDIA_ALBUM) {
        return None;
    }
    association
        .parent_message_key
        .as_option()
        .and_then(|k| k.id.clone())
}

/// The link preview WhatsApp embedded in a message. The extended text message carries the
/// first URL plus the title, description and a small JPEG its servers fetched, so the bubble
/// can draw the same card without fetching the page itself.
pub(super) fn link_preview(message: &wa::Message) -> Option<PreviewInfo> {
    use base64::Engine as _;
    let ext = message
        .get_base_message()
        .extended_text_message
        .as_option()?;
    let url = ext.matched_text.clone().filter(|u| !u.is_empty())?;
    if ext.title.is_none() && ext.description.is_none() && ext.jpeg_thumbnail.is_none() {
        return None;
    }
    Some(PreviewInfo {
        url,
        title: ext.title.clone(),
        description: ext.description.clone(),
        image: ext.jpeg_thumbnail.as_deref().map(|bytes| {
            format!(
                "data:image/jpeg;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            )
        }),
    })
}

/// The message a reply quotes, from the reply context any message type may carry.
pub(super) fn quote_of(message: &wa::Message) -> Option<QuoteRef> {
    let base = message.get_base_message();
    let context = base
        .extended_text_message
        .as_option()
        .and_then(|m| m.context_info.as_option())
        .or_else(|| {
            base.image_message
                .as_option()
                .and_then(|m| m.context_info.as_option())
        })
        .or_else(|| {
            base.video_message
                .as_option()
                .and_then(|m| m.context_info.as_option())
        })
        .or_else(|| {
            base.audio_message
                .as_option()
                .and_then(|m| m.context_info.as_option())
        })
        .or_else(|| {
            base.document_message
                .as_option()
                .and_then(|m| m.context_info.as_option())
        })
        .or_else(|| {
            base.sticker_message
                .as_option()
                .and_then(|m| m.context_info.as_option())
        })?;
    let id = context.stanza_id.clone().filter(|id| !id.is_empty())?;
    let text = context
        .quoted_message
        .as_option()
        .and_then(message_content)
        .map(|(kind, body, media, _)| preview(kind, &body, media.as_ref().map(|m| m.kind)))
        .unwrap_or_default();
    Some(QuoteRef {
        id,
        sender: context
            .participant
            .as_deref()
            .map(bare_jid)
            .unwrap_or_default(),
        text,
        chat: context
            .remote_jid
            .as_deref()
            .filter(|jid| *jid == "status@broadcast")
            .map(str::to_string),
    })
}

/// The status a story mention points at. The mention itself carries no story — only a
/// `protocolMessage` of type `STATUS_MENTION_MESSAGE` keyed to the status in
/// `status@broadcast`, whether it names you (1:1) or a group you are in. The referenced
/// story is the one shown in the status viewer.
pub(super) fn status_mention_of(message: &wa::Message) -> Option<String> {
    let inner = message
        .status_mention_message
        .as_option()
        .or(message.group_status_mention_message.as_option())?
        .message
        .as_option()?;
    let protocol = inner.protocol_message.as_option()?;
    if protocol.r#type != Some(wa::message::protocol_message::Type::StatusMentionMessage) {
        return None;
    }
    let key = protocol.key.as_option()?;
    if key.remote_jid.as_deref() != Some("status@broadcast") {
        return None;
    }
    key.id.clone().filter(|id| !id.is_empty())
}

#[cfg(test)]
mod content_tests {
    use super::*;
    use whatsapp_rust::waproto::buffa::MessageField;
    use whatsapp_rust::waproto::whatsapp as wa;

    fn poll(name: &str, options: &[&str]) -> wa::Message {
        poll_choosing(name, options, 1)
    }

    fn poll_choosing(name: &str, options: &[&str], selectable: u32) -> wa::Message {
        wa::Message {
            poll_creation_message_v3: MessageField::some(wa::message::PollCreationMessage {
                name: Some(name.to_string()),
                selectable_options_count: Some(selectable),
                options: options
                    .iter()
                    .map(|o| wa::message::poll_creation_message::Option {
                        option_name: Some((*o).to_string()),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn a_v3_poll_is_read_as_a_poll() {
        let (kind, body, media, interactive) =
            message_content(&poll("Lunch?", &["Yes", "No"])).unwrap();
        assert!(matches!(kind, MessageKind::Poll));
        assert!(media.is_none());
        assert_eq!(body, "Lunch?\n• Yes\n• No");
        let poll = interactive.and_then(|i| i.poll).unwrap();
        assert_eq!(poll.options, vec!["Yes", "No"]);
        assert!(!poll.multiple);
    }

    #[test]
    fn a_v4_wrapped_poll_is_unwrapped() {
        let wrapped = wa::Message {
            poll_creation_message_v4: MessageField::some(wa::message::FutureProofMessage {
                message: MessageField::some(poll("Pick", &["A"])),
            }),
            ..Default::default()
        };
        let (kind, _, _, interactive) = message_content(&wrapped).unwrap();
        assert!(matches!(kind, MessageKind::Poll));
        assert_eq!(interactive.and_then(|i| i.poll).unwrap().question, "Pick");
    }

    #[test]
    fn a_location_is_read_with_its_coordinates() {
        let message = wa::Message {
            location_message: MessageField::some(wa::message::LocationMessage {
                degrees_latitude: Some(1.5),
                degrees_longitude: Some(-2.25),
                name: Some("Office".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let (kind, body, _, interactive) = message_content(&message).unwrap();
        assert!(matches!(kind, MessageKind::Location));
        assert!(body.contains("1.5, -2.25"));
        let location = interactive.and_then(|i| i.location).unwrap();
        assert_eq!(location.longitude, -2.25);
    }

    #[test]
    fn a_contact_is_read_with_its_phone() {
        let message = wa::Message {
            contact_message: MessageField::some(wa::message::ContactMessage {
                display_name: Some("Bob".to_string()),
                vcard: Some(
                    "BEGIN:VCARD\nFN:Bob\nTEL;type=CELL;waid=123:+1 234-567\nEND:VCARD".to_string(),
                ),
                ..Default::default()
            }),
            ..Default::default()
        };
        let (kind, _, _, interactive) = message_content(&message).unwrap();
        assert!(matches!(kind, MessageKind::Contact));
        let contacts = interactive.and_then(|i| i.contacts).unwrap();
        assert_eq!(contacts[0].name, "Bob");
        assert_eq!(contacts[0].phone.as_deref(), Some("+123"));
    }

    #[test]
    fn an_ios_vcard_phone_is_found_behind_its_group_prefix() {
        let card = contact_card(
            None,
            Some("BEGIN:VCARD\nFN:Ann: Work\nitem1.TEL:+62 812-3456\nEND:VCARD"),
        );
        assert_eq!(card.name, "Ann: Work");
        assert_eq!(card.phone.as_deref(), Some("+628123456"));
    }

    #[test]
    fn a_vcard_number_without_digits_is_no_phone() {
        let card = contact_card(Some("Eve"), Some("BEGIN:VCARD\nTEL:abc\nEND:VCARD"));
        assert_eq!(card.phone, None);
    }

    #[test]
    fn a_non_finite_coordinate_falls_back_to_zero() {
        assert_eq!(finite_degrees(Some(f64::NAN)), 0.0);
        assert_eq!(finite_degrees(Some(f64::INFINITY)), 0.0);
        assert_eq!(finite_degrees(None), 0.0);
        assert_eq!(finite_degrees(Some(-6.2)), -6.2);
    }

    #[test]
    fn a_poll_open_to_any_number_of_answers_is_multiple() {
        let (_, _, _, interactive) =
            message_content(&poll_choosing("Pick", &["A", "B"], 0)).unwrap();
        assert!(interactive.and_then(|i| i.poll).unwrap().multiple);
    }

    #[test]
    fn a_poll_vote_is_not_a_message() {
        let message = wa::Message {
            poll_update_message: MessageField::some(Default::default()),
            ..Default::default()
        };
        assert!(message_content(&message).is_none());
    }
}
