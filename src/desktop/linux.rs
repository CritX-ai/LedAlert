//! Passive Linux D-Bus metadata collection. Never acquires desktop service names.

use super::*;
use futures_util::{StreamExt, stream::FuturesUnordered};
use serde::{
    Deserialize, Deserializer,
    de::{IgnoredAny, MapAccess, Visitor},
};
use std::{collections::HashMap, fmt};
use tokio::time::{sleep, timeout};
use zbus::{
    Connection, MatchRule, Message, MessageStream, Proxy, fdo,
    message::Type as MessageType,
    proxy::MethodFlags,
    zvariant::{OwnedValue, Signature, Type, Value},
};

const RPC_TIMEOUT: Duration = Duration::from_millis(500);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const LOCK_INTERVAL: Duration = Duration::from_millis(250);
const MEDIA_INTERVAL: Duration = Duration::from_millis(750);
const PLAYER_TIMEOUT: Duration = Duration::from_millis(750);
const INITIAL_BACKOFF: Duration = Duration::from_millis(500);
const MAX_BACKOFF: Duration = Duration::from_secs(15);
const NOTIFICATION_REPLY_TIMEOUT: Duration = Duration::from_secs(5);
const NOTIFICATION_SWEEP_INTERVAL: Duration = Duration::from_millis(250);
const MAX_NOTIFICATION_BYTES: usize = 64 * 1024;
const MAX_METADATA_INPUT: usize = 1024;
const MAX_APPLICATION_BYTES: usize = 128;
const MAX_PLAYERS: usize = 32;
const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2.";
const MPRIS_PATH: &str = "/org/mpris/MediaPlayer2";
const MPRIS_ROOT: &str = "org.mpris.MediaPlayer2";
const MPRIS_PLAYER: &str = "org.mpris.MediaPlayer2.Player";
const NOTIFICATIONS: &str = "org.freedesktop.Notifications";
const NOTIFICATION_PATH: &str = "/org/freedesktop/Notifications";
const MONITORING: &str = "Monitoring notifications (metadata only)";

pub(super) async fn run(shared: &Shared, notifications: &mpsc::SyncSender<NotificationEvent>) {
    tokio::join!(
        notification_loop(shared, notifications),
        session_loop(shared),
    );
}

async fn session_connection() -> zbus::Result<Connection> {
    zbus::connection::Builder::session()?
        .max_queued(64)
        .method_timeout(RPC_TIMEOUT)
        .build()
        .await
}

async fn notification_loop(shared: &Shared, notifications: &mpsc::SyncSender<NotificationEvent>) {
    let mut backoff = INITIAL_BACKOFF;
    loop {
        let started = Instant::now();
        let status = observe_notifications(shared, notifications).await;
        notification_discontinuity(shared, status);
        if started.elapsed() >= Duration::from_secs(5) {
            backoff = INITIAL_BACKOFF;
        }
        sleep(backoff).await;
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

async fn observe_notifications(
    shared: &Shared,
    notifications: &mpsc::SyncSender<NotificationEvent>,
) -> &'static str {
    let connection = match timeout(CONNECT_TIMEOUT, session_connection()).await {
        Ok(Ok(connection)) => connection,
        _ => return "Notifications unavailable: session bus connection failed; retrying",
    };
    let owner_rule = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .sender("org.freedesktop.DBus")
        .expect("constant sender")
        .interface("org.freedesktop.DBus")
        .expect("constant interface")
        .path("/org/freedesktop/DBus")
        .expect("constant path")
        .member("NameOwnerChanged")
        .expect("constant member")
        .add_arg(NOTIFICATIONS)
        .expect("constant argument")
        .build();
    // Subscribe before the owner query; otherwise a replacement between query and BecomeMonitor
    // could leave us trusting the former owner. The single ordered stream preserves changes
    // during setup. Earlier owner changes are harmless: no Notify calls arrive before monitoring.
    let mut stream = MessageStream::from(&connection);
    let owner = match timeout(RPC_TIMEOUT, async {
        let bus = fdo::DBusProxy::new(&connection).await?;
        bus.add_match_rule(owner_rule.clone()).await?;
        match bus
            .get_name_owner(NOTIFICATIONS.try_into().expect("constant bus name"))
            .await
        {
            Ok(owner) => Ok(Some(owner.to_string())),
            Err(fdo::Error::NameHasNoOwner(_)) => Ok(None),
            Err(error) => Err(error),
        }
    })
    .await
    {
        Ok(Ok(owner)) => owner,
        _ => return "Notifications unavailable: daemon identity unknown; retrying",
    };
    let notify_rule = MatchRule::builder()
        .msg_type(MessageType::MethodCall)
        .interface(NOTIFICATIONS)
        .expect("constant interface")
        .path(NOTIFICATION_PATH)
        .expect("constant path")
        .member("Notify")
        .expect("constant member")
        .build();
    let closed_rule = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .sender(NOTIFICATIONS)
        .expect("constant sender")
        .interface(NOTIFICATIONS)
        .expect("constant interface")
        .path(NOTIFICATION_PATH)
        .expect("constant path")
        .member("NotificationClosed")
        .expect("constant member")
        .build();
    // Returns have no interface/member/path fields. The narrowest static reply subscription is
    // the current well-known daemon sender; the bus tracks its owner. Decode only replies whose
    // destination and reply serial match one of our bounded, metadata-only pending calls.
    let reply_rule = |kind| {
        MatchRule::builder()
            .msg_type(kind)
            .sender(NOTIFICATIONS)
            .expect("constant sender")
            .build()
    };
    let rules = [
        notify_rule,
        closed_rule,
        reply_rule(MessageType::MethodReturn),
        reply_rule(MessageType::Error),
        owner_rule,
    ];
    // BecomeMonitor replaces the setup subscription atomically. Never add/remove match rules
    // or send any messages on this connection after it becomes a monitor.
    let monitor = match fdo::MonitoringProxy::new(&connection).await {
        Ok(proxy) => proxy,
        Err(_) => return "Notifications unavailable: monitoring interface unavailable; retrying",
    };
    match timeout(RPC_TIMEOUT, monitor.become_monitor(&rules, 0)).await {
        Ok(Ok(())) => {}
        Ok(Err(fdo::Error::AccessDenied(_) | fdo::Error::AuthFailed(_))) => {
            return "Notifications unavailable: D-Bus monitoring permission denied; retrying";
        }
        Ok(Err(
            fdo::Error::UnknownMethod(_)
            | fdo::Error::UnknownInterface(_)
            | fdo::Error::NotSupported(_),
        )) => {
            return "Notifications unavailable: bus does not support passive monitoring; retrying";
        }
        _ => return "Notifications unavailable: monitor setup failed or timed out; retrying",
    }
    let mut observer = NotificationObserver {
        owner,
        generation: shared
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .desktop
            .notification_generation,
        pending: Vec::new(),
        closed: HashMap::new(),
    };
    let mut expiry = tokio::time::interval(NOTIFICATION_SWEEP_INTERVAL);
    expiry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    notification_status(shared, MONITORING);
    loop {
        let message = tokio::select! {
            _ = connection.closed() => break,
            _ = expiry.tick() => {
                if observer.pending.iter().any(|call| {
                    call.metadata.received.elapsed() >= NOTIFICATION_REPLY_TIMEOUT
                }) {
                    observer.reset(
                        shared,
                        "Monitoring notifications; reply timed out, lifecycle reset",
                    );
                }
                continue;
            }
            message = stream.next() => message,
        };
        match message {
            Some(Ok(message)) => {
                if !observer.observe(&message, shared, notifications) {
                    return "Notification receiver closed";
                }
            }
            _ => break,
        }
    }
    "Notifications disconnected; reconnecting"
}

struct NotificationMetadata {
    application: String,
    urgency: u8,
    received: Instant,
}

struct PendingNotification {
    caller: String,
    serial: u32,
    metadata: NotificationMetadata,
}

struct NotificationObserver {
    owner: Option<String>,
    generation: u64,
    // Never retain Notify messages, bodies, icons, actions, or arbitrary hint values.
    pending: Vec<PendingNotification>,
    // A daemon may close a notification before returning its ID. Replay that close after the
    // matching raise, without confusing it with a new call received after the close.
    closed: HashMap<u32, (Instant, u32)>,
}

impl NotificationObserver {
    fn reset(&mut self, shared: &Shared, status: &str) {
        self.pending.clear();
        self.closed.clear();
        self.generation = notification_discontinuity(shared, status);
    }

    fn send(
        &mut self,
        event: NotificationEvent,
        shared: &Shared,
        notifications: &mpsc::SyncSender<NotificationEvent>,
    ) -> bool {
        match notifications.try_send(event) {
            Ok(()) => true,
            Err(mpsc::TrySendError::Full(_)) => {
                self.reset(
                    shared,
                    "Monitoring notifications; queue full, lifecycle reset",
                );
                true
            }
            Err(mpsc::TrySendError::Disconnected(_)) => false,
        }
    }

    fn observe(
        &mut self,
        message: &Message,
        shared: &Shared,
        notifications: &mpsc::SyncSender<NotificationEvent>,
    ) -> bool {
        let header = message.header();
        let sender = header.sender().map(|name| name.as_str());
        if message.message_type() == MessageType::Signal
            && sender == Some("org.freedesktop.DBus")
            && header
                .interface()
                .is_some_and(|name| name.as_str() == "org.freedesktop.DBus")
            && header
                .path()
                .is_some_and(|path| path.as_str() == "/org/freedesktop/DBus")
            && header
                .member()
                .is_some_and(|name| name.as_str() == "NameOwnerChanged")
        {
            if let Ok((name, _, owner)) = message.body().deserialize::<(&str, &str, &str)>()
                && name == NOTIFICATIONS
            {
                self.owner = (!owner.is_empty()).then(|| owner.to_owned());
                self.reset(
                    shared,
                    "Monitoring notifications (metadata only); daemon changed",
                );
            }
            return true;
        }
        if is_notify(message) {
            let Some(owner) = self.owner.as_deref() else {
                return true;
            };
            if !header.destination().is_some_and(|destination| {
                destination.as_str() == NOTIFICATIONS || destination.as_str() == owner
            }) {
                return true;
            }
            let Some(sender) = sender else {
                return true;
            };
            let Some(metadata) = notification_metadata(message) else {
                // It could replace an already lit ID; dropping only the new metadata is unsafe.
                self.reset(
                    shared,
                    "Monitoring notifications; oversized or invalid event, lifecycle reset",
                );
                return true;
            };
            let serial = message.primary_header().serial_num().get();
            if self
                .pending
                .iter()
                .any(|call| call.caller == sender && call.serial == serial)
            {
                self.reset(
                    shared,
                    "Monitoring notifications; ambiguous reply serial, lifecycle reset",
                );
                return true;
            }
            if self.pending.len() == NOTIFICATION_CAPACITY {
                self.reset(
                    shared,
                    "Monitoring notifications; in-flight calls lost, lifecycle reset",
                );
            }
            self.pending.push(PendingNotification {
                caller: sender.to_owned(),
                serial,
                metadata,
            });
            return true;
        }
        if sender.is_none() || sender != self.owner.as_deref() {
            return true;
        }
        if message.message_type() == MessageType::Signal
            && header
                .interface()
                .is_some_and(|name| name.as_str() == NOTIFICATIONS)
            && header
                .path()
                .is_some_and(|path| path.as_str() == NOTIFICATION_PATH)
            && header
                .member()
                .is_some_and(|name| name.as_str() == "NotificationClosed")
        {
            let Ok((id, reason)) = message.body().deserialize::<(u32, u32)>() else {
                self.reset(
                    shared,
                    "Monitoring notifications; invalid close, lifecycle reset",
                );
                return true;
            };
            if !self.pending.is_empty() {
                if self.closed.len() == NOTIFICATION_CAPACITY && !self.closed.contains_key(&id) {
                    self.reset(
                        shared,
                        "Monitoring notifications; early closes lost, lifecycle reset",
                    );
                } else {
                    self.closed.insert(id, (Instant::now(), reason));
                }
            }
            return self.send(
                NotificationEvent::Closed {
                    generation: self.generation,
                    id,
                    reason,
                },
                shared,
                notifications,
            );
        }
        if !matches!(
            message.message_type(),
            MessageType::MethodReturn | MessageType::Error
        ) {
            return true;
        }
        let (Some(destination), Some(serial)) = (header.destination(), header.reply_serial())
        else {
            return true;
        };
        // At most 64 entries; a borrowed lookup and swap-remove avoid copying caller metadata.
        let call = self
            .pending
            .iter()
            .position(|call| call.caller == destination.as_str() && call.serial == serial.get());
        let Some(call) = call else {
            return true;
        };
        let metadata = self.pending.swap_remove(call).metadata;
        let result = if message.message_type() == MessageType::Error {
            true
        } else if let Ok(id) = message.body().deserialize::<u32>()
            && id != 0
        {
            let early_close = self
                .closed
                .get(&id)
                .copied()
                .filter(|(closed, _)| *closed >= metadata.received);
            let generation = self.generation;
            let raised = NotificationEvent::Raised(Notification {
                id,
                generation,
                application: metadata.application,
                urgency: metadata.urgency,
                received: metadata.received,
            });
            if !self.send(raised, shared, notifications) {
                return false;
            }
            if let Some((_, reason)) = early_close
                && self.generation == generation
            {
                self.send(
                    NotificationEvent::Closed {
                        generation,
                        id,
                        reason,
                    },
                    shared,
                    notifications,
                )
            } else {
                true
            }
        } else {
            self.reset(
                shared,
                "Monitoring notifications; invalid daemon reply, lifecycle reset",
            );
            true
        };
        if self.pending.is_empty() {
            self.closed.clear();
        }
        result
    }
}

fn is_notify(message: &Message) -> bool {
    let header = message.header();
    message.message_type() == MessageType::MethodCall
        && header
            .interface()
            .is_some_and(|name| name.as_str() == "org.freedesktop.Notifications")
        && header
            .path()
            .is_some_and(|path| path.as_str() == "/org/freedesktop/Notifications")
        && header
            .member()
            .is_some_and(|name| name.as_str() == "Notify")
}

// IgnoredAny traverses and discards values without retaining strings, actions, images, or body
// text. Only the two allowlisted hint values are decoded as variants; nothing here is logged.
#[derive(Deserialize)]
struct NotifyBody<'a> {
    #[serde(borrow)]
    application: &'a str,
    _replacement: IgnoredAny,
    _icon: IgnoredAny,
    _summary: IgnoredAny,
    _body: IgnoredAny,
    _actions: IgnoredAny,
    hints: NotifyHints,
    _expiry: IgnoredAny,
}

impl Type for NotifyBody<'_> {
    const SIGNATURE: &'static Signature = <(
        String,
        u32,
        String,
        String,
        String,
        Vec<String>,
        HashMap<String, OwnedValue>,
        i32,
    )>::SIGNATURE;
}

#[derive(Default)]
struct NotifyHints {
    desktop_entry: Option<String>,
    urgency: Option<u8>,
}

impl<'de> Deserialize<'de> for NotifyHints {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct HintsVisitor;
        impl<'de> Visitor<'de> for HintsVisitor {
            type Value = NotifyHints;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("notification hints")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut hints = NotifyHints::default();
                while let Some(key) = map.next_key::<&str>()? {
                    match key {
                        "desktop-entry" => {
                            if let Value::Str(value) = map.next_value::<Value<'de>>()? {
                                if value.len() > MAX_METADATA_INPUT {
                                    return Err(serde::de::Error::custom(
                                        "oversized application metadata",
                                    ));
                                }
                                hints.desktop_entry = application_name(value.as_str());
                            }
                        }
                        "urgency" => {
                            if let Value::U8(value) = map.next_value::<Value<'de>>()? {
                                hints.urgency = (value <= 2).then_some(value);
                            }
                        }
                        _ => {
                            map.next_value::<IgnoredAny>()?;
                        }
                    }
                }
                Ok(hints)
            }
        }
        deserializer.deserialize_map(HintsVisitor)
    }
}

fn notification_metadata(message: &Message) -> Option<NotificationMetadata> {
    let received = Instant::now();
    let body = message.body();
    if body.len() > MAX_NOTIFICATION_BYTES || body.signature() != NotifyBody::SIGNATURE {
        return None;
    }
    let metadata: NotifyBody<'_> = body.deserialize().ok()?;
    // Oversized app metadata is rejected even if a usable desktop-entry was supplied.
    if metadata.application.len() > MAX_METADATA_INPUT {
        return None;
    }
    Some(NotificationMetadata {
        application: metadata
            .hints
            .desktop_entry
            .or_else(|| application_name(metadata.application))?,
        urgency: metadata.hints.urgency.unwrap_or(1),
        received,
    })
}

fn application_name(input: &str) -> Option<String> {
    if input.len() > MAX_METADATA_INPUT {
        return None;
    }
    let input = input.trim();
    // DesktopEntry values already are filename prefixes; preserve their own suffixes.
    let mut output = String::with_capacity(input.len().min(MAX_APPLICATION_BYTES));
    for character in input.chars() {
        if character.is_control()
            || matches!(character,
            '\u{061c}' | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' |
            '\u{2060}'..='\u{206f}' | '\u{feff}')
        {
            continue;
        }
        if output.len() + character.len_utf8() > MAX_APPLICATION_BYTES {
            break;
        }
        output.push(character);
    }
    let trimmed = output.trim();
    if trimmed.is_empty() {
        None
    } else if trimmed.len() == output.len() {
        Some(output)
    } else {
        Some(trimmed.to_owned())
    }
}

async fn session_loop(shared: &Shared) {
    let mut backoff = INITIAL_BACKOFF;
    loop {
        let started = Instant::now();
        if let Ok(Ok(connection)) = timeout(CONNECT_TIMEOUT, session_connection()).await {
            tokio::select! {
                _ = connection.closed() => {},
                _ = lock_loop(&connection, shared) => {},
                _ = media_loop(&connection, shared) => {},
            }
        }
        session_unavailable(shared, "Session bus unavailable; reconnecting");
        if started.elapsed() >= Duration::from_secs(5) {
            backoff = INITIAL_BACKOFF;
        }
        sleep(backoff).await;
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

async fn lock_loop(connection: &Connection, shared: &Shared) {
    loop {
        let result = timeout(RPC_TIMEOUT, async {
            let proxy = Proxy::new(
                connection,
                "org.freedesktop.ScreenSaver",
                "/ScreenSaver",
                "org.freedesktop.ScreenSaver",
            )
            .await?;
            proxy
                .call_with_flags::<_, _, bool>("GetActive", MethodFlags::NoAutoStart.into(), &())
                .await
        })
        .await;
        let locked = match result {
            Ok(Ok(Some(active))) => Some(active),
            _ => None,
        };
        {
            let mut state = shared.lock().unwrap_or_else(|error| error.into_inner());
            state.desktop.locked = locked;
            state.desktop.lock_status = match locked {
                Some(true) => "Session locked; lighting inhibited",
                Some(false) => "Session unlocked",
                None => "Lock service unavailable or timed out; lighting inhibited",
            }
            .into();
            state.lock_checked = Some(Instant::now());
        }
        sleep(LOCK_INTERVAL).await;
    }
}

async fn media_loop(connection: &Connection, shared: &Shared) {
    loop {
        let names = timeout(RPC_TIMEOUT, async {
            fdo::DBusProxy::new(connection).await?.list_names().await
        })
        .await;
        let names = match names {
            Ok(Ok(names)) => names,
            _ => return,
        };
        let mut names: Vec<_> = names
            .into_iter()
            .filter(|name| name.as_str().starts_with(MPRIS_PREFIX))
            .collect();
        names.sort_unstable_by(|left, right| left.as_str().cmp(right.as_str()));
        let limited = names.len() > MAX_PLAYERS;
        names.truncate(MAX_PLAYERS);
        let mut pending = FuturesUnordered::new();
        for name in &names {
            pending.push(timeout(
                PLAYER_TIMEOUT,
                playing_application(connection, name.as_str()),
            ));
        }
        let mut playing = Vec::new();
        let mut unavailable = false;
        while let Some(result) = pending.next().await {
            match result {
                Ok(Ok(Some(application))) => playing.push(application),
                Ok(Ok(None)) => {}
                _ => unavailable = true,
            }
        }
        playing.sort_unstable();
        playing.dedup();
        {
            let mut state = shared.lock().unwrap_or_else(|error| error.into_inner());
            state.desktop.playing = playing;
            state.desktop.media_status = if limited {
                "Monitoring media; limited to 32 player services"
            } else if unavailable {
                "Monitoring media; some players unavailable"
            } else {
                "Monitoring media playback"
            }
            .into();
            state.media_checked = Some(Instant::now());
        }
        sleep(MEDIA_INTERVAL).await;
    }
}

async fn playing_application(connection: &Connection, name: &str) -> zbus::Result<Option<String>> {
    // Bind all reads to one unique owner so a restart cannot mix two applications' properties.
    let owner = fdo::DBusProxy::new(connection)
        .await?
        .get_name_owner(name.try_into()?)
        .await?;
    if string_property(
        connection,
        owner.as_str(),
        MPRIS_PLAYER,
        "PlaybackStatus",
        |status| Some(status == "Playing"),
    )
    .await?
        != Some(true)
    {
        return Ok(None);
    }
    let entry = string_property(
        connection,
        owner.as_str(),
        MPRIS_ROOT,
        "DesktopEntry",
        application_name,
    )
    .await;
    let application = match entry.ok().flatten() {
        Some(entry) => Some(entry),
        None => {
            let identity = string_property(
                connection,
                owner.as_str(),
                MPRIS_ROOT,
                "Identity",
                application_name,
            )
            .await;
            identity
                .ok()
                .flatten()
                .or_else(|| application_name(name.strip_prefix(MPRIS_PREFIX).unwrap_or(name)))
        }
    };
    // A disappearing owner must not turn a failed metadata read into a fallback playing entry.
    if !fdo::DBusProxy::new(connection)
        .await?
        .name_has_owner(owner.as_str().try_into()?)
        .await?
    {
        return Ok(None);
    }
    Ok(application)
}

async fn string_property<T>(
    connection: &Connection,
    owner: &str,
    interface: &str,
    property: &str,
    decode: impl FnOnce(&str) -> Option<T>,
) -> zbus::Result<Option<T>> {
    // Raw Properties.Get deliberately avoids proxy caching/GetAll, which would fetch song metadata.
    let reply = connection
        .call_method(
            Some(owner),
            MPRIS_PATH,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(interface, property),
        )
        .await?;
    let body = reply.body();
    if body.len() > MAX_METADATA_INPUT + 32 {
        return Ok(None);
    }
    match body.deserialize::<Value<'_>>()? {
        Value::Str(value) if value.len() <= MAX_METADATA_INPUT => Ok(decode(value.as_str())),
        _ => Ok(None),
    }
}
