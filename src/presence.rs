//! Discord Rich Presence.
//!
//! Подключение к локальному каналу Discord (`\\.\pipe\discord-ipc-N`) и
//! отправку `SET_ACTIVITY` делает крейт `discord-rich-presence`. Если Discord
//! не запущен или перезапустился, поток тихо ждёт и подключается заново.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use discord_rich_presence::activity::{Activity, ActivityType, Assets, Button, Timestamps};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};
use serde::Deserialize;

/// Application ID из Discord Developer Portal (имя приложения = emocloud).
const CLIENT_ID: &str = "1570051660976365588";

/// Как часто пробуем подключиться, пока Discord закрыт.
const RETRY_DELAY: Duration = Duration::from_secs(5);
/// Как часто повторяем статус, даже если трек не менялся.
const MIN_RESEND: Duration = Duration::from_secs(15);
/// Насколько позиция должна разойтись с ожидаемой, чтобы счесть это перемоткой.
const SEEK_TOLERANCE: f64 = 5.0;

/// Данные о треке, которые присылает скрипт со страницы SoundCloud.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackInfo {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub artwork: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub playing: bool,
    #[serde(default)]
    pub position: Option<f64>,
    #[serde(default)]
    pub duration: Option<f64>,
}

enum Event {
    /// Пришло новое состояние плеера (None — ничего не играет).
    Track(Option<TrackInfo>),
}

static EVENTS: OnceLock<Sender<Event>> = OnceLock::new();
static START: OnceLock<Instant> = OnceLock::new();

/// Запускает фоновый поток Discord-статуса. Вызывать один раз при старте.
pub fn start() {
    if EVENTS.get().is_some() {
        return;
    }

    let _ = std::fs::remove_file(log_path());

    let (tx, rx) = mpsc::channel();
    if EVENTS.set(tx).is_err() {
        return;
    }

    std::thread::spawn(move || run(rx));
}

/// Отправляет в поток статуса текущий трек (None — очистить статус).
pub fn push(track: Option<TrackInfo>) {
    if let Some(tx) = EVENTS.get() {
        let _ = tx.send(Event::Track(track));
    }
}

fn run(events: Receiver<Event>) {
    let mut last_track: Option<TrackInfo> = None;
    let mut offline_logged = false;

    loop {
        let mut client = DiscordIpcClient::new(CLIENT_ID);
        if let Err(err) = client.connect() {
            if !offline_logged {
                log_line(&format!("Discord не запущен ({err}), ждём"));
                offline_logged = true;
            }
            wait_retry(&events, &mut last_track);
            continue;
        }

        offline_logged = false;
        log_line("подключились к Discord");

        let mut state = PresenceState::default();
        if let Err(err) = state.apply(&mut client, last_track.as_ref()) {
            log_line(&format!("SET_ACTIVITY не отправлен: {err}"));
            let _ = client.close();
            wait_retry(&events, &mut last_track);
            continue;
        }

        loop {
            let event = match events.recv_timeout(Duration::from_secs(3600)) {
                Ok(event) => event,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return,
            };

            let result = match event {
                Event::Track(track) => {
                    last_track = track.clone();
                    state.apply(&mut client, track.as_ref())
                }
            };

            if let Err(err) = result {
                log_line(&format!("Discord недоступен: {err}"));
                break;
            }
        }

        let _ = client.close();
        log_line("соединение с Discord закрыто");
        wait_retry(&events, &mut last_track);
    }
}

/// Пока Discord недоступен — ждём и запоминаем последний известный трек.
fn wait_retry(events: &Receiver<Event>, last_track: &mut Option<TrackInfo>) {
    let deadline = Instant::now() + RETRY_DELAY;

    while Instant::now() < deadline {
        let left = deadline.saturating_duration_since(Instant::now());
        match events.recv_timeout(left) {
            Ok(Event::Track(track)) => *last_track = track,
            Err(RecvTimeoutError::Timeout) => break,
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

struct PresenceState {
    /// Название+исполнитель+пауза: если не менялось — трек тот же.
    key: Option<String>,
    /// Начало трека в unix-миллисекундах (только пока играет).
    start_ms: Option<u64>,
    /// Позиция и момент, от которых считаем прогресс.
    anchor_pos: f64,
    anchor_at: Option<Instant>,
    sent_at: Option<Instant>,
    /// Статус уже очищен — не шлём null повторно.
    cleared: bool,
}

impl Default for PresenceState {
    fn default() -> Self {
        Self {
            key: None,
            start_ms: None,
            anchor_pos: 0.0,
            anchor_at: None,
            sent_at: None,
            cleared: false,
        }
    }
}

impl PresenceState {
    fn apply(
        &mut self,
        client: &mut DiscordIpcClient,
        track: Option<&TrackInfo>,
    ) -> Result<(), discord_rich_presence::error::Error> {
        let track = track.filter(|track| track.title.trim().chars().count() >= 2);

        let Some(track) = track else {
            self.key = None;
            self.start_ms = None;
            self.sent_at = None;
            if self.cleared {
                return Ok(());
            }
            self.cleared = true;
            return client.clear_activity();
        };

        self.cleared = false;

        let key = format!("{}|{}|{}", track.title, track.artist, track.playing);
        let now = Instant::now();
        let key_changed = self.key.as_deref() != Some(key.as_str());
        let mut rebuild = key_changed;

        if key_changed {
            let state = if track.playing { "играет" } else { "пауза" };
            log_line(&format!(
                "трек: {} — {} ({state})",
                track.title, track.artist
            ));
        }

        // Перемотка: позиция ушла от той, что мы предсказывали.
        if !rebuild && track.playing {
            if let (Some(position), Some(anchor_at)) = (track.position, self.anchor_at) {
                let expected = self.anchor_pos + anchor_at.elapsed().as_secs_f64();
                if (position - expected).abs() > SEEK_TOLERANCE {
                    rebuild = true;
                }
            }
        }

        let due = self
            .sent_at
            .map_or(true, |sent_at| sent_at.elapsed() >= MIN_RESEND);
        if !rebuild && !due {
            return Ok(());
        }

        if rebuild || self.start_ms.is_none() {
            self.start_ms = match (track.playing, track.position) {
                (true, Some(position)) => {
                    let played_ms = (position.max(0.0) * 1000.0) as u64;
                    Some(now_ms().saturating_sub(played_ms))
                }
                _ => None,
            };
            self.anchor_pos = track.position.unwrap_or(0.0);
            self.anchor_at = Some(now);
        }

        self.key = Some(key);
        self.sent_at = Some(now);
        client.set_activity(activity(track, self.start_ms))
    }
}

fn activity(track: &TrackInfo, start_ms: Option<u64>) -> Activity<'static> {
    let title = clip(&track.title, 128);
    let artist = clip(&track.artist, 128);

    // 2 = «Слушает»; имя приложения Discord берёт из Application ID.
    let mut activity = Activity::new()
        .activity_type(ActivityType::Listening)
        .details(title);

    if !artist.is_empty() {
        activity = activity.state(format!("by {artist}"));
    }

    if let Some(start) = start_ms {
        let mut timestamps = Timestamps::new().start(start as i64);
        if let Some(duration) = track.duration {
            if duration > 0.0 {
                timestamps = timestamps.end(start as i64 + (duration * 1000.0) as i64);
            }
        }
        activity = activity.timestamps(timestamps);
    }

    if let Some(artwork) = track
        .artwork
        .as_deref()
        .filter(|url| url.starts_with("https://"))
    {
        activity = activity.assets(
            Assets::new()
                .large_image(artwork.to_string())
        );
    }

    if let Some(url) = track
        .url
        .as_deref()
        .filter(|url| url.starts_with("https://"))
    {
        activity = activity.buttons(vec![Button::new(
            "Слушать в SoundCloud",
            url.to_string(),
        )]);
    }

    activity
}

fn clip(text: &str, max_chars: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        text.chars().take(max_chars).collect()
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

fn log_path() -> std::path::PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("com.artur.soundcloudrust")
        .join("presence.log")
}

/// Небольшой лог для разбора проблем: пишем только важные события.
fn log_line(message: &str) {
    use std::io::Write;

    let path = log_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }

    let seconds = START.get_or_init(Instant::now).elapsed().as_secs_f32();
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(file, "[{seconds:7.2}] {message}");
    }
}