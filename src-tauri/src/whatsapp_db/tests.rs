use super::*;

fn temp_db(name: &str) -> (ChatDb, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!("wahana-{name}-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    (ChatDb::open(&path).unwrap(), path)
}

#[test]
fn reopens_after_an_older_build_stamped_the_version_down() {
    let (db, path) = temp_db("downgraded");
    // What an older release does on open: its own, lower version over a newer schema.
    db.conn.execute_batch("PRAGMA user_version = 10").unwrap();
    drop(db);
    let db = ChatDb::open(&path).unwrap();
    let version: i64 = db
        .conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
    drop(db);

    let db = ChatDb::open(&path).unwrap();
    db.conn
        .execute_batch(&format!("PRAGMA user_version = {}", SCHEMA_VERSION + 1))
        .unwrap();
    drop(db);
    // A newer build's stamp is left alone.
    let db = ChatDb::open(&path).unwrap();
    let version: i64 = db
        .conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION + 1);
    let _ = std::fs::remove_file(&path);
}

fn add_message(db: &ChatDb, chat: &str, id: &str, body: &str, timestamp: i64) {
    db.conn
        .execute(
            "INSERT INTO messages (chat_id, id, from_me, sender_id, sender_name, kind, body, timestamp)
             VALUES (?1, ?2, 0, 'a@c.us', 'A', 'text', ?3, ?4)",
            params![chat, id, body, timestamp],
        )
        .unwrap();
}

fn last(db: &ChatDb, chat: &str) -> (String, i64) {
    db.conn
        .query_row(
            "SELECT last_text, last_timestamp FROM chats WHERE id = ?1",
            params![chat],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
}

#[test]
fn story_mention_target_survives_a_round_trip() {
    let (db, path) = temp_db("mention");
    let message = IncomingMessage {
        view: MessageView {
            id: "m1".to_string(),
            chat_id: "g@g.us".to_string(),
            from_me: false,
            sender_name: "A".to_string(),
            sender_phone: None,
            sender_id: None,
            kind: MessageKind::Text,
            body: "📣 Mentioned you in a story".to_string(),
            timestamp: 1,
            media: None,
            ack: 0,
            revoked_at: None,
            edited_at: None,
            edits: Vec::new(),
            channel_reactions: Vec::new(),
            reply_to: None,
            status_mention: Some("status1".to_string()),
            album_id: None,
            preview: None,
            interactive: None,
        },
        sender_id: "a@c.us".to_string(),
        media: None,
        quote: None,
        album: None,
        poll_key: None,
    };
    db.insert_message(&message, false).unwrap();
    let messages = db.messages("g@g.us", 10).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].status_mention.as_deref(), Some("status1"));
    drop(db);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn interactive_poll_survives_a_round_trip() {
    let (db, path) = temp_db("interactive");
    let message = IncomingMessage {
        view: MessageView {
            id: "p1".to_string(),
            chat_id: "g@g.us".to_string(),
            from_me: false,
            sender_name: "A".to_string(),
            sender_phone: None,
            sender_id: None,
            kind: MessageKind::Poll,
            body: "Lunch?\n• Yes\n• No".to_string(),
            timestamp: 1,
            media: None,
            ack: 0,
            revoked_at: None,
            edited_at: None,
            edits: Vec::new(),
            channel_reactions: Vec::new(),
            reply_to: None,
            status_mention: None,
            album_id: None,
            preview: None,
            interactive: Some(Interactive {
                poll: Some(crate::whatsapp::PollInfo {
                    question: "Lunch?".to_string(),
                    options: vec!["Yes".to_string(), "No".to_string()],
                    multiple: false,
                    results: None,
                }),
                ..Default::default()
            }),
        },
        sender_id: "a@c.us".to_string(),
        media: None,
        quote: None,
        album: None,
        poll_key: Some(PollKey {
            secret: vec![7; 32],
            creator: Some("a@lid".to_string()),
        }),
    };
    db.insert_message(&message, false).unwrap();
    let messages = db.messages("g@g.us", 10).unwrap();
    assert_eq!(messages.len(), 1);
    assert!(matches!(messages[0].kind, MessageKind::Poll));
    let poll = messages[0]
        .interactive
        .as_ref()
        .and_then(|i| i.poll.as_ref());
    assert_eq!(
        poll.map(|p| p.options.clone()),
        Some(vec!["Yes".to_string(), "No".to_string()])
    );
    assert!(poll
        .and_then(|p| p.results.as_ref())
        .is_some_and(|r| r.can_vote));
    let target = db.poll_target("other@g.us", "p1").unwrap().unwrap();
    assert_eq!(target.chat_id, "g@g.us");
    assert_eq!(target.secret.as_deref(), Some(&[7u8; 32][..]));
    assert_eq!(target.creator.as_deref(), Some("a@lid"));
    drop(db);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn poll_votes_keep_the_latest_choice_per_voter() {
    let (db, path) = temp_db("poll_votes");
    db.set_lid_pn("b@lid", "1@s.whatsapp.net").unwrap();
    db.set_name("b@lid", "Budi", NameSource::Contact).unwrap();
    let options = vec!["Yes".to_string(), "No".to_string()];
    let yes = vec!["Yes".to_string()];
    let no = vec!["No".to_string()];
    // Voted under the phone number, then changed their mind under the privacy id.
    assert!(db
        .set_poll_vote("p1", "1@s.whatsapp.net", &yes, 10)
        .unwrap());
    assert!(db.set_poll_vote("p1", "b@lid", &no, 20).unwrap());
    // A late copy of the older vote does not undo the newer one.
    assert!(!db.set_poll_vote("p1", "b@lid", &yes, 15).unwrap());
    db.set_poll_vote("p1", "me", &yes, 30).unwrap();
    let results = db.poll_results("p1", &options, true).unwrap();
    assert_eq!(
        results.voters,
        vec![Vec::<String>::new(), vec!["Budi".to_string()]]
    );
    assert_eq!(results.mine, yes);
    // Withdrawing a vote leaves it out of the tally.
    db.set_poll_vote("p1", "b@lid", &[], 40).unwrap();
    let results = db.poll_results("p1", &options, true).unwrap();
    assert!(results.voters.iter().all(Vec::is_empty));
    drop(db);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn repair_rebuilds_future_chat_from_real_messages() {
    let (db, path) = temp_db("repair");
    let now = now_millis();
    let future = now + 30 * 24 * 60 * 60 * 1000;
    db.ensure_chat("x@c.us", future, 0).unwrap();
    add_message(&db, "x@c.us", "1", "old", now - 5000);
    add_message(&db, "x@c.us", "2", "newest real", now - 1000);
    add_message(&db, "x@c.us", "3", "corrupt", future);
    db.ensure_chat("empty@c.us", future, 0).unwrap();
    drop(db);

    let db = ChatDb::open(&path).unwrap();
    assert_eq!(last(&db, "x@c.us"), ("newest real".to_string(), now - 1000));
    assert_eq!(last(&db, "empty@c.us").1, 0);
    drop(db);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn device_read_state_clears_and_flags_unread() {
    let (db, path) = temp_db("read");
    db.ensure_chat("g@g.us", 1, 4).unwrap();
    assert!(db.set_read_from_device("g@g.us", true).unwrap());
    assert!(!db.set_read_from_device("g@g.us", true).unwrap());
    assert!(db.set_read_from_device("g@g.us", false).unwrap());
    assert_eq!(db.unread_chats(), 1);
    drop(db);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn whatsapp_pin_is_found_under_either_id_and_can_be_forgotten() {
    let (db, path) = temp_db("pins");
    db.set_lid_pn("111@lid", "62811@s.whatsapp.net").unwrap();
    db.set_pin("111@lid", 1_700).unwrap();
    assert_eq!(db.pin_for("62811@s.whatsapp.net").unwrap(), Some(1_700));
    db.set_pin("111@lid", 0).unwrap();
    assert_eq!(db.pin_for("111@lid").unwrap(), Some(0));
    db.forget_pin("62811@s.whatsapp.net").unwrap();
    assert_eq!(db.pin_for("111@lid").unwrap(), None);
    drop(db);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn repair_leaves_plausible_chats_alone() {
    let (db, path) = temp_db("keep");
    let now = now_millis();
    db.ensure_chat("ok@c.us", now, 0).unwrap();
    drop(db);
    let db = ChatDb::open(&path).unwrap();
    assert_eq!(last(&db, "ok@c.us").1, now);
    drop(db);
    let _ = std::fs::remove_file(&path);
}
