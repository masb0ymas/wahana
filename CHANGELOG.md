# Changelog

## 1.1.8 — 2026-10-05

### Added

- A refresh button in the chat list header (the Chats tab and every grid cell) to re-read the list on demand; the icon spins while it reloads.
- Blur a chat for privacy: right-click a chat → "Blur preview" blurs its name, last-message preview and message bubbles until you hover them, and raises a generic "New message" notification instead of the content.
- Media auto-loads are queued, so attachments download gradually instead of in one burst.

### Changed

- Notifications name the chat like the chat list does: a saved contact by name, anyone else by their number, and a group by its subject with the sender leading the body.
- The Accounts screen shows plain-word statuses (Connected, Connecting…, Waiting for scan, Stopped, Logged out, Failed), the error when a connection fails, and a hint on a logged-out account.

### Fixed

- A WhatsApp connection that never reaches a QR code or connects is now reported as failed after 40 seconds with a hint, instead of sitting at "starting" forever.
- Channel media that carries a media key without an enc hash now downloads.

## 1.1.7 — 2026-10-04

### Added

- Tell your linked accounts apart: assign each one a colour and an emoji icon on the Accounts screen. The Chats badge now shows one small badge per account with unread, coloured by that account, instead of a single combined number; the tray tooltip lists unread per account; notifications carry the account's icon; and the account picker and multi-account tiles show the icon and colour. Accounts you have not styled get a stable automatic colour.
- Check for updates by hand from Settings → About, with the result shown inline (up to date / available / error) and an install action when one is found.

### Changed

- Media auto-download (images, stickers, videos, audio) is set per account under Features → Tweaks ("Apply to"), instead of one global switch; accounts without an override keep the built-in default, and the settings are included in backups.

## 1.1.6 — 2026-10-04

### Added

- Start a chat: a "New chat" button in the chat list (the Chats tab and every multi-account grid cell). Search your existing chats or type a phone number with its country code (no "+" needed) to open a brand-new chat.
- "Suggest reply" in a message's right-click menu: the AI drafts three replies to the message you clicked, built from the chat's context; pick one to drop it into the composer.
- A story mention now shows a card that opens the mentioned status in the status viewer, instead of only "📣 Mentioned you in a story".
- Profile pictures in the "Read by" and "Delivered to" lists of message info.

### Changed

- AI goes through the official OpenAI SDK; the provider picker is gone and the base URL is optional (defaults to `api.openai.com/v1`).
- "Read by" and "Delivered to" in message info collapse and expand.

### Fixed

- Reasoning models (gpt-5, o-series, deepseek-r1, GLM-5…) work again: they get thinking headroom and `max_completion_tokens`, and an empty answer is reported as an error instead of a blank reply.
- "Reply privately" sends a real cross-chat quote and opens the same person's existing chat (privacy id or phone number) instead of creating a duplicate.
- Message info receipts show the correct number (no stray "+"), the phone under the name, and newest first.
- Knowledge, quick replies and AI are scoped strictly per account; legacy global entries are migrated on launch.

## 1.1.5 — 2026-10-04

### Added

- Multi-account grid (⌘7): several WhatsApp accounts side by side, each cell with its own chat list and conversation, at the number of columns and rows you choose (extra accounts scroll). Right-click a chat to pin, mute or label it; opening a chat shows it in the cell, with a back button to the list.
- The account's own profile picture in the grid cells and on the Accounts screen.
- Contributors shown in the README.

### Changed

- Each grid cell reuses the full chat screen, so search, filter tabs, pin/mute, labels, the right-click menu, read-all and select behave exactly like the Chats tab.
- Per-account message, label and open-chat tracking: one account's activity no longer refreshes or suppresses notifications for the others.

### Fixed

- Opening a chat in a grid cell clears its unread badge, and blue ticks still follow the read-receipt setting (Tweaks).

## 1.1.4 — 2026-10-04

### Added

- Media screen (⌘6): browse the media saved on this computer by session and type (images, videos, audio, documents) with file sizes, sorted by newest or largest. Delete single files, a selection, everything shown, or all media of a session from the right-click menu. Removed accounts that still have media keep their own "removed" session.

### Changed

- Each cached file now gets a `.meta` sidecar (account, chat, mimetype, file name, time) so it can be grouped; files cached earlier show as Unknown. Eviction removes the sidecar too.

## 1.1.3 — 2026-10-04

### Fixed

- Linked accounts no longer ask for a new QR scan after an update. Opening the chat history with an older version stamped its schema version down, and the next newer version then failed to open the account and showed the welcome screen instead.
- An account whose chat history fails to open at launch now stays in the account list instead of being dropped the next time the list is saved.
- The welcome screen's "Link WhatsApp" card is centered again.

### Changed

- README (English and Chinese) lists @mentions, status reply and download, and the Private / Community filter tabs.
- Dependabot opens a weekly PR when `whatsapp-rust` has a new release.

## 1.1.2 — 2026-10-04

### Added

- Community tab: groups are listed under their parent WhatsApp Community.
- Private tab: show one-to-one chats only.
- Reorder the chat filter tabs by dragging; the order is remembered.

### Fixed

- The macOS tray label no longer shows a stale count when nothing is unread.

## 1.1.1 — 2026-10-03

### Added

- @mention group members in the composer: type `@` to pick a member; the message tags them like WhatsApp does.
- Download button for contacts' status stories (photo/video), also on the `d` key.
- Reply to a contact's status from the story viewer; the reply quotes the story in their chat.

## 1.1.0 — 2026-10-03

### Added

- Knowledge base for AI auto-replies: tables with custom columns and free-text documents, embedded through an OpenAI-compatible `/embeddings` endpoint (shared with chat or its own URL/key) and given to the AI as authoritative facts. Entries are per account or global.
- Per-rule "Use knowledge base" toggle with top-k and minimum-similarity settings, test retrieval, stale/model-mismatch detection and re-index all.
- Simplified Chinese README.

### Changed

- Backup v3 includes knowledge base entries (without vectors) and the embeddings key; restore re-indexes in the background.

## 1.0.0 — 2026-10-03

### Removed

- WAHA server integration. The app now links WhatsApp accounts directly only (native, via whatsapp-rust): the API client, session/profile settings, WebSocket, webhooks/events screens and the WAHA-only stores are gone.

### Changed

- Scheduler, broadcast and auto-reply work from native accounts only.
- About, Backup & restore, Welcome and Settings are native-only.

## 0.9.0 — 2026-10-03

### Added

- Your own native stories show who viewed them.
- Full-screen button on videos in chat bubbles (native and WAHA) and in the story viewer; the lightbox can take the whole window full screen (F).
- Clicking a quoted story in a native chat opens it in the story viewer.

### Changed

- AI image descriptions are always written in My language and use WhatsApp formatting instead of raw Markdown.
- About shows native accounts and the WAHA server separately, and states that Wahana is an unofficial client (also in the README).

## 0.8.0 — 2026-10-03

### Added

- Pin a message for 24 hours, 7 days or 30 days (native and WAHA), like WhatsApp.
- Pins made on the phone or by other members show up in the pin banner: live, and from history (native history sync; WAHA loaded pages). Pin notices no longer show as empty bubbles; the chat list shows "📌 Pinned a message".
- Drafts are kept per chat in native chats, and chat lists show "Draft:" for unsent text.
- Search in group join requests (native and WAHA); requests are sorted newest first.
- Backup now includes auto-reply rules, theme, saved stickers and native account names (restored accounts ask for a fresh QR scan).

### Fixed

- A corrupt future timestamp no longer pins a chat to the top or freezes its preview.
- Reading a chat on the phone clears it here too (native).
- Replies and statuses show contact names instead of privacy ids (LID).
- Group participant menus float above the list instead of being clipped.
- Restoring chat preferences from a backup no longer gets overwritten by the next change; schedules keep their anchor.

## 0.7.0 — 2026-10-03

### Added

- Stickers: a tray next to the emoji button with **Recent** (collected from stickers you send or receive) and **Saved** (add any PNG/JPG/GIF/WebP; it is converted to a 512×512 WebP). Click to send, in native and WAHA chats.
- Paste a screenshot or copied image anywhere in the chat, not only inside the text box; it becomes an attachment (native) or is sent straight away (WAHA).
- Native chat: mute for 8 hours, 1 week or always, with a bell icon in the list and a control in the info panel; mutes sync both ways with the phone, including those set before linking.
- Native chat: story mentions, group member profiles, admin delete for everyone, reply privately.
- WAHA chats: copy number button, Chat and Reply privately in the group menu, group admins can delete others' messages.
- Channels: react to posts, load history, reaction totals.

### Changed

- The composer's icon buttons are smaller so the text box is wider.
- The "Suggest replies" strip can be hidden with an X until the next message.

## 0.6.0 — 2026-10-02

### Added

- Native chat: link messages show WhatsApp's own preview (thumbnail, title, description, URL) inside the bubble, with the live Open Graph fetch kept as a fallback.

### Fixed

- Native chat: opening a chat crashed with `undefined is not an object (evaluating 't.albumId')` when the newest messages were a photo/video album.

## 0.5.0 — 2026-10-02

### Added

- Channels: info panel, follow from a link, unfollow, mute, react to posts, load history from the server. Owners and admins can post, edit, delete, and edit the channel profile.
- Native chat: quoted replies, group member avatars, photo/video albums shown as a grid, composer focuses when replying.
- Native chat list: multi-select, mark all read, delete chats. Per-recipient read times on messages.
- Appearance setting with System, Light, and Dark themes.
- WAHA server profiles can be disabled without deleting them.
- Features page groups Scheduler, Broadcast, Auto-reply, Tweaks, and Quick replies.

### Fixed

- Channel metadata lookup failing on whatsapp-rust 0.7.0 (workaround for the upstream newsletter bug).

## 0.4.9 — 2026-10-02

### Added

- Native WhatsApp accounts: link a number directly (no WAHA server), with chats, media, groups, status, labels, chat pins, message actions, and sent/delivered/read ticks.
- Deleted and edited native messages stay visible, with their edit history.
- Pinned-message banner for WAHA and native chats. On WAHA it loads older history to reach the pin.
- Typing bubble in the open chat when the other side is typing or recording (WAHA and native).

### Changed

- Scheduler, Broadcast, Auto-reply, Sessions, and per-account settings (read receipts, typing, AI persona, quick replies, auto-label, link previews) work for both WAHA sessions and native accounts.
- WAHA auto-reply runs for every configured server, listening only to sessions with an enabled rule.

### Fixed

- Message action permissions, multi-server send routing, and backups missing some AI settings.

## 0.4.8 — 2026-09-18

### Fixed

- "Draft with AI" (broadcast, status) wrote in the _Target language_ (the other side's) instead of _My language_.

### Changed

- README and release notes explain the macOS Gatekeeper "damaged" message and the `xattr -cr` fix, and which build (`aarch64` / `x64`) to pick.

## 0.4.7 — 2026-09-18

### Added

- Welcome screen on first run: start with WhatsApp Web in one click, or connect a WAHA server (URL + API key, with Test connection). Reopen it any time from Settings → About.
- WhatsApp Web: unread count per session — shown on the pane's title bar, in the session picker, and included in the dock/taskbar badge and tray tooltip.

### Changed

- Internal: `SettingsScreen.tsx` (1,180 lines) split into one module per section under `src/screens/settings/`; no behaviour change.

## 0.4.6 — 2026-09-17

### Added

- WhatsApp Web split view: drag the divider between two panes to resize them (double-click to reset to equal widths). Sizes are remembered.
- WhatsApp Web split view: drag a pane's title bar onto another pane to reorder them.
- WhatsApp Web split view: choose how many rows the panes are laid out in (Auto / 1–4).

### Changed

- WhatsApp Web split view is responsive: panes never shrink below 400px; when the window can't fit them all in one row they wrap onto additional rows instead of squeezing.

### Fixed

- Windows: dropping a file onto the composer did nothing (Tauri's native drag-drop handler swallowed the drop).

## 0.4.5 — 2026-09-17

### Changed

- Windows: the setup wizard (NSIS `.exe` header/sidebar and MSI banner/dialog) now shows the Wahana logo instead of the default installer artwork.

## 0.4.4 — 2026-09-17

### Fixed

- Windows: options in native dropdowns (session picker, language selects) were white-on-white in dark mode.

### Changed

- New installs default the AI provider to "OpenAI-compatible" with an empty model; a saved Anthropic setup is unchanged.

## 0.4.3 — 2026-09-17

### Fixed

- A message bubble that changed state in place (e.g. deleted for everyone while on screen) could crash with a hook-order error.

### Changed

- ESLint (typescript-eslint + react-hooks) and Prettier run in `npm run check` and in CI; the whole tree is formatted once.
- The conversation view is split into a header, a search bar and two hooks (`useOrderedMessages`, `useMessageList` for scrolling / paging / virtualisation); `ChatScreen.tsx` is now the glue only.
- Shared helpers replace repeated snippets: `errMsg`, `convKey`, `ViewMessage`, `mediaOpts`, `useLatest`/`useEvent`/`useDismiss`, and a `Popover`/`MenuItem` dropdown that closes on outside click and Escape.

## 0.4.2 — 2026-09-15

### Fixed

- Broadcasts created under one server profile no longer run through another profile's connection after switching servers.
- Scrolling up in a chat could page the same history range twice and show duplicate messages.
- The selected chat is cleared when switching sessions; drafts no longer leak between sessions.
- Read receipts are re-sent only when a new incoming message arrives, not every time older history loads.
- Scheduler claims a job before sending, so a second app instance or a crash right after a send cannot send it twice.
- Monthly schedules keep the originally chosen day (a "31st" schedule no longer drifts to the 28th after February).
- A weekly schedule with an empty weekday list keeps its original weekday instead of disabling itself.
- AI requests time out after 60 s instead of leaving a chat's auto-reply stuck.
- Media cache file names are hashed, so two different message ids can no longer map to the same cached file.
- WhatsApp Web: the child webview is created once even when the layout syncs several times at mount; removing a session waits for its browsing data to be cleared before closing.
- WhatsApp Web on macOS 11–13: the picker only offers one account (per-account isolation needs macOS 14).
- Unread badges, notifications and auto-replies ignore replayed / duplicate socket events.
- The WebSocket refetches chats and messages after a reconnect, and reconnects when the network returns or the window is used again after a long idle.

### Added

- "Draft with AI" in the broadcast and status composers: a short brief becomes a ready-to-send message in your persona's voice.
- Optional automatic labelling of new direct chats with your existing labels (Settings → AI, off by default).
- Daily cap on auto-replies (default 300, editable in the Auto-reply header) as a spend guard for AI replies.
- Broadcasts pause automatically after 5 consecutive failures.
- Content-Security-Policy for the app window.
- Unit tests (`npm test`) and a CI workflow that type-checks, tests, builds, and runs clippy/rustfmt on pull requests.

### Changed

- The message list is virtualised: only the bubbles near the viewport are in the DOM, so long scroll-backs no longer accumulate thousands of live bubbles and images.
- `ChatScreen.tsx` split into list / bubble / composer modules; Rust shell split into keychain, media-cache and WhatsApp Web modules (no behaviour change).
- Old schedule runs, auto-reply log rows and finished broadcasts are pruned after 90 days.
- Linux (AppImage / .deb) is built by the release workflow.
- Auto-reply prompts mark the chat transcript as untrusted data (prompt-injection hardening).
- Link previews are fetched only for public http(s) hosts and the preview cache is bounded.
- Chat bubbles are memoised; the session poll no longer re-renders every message.
- Media cache eviction runs on a background thread and only after a batch of new writes.

## 0.4.1 — 2026-09-14

- Fix WhatsApp Web never loading on Windows (WebView2 nested user-data folder) and the settings-page scroll freeze it caused.
- Forced exit fallback when quitting from the tray with a stuck event loop.

## 0.4.0 — 2026-09-14

- Multi-business: session picker for schedules and broadcasts, persona and quick replies per session.
- Auto-reply rules (fixed text or AI) with preview, cooldown, log and kill switch.
- WhatsApp Web split view: several accounts side by side.

## 0.3.0 — 2026-09-14

- Embedded WhatsApp Web sessions in the session picker with OS notifications, downloads and external links.

## 0.2.0 — 2026-09-13

- AI: translation, summaries and Q&A, writing assistant, smart replies, persona, image description / OCR, task extraction, label suggestions.
- Settings backup / restore, per-chat auto-translate.

## 0.1.0 — 2026-09-13

- Initial release: chats, groups, status, scheduler, broadcast, quick replies, labels, exports, webhooks.
