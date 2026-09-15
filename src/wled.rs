//! Bounded, latest-frame DDP output. UDP submission is not proof of delivery.
//! Only transient realtime control is released; permanent WLED state is never written.
use crate::config::{DeviceConfig, MAX_LEDS};
use anyhow::{Context, Result, anyhow, bail, ensure};
use reqwest::blocking::{Client, Response};
use serde::Deserialize;
use std::{
    io::Read,
    net::{Ipv4Addr, SocketAddrV4, UdpSocket},
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const MAX_JSON_BYTES: usize = 64 * 1024;
const HTTP_TIMEOUT: Duration = Duration::from_millis(250);
const FRESHNESS: Duration = Duration::from_millis(500);
const FRAME_INTERVAL: Duration = Duration::from_nanos(33_333_334);
const HEALTH_INTERVAL: Duration = Duration::from_secs(1);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(1);
const PAYLOAD_LEDS: usize = 480;

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub led_count: usize,
    pub version: String,
    pub live: bool,
}

#[derive(Clone, Debug)]
pub struct OutputState {
    /// Observed local/HTTP status, never an assertion of UDP delivery or settings restoration.
    pub status: String,
    /// Complete frames accepted by the local UDP socket, not device acknowledgments.
    pub sent_frames: u64,
    /// Local sending activity, not confirmation that WLED displayed the pixels.
    pub active: bool,
    pub error: Option<String>,
    /// Changes on every transport failure, even if its error text is unchanged.
    pub failure_epoch: u64,
}

#[derive(Clone, Copy)]
struct Ports {
    http: u16,
    udp: u16,
    loopback_only: bool,
}

impl Ports {
    const WLED: Self = Self {
        http: 80,
        udp: 4048,
        loopback_only: false,
    };

    fn validate(self, device: DeviceConfig) -> Result<()> {
        device.validate()?;
        ensure!(
            !self.loopback_only || device.address.is_loopback(),
            "Synthetic endpoints require loopback IPv4"
        );
        Ok(())
    }

    fn http_url(self, device: DeviceConfig, path: &str) -> String {
        format!("http://{}:{}{path}", device.address, self.http)
    }
}

fn http_client() -> Result<Client> {
    Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .connect_timeout(Duration::from_millis(150))
        .timeout(HTTP_TIMEOUT)
        .pool_max_idle_per_host(0)
        .build()
        .context("Creating bounded WLED HTTP client")
}

fn bounded_body(response: Response, operation: &str) -> Result<Vec<u8>> {
    let status = response.status();
    ensure!(
        status.is_success(),
        "WLED {operation} returned HTTP {status}"
    );
    ensure!(
        response
            .content_length()
            .is_none_or(|length| length <= MAX_JSON_BYTES as u64),
        "WLED {operation} exceeded the 64 KiB response limit"
    );
    let mut bytes = Vec::with_capacity(1024);
    response
        .take((MAX_JSON_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .with_context(|| format!("Reading WLED {operation} response within deadline"))?;
    ensure!(
        bytes.len() <= MAX_JSON_BYTES,
        "WLED {operation} exceeded the 64 KiB response limit"
    );
    Ok(bytes)
}

#[derive(Deserialize)]
struct InfoJson {
    ver: String,
    leds: LedInfoJson,
    live: bool,
}

#[derive(Deserialize)]
struct LedInfoJson {
    count: usize,
}

fn probe_with(client: &Client, device: DeviceConfig, ports: Ports) -> Result<DeviceInfo> {
    ports.validate(device)?;
    let response = client
        .get(ports.http_url(device, "/json/info"))
        .send()
        .context("WLED info HTTP request failed")?;
    let bytes = bounded_body(response, "info")?;
    // Do not include serde's error text: it can quote untrusted response contents.
    let info: InfoJson = serde_json::from_slice(&bytes).map_err(|error| {
        anyhow!(
            "Invalid WLED info JSON/schema at line {}, column {}",
            error.line(),
            error.column()
        )
    })?;
    ensure!(
        (1..=MAX_LEDS).contains(&info.leds.count),
        "WLED reported an unsupported LED count (expected 1–{MAX_LEDS})"
    );
    ensure!(
        !info.ver.trim().is_empty()
            && info.ver.len() <= 128
            && info
                .ver
                .bytes()
                .all(|byte| byte.is_ascii_graphic() || byte == b' '),
        "WLED reported an invalid version string"
    );
    Ok(DeviceInfo {
        led_count: info.leds.count,
        version: info.ver,
        live: info.live,
    })
}

/// Read-only discovery on the validated local IPv4 device's HTTP port 80.
/// Call off the GUI thread: the complete request has a 250 ms deadline.
pub fn probe(device: DeviceConfig) -> Result<DeviceInfo> {
    device.validate()?;
    probe_with(&http_client()?, device, Ports::WLED)
}

fn release_with(client: &Client, device: DeviceConfig, ports: Ports) -> Result<()> {
    ports.validate(device)?;
    let response = client
        .post(ports.http_url(device, "/json/state"))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(r#"{"live":false}"#)
        .send()
        .context("WLED realtime-release HTTP request failed")?;
    let bytes = bounded_body(response, "realtime release")?;
    #[derive(Deserialize)]
    struct Acknowledgment {
        success: bool,
    }
    let reply: Acknowledgment = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow!("WLED realtime release returned an invalid acknowledgment"))?;
    ensure!(
        reply.success,
        "WLED did not acknowledge realtime release (success=false)"
    );
    Ok(())
}

/// Visit one complete RGB24 DDP frame, without allocating packet buffers.
/// Sequence must be 1..=15; advance once per frame, wrapping 15 to 1.
/// Datagrams have at most 1440 RGB bytes plus a 10-byte header. Only the last
/// datagram carries PUSH. The callback must consume the borrowed packet before returning.
/// This helper does not send, retry, rate-limit, or authorize hardware control.
pub fn visit_ddp_packets(
    pixels: &[[u8; 3]],
    sequence: u8,
    mut visit: impl FnMut(&[u8]) -> Result<()>,
) -> Result<()> {
    ensure!(
        (1..=MAX_LEDS).contains(&pixels.len()),
        "DDP frames require 1–{MAX_LEDS} RGB pixels"
    );
    ensure!((1..=15).contains(&sequence), "DDP sequence must be 1–15");
    let mut packet = [0_u8; 10 + PAYLOAD_LEDS * 3];
    packet[1] = sequence;
    packet[2] = 0x0b; // RGB24
    packet[3] = 1; // Display destination
    for (index, chunk) in pixels.chunks(PAYLOAD_LEDS).enumerate() {
        let offset = index * PAYLOAD_LEDS * 3;
        let length = chunk.len() * 3;
        packet[0] = 0x40 | u8::from(offset + length == pixels.len() * 3);
        packet[4..8].copy_from_slice(&(offset as u32).to_be_bytes());
        packet[8..10].copy_from_slice(&(length as u16).to_be_bytes());
        packet[10..10 + length].copy_from_slice(chunk.as_flattened());
        visit(&packet[..10 + length])?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Frame {
    device: DeviceConfig,
    active: bool,
    valid_pixels: bool,
    expires_at: Instant,
    revision: u64,
    epoch: u64,
}

struct Slot {
    frame: Option<Frame>,
    pixels: Vec<[u8; 3]>,
}

struct Shared {
    slot: Mutex<Slot>,
    wake: Condvar,
    state: Mutex<OutputState>,
    shutdown: AtomicBool,
    epoch: AtomicU64,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// One worker, one reusable latest-frame slot, and no queued frame backlog.
/// The worker never holds the slot or status mutex during network operations.
pub struct WledOutput {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    finished: mpsc::Receiver<()>,
}

impl WledOutput {
    pub fn start() -> Result<Self> {
        Self::start_with(Ports::WLED)
    }

    fn start_with(ports: Ports) -> Result<Self> {
        let shared = Arc::new(Shared {
            slot: Mutex::new(Slot {
                frame: None,
                pixels: Vec::with_capacity(MAX_LEDS),
            }),
            wake: Condvar::new(),
            state: Mutex::new(OutputState {
                status: "Lighting disabled".into(),
                sent_frames: 0,
                active: false,
                error: None,
                failure_epoch: 0,
            }),
            shutdown: AtomicBool::new(false),
            epoch: AtomicU64::new(0),
        });
        let (finished_tx, finished) = mpsc::sync_channel(1);
        let (ready_tx, ready) = mpsc::sync_channel(1);
        let worker_shared = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name("ledalert-wled".into())
            .spawn(move || {
                match Worker::new(worker_shared, ports) {
                    Ok(mut worker) => {
                        let _ = ready_tx.send(Ok(()));
                        worker.run();
                    }
                    Err(error) => {
                        let _ = ready_tx.send(Err(format!("{error:#}")));
                    }
                }
                let _ = finished_tx.send(());
            })
            .context("Starting WLED output worker")?;
        let output = Self {
            shared,
            thread: Some(thread),
            finished,
        };
        match ready.recv_timeout(SHUTDOWN_TIMEOUT) {
            Ok(Ok(())) => Ok(output),
            Ok(Err(error)) => Err(anyhow!(error)),
            Err(_) => Err(anyhow!(
                "WLED worker initialization did not complete within one second"
            )),
        }
    }

    /// CPU-only bounded copy; network operations are exclusively on the worker.
    /// Disable/target changes or shorter authority invalidate in-flight packets.
    /// `valid_until` bounds authority independently of producer/UI progress;
    /// every frame also expires within the normal producer freshness limit.
    pub fn submit(
        &self,
        device: DeviceConfig,
        pixels: &[[u8; 3]],
        active: bool,
        valid_until: Option<Instant>,
    ) {
        let mut slot = lock(&self.shared.slot);
        let valid_pixels =
            !active || (pixels.len() == device.led_count && (1..=MAX_LEDS).contains(&pixels.len()));
        let changed = slot.frame.is_none_or(|old| {
            old.device != device
                || old.active != active
                || old.valid_pixels != valid_pixels
                || valid_until.is_some_and(|end| end < old.expires_at)
        });
        let epoch = if changed {
            self.shared
                .epoch
                .fetch_add(1, Ordering::AcqRel)
                .wrapping_add(1)
        } else {
            self.shared.epoch.load(Ordering::Acquire)
        };
        let revision = slot.frame.map_or(1, |old| old.revision.wrapping_add(1));
        let freshness_end = Instant::now() + FRESHNESS;
        slot.pixels.clear();
        if active && valid_pixels {
            slot.pixels.extend_from_slice(pixels);
        }
        slot.frame = Some(Frame {
            device,
            active,
            valid_pixels,
            expires_at: valid_until.map_or(freshness_end, |end| end.min(freshness_end)),
            revision,
            epoch,
        });
        drop(slot);
        self.shared.wake.notify_one();
    }

    pub fn snapshot(&self) -> OutputState {
        lock(&self.shared.state).clone()
    }
}

impl Drop for WledOutput {
    fn drop(&mut self) {
        self.shared.shutdown.store(true, Ordering::Release);
        self.shared.wake.notify_one();
        if let Some(thread) = self.thread.take() {
            match self.finished.recv_timeout(SHUTDOWN_TIMEOUT) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if thread.join().is_err() {
                        eprintln!(
                            "WLED worker failed during shutdown; realtime release is unconfirmed"
                        );
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Detach rather than freezing application shutdown. The cancellation flag
                    // forbids further DDP sends, even if the bounded HTTP call is still exiting.
                    eprintln!(
                        "WLED shutdown deadline exceeded; realtime release is unconfirmed; recovery depends on WLED's device-configured realtime timeout"
                    );
                }
            }
        }
        let state = lock(&self.shared.state);
        if let Some(error) = &state.error {
            eprintln!("WLED output stopped: {error}");
        }
    }
}

struct Worker {
    shared: Arc<Shared>,
    ports: Ports,
    client: Client,
    socket: UdpSocket,
    frame: Option<Frame>,
    pixels: Vec<[u8; 3]>,
    ready: Option<(DeviceConfig, u64)>,
    acquired: Option<DeviceConfig>,
    uncertain: Option<DeviceConfig>,
    blocked_through: u64,
    retry_after: Instant,
    last_send: Option<Instant>,
    last_health: Instant,
    sequence: u8,
    last_sent_revision: u64,
}

impl Worker {
    fn new(shared: Arc<Shared>, ports: Ports) -> Result<Self> {
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))
            .context("Binding WLED UDP socket")?;
        socket
            .set_nonblocking(true)
            .context("Making WLED UDP socket nonblocking")?;
        Ok(Self {
            shared,
            ports,
            socket,
            client: http_client()?,
            frame: None,
            pixels: Vec::with_capacity(MAX_LEDS),
            ready: None,
            acquired: None,
            uncertain: None,
            blocked_through: 0,
            retry_after: Instant::now(),
            last_send: None,
            last_health: Instant::now(),
            sequence: 1,
            last_sent_revision: 0,
        })
    }

    fn refresh(&mut self) {
        let mut slot = lock(&self.shared.slot);
        if slot.frame.map(|frame| frame.revision) != self.frame.map(|frame| frame.revision) {
            self.frame = slot.frame;
            std::mem::swap(&mut self.pixels, &mut slot.pixels);
        }
    }

    fn wait(&self, timeout: Duration) {
        let slot = lock(&self.shared.slot);
        if !self.shared.shutdown.load(Ordering::Acquire)
            && slot.frame.map(|frame| frame.revision) == self.frame.map(|frame| frame.revision)
        {
            drop(self.shared.wake.wait_timeout(slot, timeout));
        }
    }

    fn stop(&mut self, reason: &str, error: Option<String>) {
        self.ready = None;
        {
            let mut state = lock(&self.shared.state);
            state.active = false;
            if error.is_some() {
                state.failure_epoch = state.failure_epoch.wrapping_add(1);
            }
            if error.is_some() || state.error.is_none() {
                if state.status != reason {
                    state.status = reason.into();
                }
                state.error = error;
            }
        }
        // A successful local send, even of only part of a frame, is the sole
        // acquisition condition. Never release a probed-but-untouched device.
        if let Some(device) = self.acquired.take() {
            let result = release_with(&self.client, device, self.ports);
            let mut state = lock(&self.shared.state);
            match result {
                Ok(()) => {
                    state.status = format!(
                        "{reason}; WLED acknowledged realtime release (settings not verified)"
                    );
                }
                Err(error) => {
                    state.failure_epoch = state.failure_epoch.wrapping_add(1);
                    self.uncertain = Some(device);
                    let release_error = format!(
                        "Realtime release uncertain: {error:#}; recovery depends on WLED's device-configured realtime timeout"
                    );
                    state.error = Some(match state.error.take() {
                        Some(previous) => format!("{previous}; {release_error}"),
                        None => release_error,
                    });
                    state.status = format!("{reason}; realtime release uncertain");
                }
            }
        }
    }

    fn fail(&mut self, error: anyhow::Error) {
        self.stop("Output stopped", Some(format!("{error:#}")));
        // A failed operation never replays the frame that preceded it, nor a
        // submission made while its failure/release was being handled.
        self.blocked_through = lock(&self.shared.slot)
            .frame
            .map_or(0, |frame| frame.revision);
        self.retry_after = Instant::now() + HEALTH_INTERVAL;
    }

    fn run(&mut self) {
        loop {
            self.refresh();
            if self.shared.shutdown.load(Ordering::Acquire) {
                self.stop("Shutdown", None);
                return;
            }
            let Some(frame) = self.frame else {
                self.wait(FRESHNESS);
                continue;
            };
            if !frame.active || Instant::now() >= frame.expires_at {
                let reason = if frame.active {
                    "Frame permission or freshness expired; packets stopped"
                } else {
                    "Lighting disabled"
                };
                self.stop(reason, None);
                self.wait(FRESHNESS);
                continue;
            }
            if self
                .ready
                .is_some_and(|ready| ready != (frame.device, frame.epoch))
            {
                self.stop("Output permission or target changed", None);
                continue;
            }
            if frame.revision <= self.blocked_through || Instant::now() < self.retry_after {
                self.wait(Duration::from_millis(50));
                continue;
            }
            if !frame.valid_pixels {
                self.fail(anyhow!(
                    "A complete RGB frame matching the configured LED count is required"
                ));
                continue;
            }
            if let Err(error) = self.ports.validate(frame.device) {
                self.fail(error);
                continue;
            }
            if let Some(previous) = self.uncertain {
                // Do not retry a release that could now interrupt a new owner.
                // Read-only evidence of live=false is required before moving on.
                match probe_with(&self.client, previous, self.ports) {
                    Ok(info) if !info.live => {
                        self.uncertain = None;
                    }
                    Ok(_) => self.fail(anyhow!(
                        "Previous realtime release remains uncertain: WLED HTTP reports live=true"
                    )),
                    Err(error) => {
                        self.fail(error.context("Previous realtime release remains uncertain"))
                    }
                }
                continue;
            }
            if self.ready.is_none() {
                {
                    let mut state = lock(&self.shared.state);
                    state.status = "Checking WLED before realtime control".into();
                    state.error = None;
                }
                let checked = probe_with(&self.client, frame.device, self.ports).and_then(|info| {
                    ensure!(info.led_count == frame.device.led_count, "WLED LED count {} differs from configured {}; no pixels sent", info.led_count, frame.device.led_count);
                    ensure!(!info.live, "WLED HTTP reports live=true before acquisition; refusing another controller's realtime session");
                    self.socket.connect(SocketAddrV4::new(frame.device.address, self.ports.udp))
                        .context("Connecting local WLED UDP socket")?;
                    Ok(())
                });
                match checked {
                    Ok(()) => {
                        self.ready = Some((frame.device, frame.epoch));
                        self.last_health = Instant::now();
                    }
                    Err(error) => self.fail(error),
                }
                // Probe may have consumed the frame's lifetime or raced disable.
                continue;
            }
            if self.acquired.is_some() && self.last_health.elapsed() >= HEALTH_INTERVAL {
                let result = probe_with(&self.client, frame.device, self.ports);
                match result {
                    Ok(info) if info.led_count == frame.device.led_count && info.live => {
                        self.last_health = Instant::now();
                        lock(&self.shared.state).status =
                            "WLED HTTP reports live=true; UDP delivery unconfirmed".into();
                    }
                    Ok(info) => self.fail(anyhow!(
                        "WLED health changed: HTTP reports live={}, LED count={} (configured {})",
                        info.live,
                        info.led_count,
                        frame.device.led_count
                    )),
                    Err(error) => self.fail(error.context("WLED health check failed")),
                }
                continue;
            }
            if frame.revision == self.last_sent_revision {
                self.wait(
                    frame
                        .expires_at
                        .saturating_duration_since(Instant::now())
                        .min(Duration::from_millis(50)),
                );
                continue;
            }
            if let Some(last_send) = self.last_send {
                let remaining = FRAME_INTERVAL.saturating_sub(last_send.elapsed());
                if !remaining.is_zero() {
                    self.wait(remaining);
                    continue;
                }
            }
            let mut interrupted = false;
            let mut sent_any = false;
            let result = visit_ddp_packets(&self.pixels, self.sequence, |packet| {
                if self.shared.shutdown.load(Ordering::Acquire)
                    || self.shared.epoch.load(Ordering::Acquire) != frame.epoch
                    || Instant::now() >= frame.expires_at
                {
                    interrupted = true;
                    bail!("Frame superseded by stop/target change/expiry");
                }
                let sent = self
                    .socket
                    .send(packet)
                    .context("WLED UDP send failed (delivery unconfirmed)")?;
                if sent > 0 {
                    sent_any = true;
                }
                ensure!(
                    sent == packet.len(),
                    "WLED UDP socket accepted a partial datagram; delivery unconfirmed"
                );
                Ok(())
            });
            if sent_any {
                self.acquired = Some(frame.device);
                self.last_send = Some(Instant::now());
            }
            if interrupted {
                continue;
            }
            match result {
                Ok(()) => {
                    self.last_sent_revision = frame.revision;
                    self.sequence = if self.sequence == 15 {
                        1
                    } else {
                        self.sequence + 1
                    };
                    let mut state = lock(&self.shared.state);
                    if !state.active {
                        state.status =
                            "DDP frames submitted locally; UDP delivery unconfirmed".into();
                    }
                    state.active = true;
                    state.sent_frames = state.sent_frames.saturating_add(1);
                    state.error = None;
                }
                Err(error) => self.fail(error),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{ErrorKind, Write},
        net::{TcpListener, TcpStream},
    };

    #[derive(Clone)]
    struct Reply {
        status: &'static str,
        body: Vec<u8>,
        declared_length: Option<usize>,
    }

    impl Reply {
        fn json(body: impl Into<Vec<u8>>) -> Self {
            let body = body.into();
            Self {
                status: "200 OK",
                declared_length: Some(body.len()),
                body,
            }
        }
    }

    #[derive(Debug)]
    struct Request {
        method: String,
        path: String,
        body: Vec<u8>,
    }

    struct HttpState {
        requests: Mutex<Vec<Request>>,
        info_override: Mutex<Option<Reply>>,
        release_override: Mutex<Option<Reply>>,
        live: AtomicBool,
        hold_info: AtomicBool,
        stop: AtomicBool,
    }

    struct Fixture {
        ports: Ports,
        state: Arc<HttpState>,
        udp: UdpSocket,
        thread: Option<JoinHandle<()>>,
    }

    fn read_request(stream: &mut TcpStream) -> std::io::Result<Request> {
        stream.set_read_timeout(Some(Duration::from_millis(100)))?;
        stream.set_write_timeout(Some(Duration::from_millis(100)))?;
        let mut bytes = Vec::new();
        let mut buffer = [0; 1024];
        let mut boundary = None;
        let deadline = Instant::now() + Duration::from_millis(200);
        while bytes.len() < 8192 && Instant::now() < deadline {
            let count = stream.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..count]);
            if let Some(index) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                boundary = Some(index + 4);
                break;
            }
        }
        let boundary =
            boundary.ok_or_else(|| std::io::Error::other("Incomplete fixture request"))?;
        let headers = String::from_utf8_lossy(&bytes[..boundary]).into_owned();
        let mut first = headers
            .lines()
            .next()
            .unwrap_or_default()
            .split_whitespace();
        let method = first.next().unwrap_or_default().to_owned();
        let path = first.next().unwrap_or_default().to_owned();
        let length = headers
            .lines()
            .filter_map(|line| line.split_once(':'))
            .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .and_then(|(_, value)| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        if length > 256 {
            return Err(std::io::Error::other("Fixture body limit"));
        }
        while bytes.len() < boundary + length && Instant::now() < deadline {
            let count = stream.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..count]);
        }
        if bytes.len() < boundary + length {
            return Err(std::io::Error::other("Incomplete fixture body"));
        }
        Ok(Request {
            method,
            path,
            body: bytes[boundary..boundary + length].to_vec(),
        })
    }

    impl Fixture {
        fn new(led_count: usize) -> Self {
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            listener.set_nonblocking(true).unwrap();
            let udp = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            udp.set_read_timeout(Some(Duration::from_millis(800)))
                .unwrap();
            let ports = Ports {
                http: listener.local_addr().unwrap().port(),
                udp: udp.local_addr().unwrap().port(),
                loopback_only: true,
            };
            let state = Arc::new(HttpState {
                requests: Mutex::new(Vec::new()),
                info_override: Mutex::new(None),
                release_override: Mutex::new(None),
                live: AtomicBool::new(false),
                hold_info: AtomicBool::new(false),
                stop: AtomicBool::new(false),
            });
            let worker_state = Arc::clone(&state);
            let thread = thread::spawn(move || {
                while !worker_state.stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let Ok(request) = read_request(&mut stream) else {
                                continue;
                            };
                            let is_info = request.method == "GET" && request.path == "/json/info";
                            let is_release =
                                request.method == "POST" && request.path == "/json/state";
                            {
                                let mut requests = lock(&worker_state.requests);
                                assert!(
                                    requests.len() < 128,
                                    "unexpected unbounded HTTP request loop"
                                );
                                requests.push(request);
                            }
                            if is_info {
                                let deadline = Instant::now() + Duration::from_millis(400);
                                while worker_state.hold_info.load(Ordering::Acquire)
                                    && !worker_state.stop.load(Ordering::Acquire)
                                    && Instant::now() < deadline
                                {
                                    thread::sleep(Duration::from_millis(2));
                                }
                            }
                            let reply = if is_info {
                                lock(&worker_state.info_override).clone().unwrap_or_else(|| {
                                    Reply::json(format!(r#"{{"ver":"0.15.0","leds":{{"count":{led_count}}},"live":{}}}"#, worker_state.live.load(Ordering::Acquire)).into_bytes())
                                })
                            } else if is_release {
                                let reply = lock(&worker_state.release_override).clone();
                                if reply.is_none() {
                                    worker_state.live.store(false, Ordering::Release);
                                }
                                reply
                                    .unwrap_or_else(|| Reply::json(br#"{"success":true}"#.to_vec()))
                            } else {
                                Reply {
                                    status: "404 Not Found",
                                    body: Vec::new(),
                                    declared_length: None,
                                }
                            };
                            let length = reply.declared_length.map_or_else(String::new, |length| {
                                format!("Content-Length: {length}\r\n")
                            });
                            let header = format!(
                                "HTTP/1.1 {}\r\nContent-Type: application/json\r\n{length}Connection: close\r\nLocation: http://127.0.0.1:{}/redirected\r\n\r\n",
                                reply.status, ports.http
                            );
                            let _ = stream
                                .write_all(header.as_bytes())
                                .and_then(|()| stream.write_all(&reply.body));
                        }
                        Err(error) if error.kind() == ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2))
                        }
                        Err(error) => panic!("loopback fixture accept failed: {error}"),
                    }
                }
            });
            Self {
                ports,
                state,
                udp,
                thread: Some(thread),
            }
        }

        fn device(&self, led_count: usize) -> DeviceConfig {
            DeviceConfig {
                address: Ipv4Addr::LOCALHOST,
                led_count,
            }
        }

        fn output(&self) -> WledOutput {
            WledOutput::start_with(self.ports).unwrap()
        }

        fn requests(&self, method: &str) -> usize {
            lock(&self.state.requests)
                .iter()
                .filter(|request| request.method == method)
                .count()
        }

        fn packet(&self) -> Vec<u8> {
            let mut packet = [0; 1451];
            let count = self
                .udp
                .recv(&mut packet)
                .expect("expected bounded loopback DDP datagram");
            self.state.live.store(true, Ordering::Release);
            packet[..count].to_vec()
        }

        fn no_packets(&self) {
            self.udp.set_nonblocking(true).unwrap();
            let mut packet = [0; 1451];
            let result = self.udp.recv(&mut packet);
            self.udp.set_nonblocking(false).unwrap();
            assert_eq!(result.unwrap_err().kind(), ErrorKind::WouldBlock);
        }

        fn assert_release(&self, expected: usize) {
            let requests = lock(&self.state.requests);
            let releases: Vec<_> = requests
                .iter()
                .filter(|request| request.method == "POST")
                .collect();
            assert_eq!(releases.len(), expected);
            for request in releases {
                assert_eq!(request.path, "/json/state");
                assert_eq!(request.body, br#"{"live":false}"#);
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            self.state.stop.store(true, Ordering::Release);
            self.state.hold_info.store(false, Ordering::Release);
            self.thread.take().unwrap().join().unwrap();
        }
    }

    fn until(mut predicate: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !predicate() {
            assert!(
                Instant::now() < deadline,
                "bounded fixture observation timed out"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn discovery_bounds_and_validates_untrusted_http_without_echoing_content() {
        let fixture = Fixture::new(481);
        let client = http_client().unwrap();
        let info = probe_with(&client, fixture.device(1), fixture.ports).unwrap();
        assert_eq!(
            info.led_count, 481,
            "discovery reports actual count, not configured count"
        );
        assert_eq!(info.version, "0.15.0");
        assert!(!info.live);
        for reply in [
            Reply::json(br#"{"ver":"0.15.0","leds":{"count":0},"live":false}"#.to_vec()),
            Reply::json(br#"{"ver":"0.15.0","leds":{"count":1}}"#.to_vec()),
            Reply::json(br#"{"ver":"private-response-marker","leds":{"count":"private-response-marker"},"live":false}"#.to_vec()),
            Reply::json(b"not-json private-response-marker".to_vec()),
            Reply::json(vec![b' '; MAX_JSON_BYTES + 1]),
            Reply { status: "200 OK", body: Vec::new(), declared_length: Some(MAX_JSON_BYTES + 1) },
            Reply { status: "200 OK", body: vec![b' '; MAX_JSON_BYTES + 1], declared_length: None },
        ] {
            *lock(&fixture.state.info_override) = Some(reply);
            let error = probe_with(&client, fixture.device(1), fixture.ports).unwrap_err();
            assert!(!format!("{error:#}").contains("private-response-marker"));
        }
        for status in ["503 Service Unavailable", "302 Found"] {
            let before = fixture.requests("GET");
            *lock(&fixture.state.info_override) = Some(Reply {
                status,
                body: Vec::new(),
                declared_length: None,
            });
            let error = probe_with(&client, fixture.device(1), fixture.ports).unwrap_err();
            assert!(error.to_string().contains(status));
            assert_eq!(
                fixture.requests("GET"),
                before + 1,
                "HTTP redirects must not be followed"
            );
        }
        fixture.assert_release(0);
    }

    #[test]
    fn discovery_deadline_and_local_address_guard_are_enforced() {
        let fixture = Fixture::new(1);
        let client = http_client().unwrap();
        let public = DeviceConfig {
            address: Ipv4Addr::new(8, 8, 8, 8),
            led_count: 1,
        };
        assert!(probe_with(&client, public, fixture.ports).is_err());
        assert_eq!(fixture.requests("GET"), 0);
        fixture.state.hold_info.store(true, Ordering::Release);
        let started = Instant::now();
        assert!(probe_with(&client, fixture.device(1), fixture.ports).is_err());
        assert!(started.elapsed() < Duration::from_millis(700));
        fixture.assert_release(0);
    }

    #[test]
    fn startup_and_inactive_submissions_never_probe_send_or_release() {
        let fixture = Fixture::new(1);
        let output = fixture.output();
        let state = output.snapshot();
        assert!(!state.active);
        assert_eq!(state.sent_frames, 0);
        output.submit(fixture.device(1), &[], false, None);
        thread::sleep(Duration::from_millis(100));
        drop(output);
        fixture.no_packets();
        assert_eq!(fixture.requests("GET"), 0);
        fixture.assert_release(0);
    }

    #[test]
    fn failed_initial_http_probe_requires_fresh_input_and_never_releases() {
        let fixture = Fixture::new(1);
        *lock(&fixture.state.info_override) = Some(Reply {
            status: "503 Service Unavailable",
            body: Vec::new(),
            declared_length: None,
        });
        let output = fixture.output();
        output.submit(fixture.device(1), &[[1, 2, 3]], true, None);
        until(|| output.snapshot().error.is_some());
        assert!(
            output
                .snapshot()
                .error
                .unwrap()
                .contains("503 Service Unavailable")
        );
        *lock(&fixture.state.info_override) = None;
        thread::sleep(Duration::from_millis(1100));
        assert_eq!(fixture.requests("GET"), 1);
        assert_eq!(output.snapshot().sent_frames, 0);
        fixture.no_packets();
        fixture.assert_release(0);
        output.submit(fixture.device(1), &[[4, 5, 6]], true, None);
        assert_eq!(&fixture.packet()[10..], &[4, 5, 6]);
        drop(output);
        fixture.assert_release(1);
    }

    #[test]
    fn refused_probe_and_invalid_frame_never_acquire_or_release() {
        for (reported_count, live, pixels) in [
            (2, false, vec![[1, 2, 3]]),
            (1, true, vec![[1, 2, 3]]),
            (1, false, vec![]),
        ] {
            let fixture = Fixture::new(reported_count);
            fixture.state.live.store(live, Ordering::Release);
            let output = fixture.output();
            output.submit(fixture.device(1), &pixels, true, None);
            until(|| output.snapshot().error.is_some());
            assert!(!output.snapshot().active);
            drop(output);
            fixture.no_packets();
            fixture.assert_release(0);
        }
    }

    #[test]
    fn latest_frame_replaces_backlog_and_expiry_releases_only_acquired_device() {
        let fixture = Fixture::new(1);
        fixture.state.hold_info.store(true, Ordering::Release);
        let output = fixture.output();
        let deadline = Some(Instant::now() + Duration::from_secs(60));
        output.submit(fixture.device(1), &[[1, 1, 1]], true, deadline);
        until(|| fixture.requests("GET") == 1);
        output.submit(fixture.device(1), &[[2, 2, 2]], true, deadline);
        output.submit(fixture.device(1), &[[3, 4, 5]], true, deadline);
        fixture.state.hold_info.store(false, Ordering::Release);
        assert_eq!(&fixture.packet()[10..], &[3, 4, 5]);
        until(|| fixture.requests("POST") == 1 && !output.snapshot().active);
        assert_eq!(output.snapshot().sent_frames, 1);
        fixture.no_packets();
        drop(output);
        fixture.assert_release(1);
    }

    #[test]
    fn expired_authority_cannot_send_or_renew_an_acquired_session() {
        let fixture = Fixture::new(1);
        let output = fixture.output();
        output.submit(fixture.device(1), &[[1, 2, 3]], true, None);
        assert_eq!(&fixture.packet()[10..], &[1, 2, 3]);

        let expired = Some(Instant::now());
        output.submit(fixture.device(1), &[[9, 8, 7]], true, expired);
        until(|| fixture.requests("POST") == 1 && !output.snapshot().active);
        assert_eq!(output.snapshot().sent_frames, 1);
        fixture.no_packets();

        output.submit(fixture.device(1), &[[9, 8, 7]], true, expired);
        thread::sleep(Duration::from_millis(100));
        assert!(!output.snapshot().active);
        fixture.no_packets();
        drop(output);
        fixture.assert_release(1);
    }

    #[test]
    fn future_authority_bound_does_not_interrupt_the_current_session() {
        let fixture = Fixture::new(1);
        let output = fixture.output();
        output.submit(fixture.device(1), &[[1, 2, 3]], true, None);
        fixture.packet();
        let deadline = Some(Instant::now() + Duration::from_secs(120));
        output.submit(fixture.device(1), &[[25, 25, 25]], true, deadline);
        assert_eq!(&fixture.packet()[10..], &[25, 25, 25]);
        fixture.assert_release(0);
        assert!(output.snapshot().active);
        drop(output);
        fixture.assert_release(1);
    }

    #[test]
    fn disable_during_probe_cancels_without_arbitrary_release() {
        let fixture = Fixture::new(1);
        fixture.state.hold_info.store(true, Ordering::Release);
        let output = fixture.output();
        output.submit(fixture.device(1), &[[1, 2, 3]], true, None);
        until(|| fixture.requests("GET") == 1);
        output.submit(fixture.device(1), &[], false, None);
        fixture.state.hold_info.store(false, Ordering::Release);
        thread::sleep(Duration::from_millis(50));
        drop(output);
        fixture.no_packets();
        fixture.assert_release(0);
    }

    #[test]
    fn disable_and_shutdown_release_once_and_do_not_send_black_or_state_changes() {
        for disable in [false, true] {
            let fixture = Fixture::new(1);
            let output = fixture.output();
            output.submit(fixture.device(1), &[[5, 6, 7]], true, None);
            assert_eq!(&fixture.packet()[10..], &[5, 6, 7]);
            if disable {
                output.submit(fixture.device(1), &[], false, None);
                until(|| fixture.requests("POST") == 1);
            }
            let started = Instant::now();
            drop(output);
            assert!(started.elapsed() < Duration::from_millis(1200));
            fixture.no_packets();
            fixture.assert_release(1);
        }
    }

    #[test]
    fn target_change_releases_old_target_before_refusing_failed_new_target() {
        let fixture = Fixture::new(1);
        let output = fixture.output();
        output.submit(fixture.device(1), &[[1, 2, 3]], true, None);
        fixture.packet();
        let other = DeviceConfig {
            address: Ipv4Addr::new(127, 0, 0, 2),
            led_count: 1,
        };
        let listener = TcpListener::bind((other.address, fixture.ports.http)).unwrap();
        listener.set_nonblocking(true).unwrap();
        output.submit(other, &[[4, 5, 6]], true, None);
        let mut accepted = None;
        until(|| {
            match listener.accept() {
                Ok((stream, _)) => accepted = Some(stream),
                Err(error) => assert_eq!(error.kind(), ErrorKind::WouldBlock),
            }
            accepted.is_some()
        });
        fixture.assert_release(1);
        let mut stream = accepted.unwrap();
        let request = read_request(&mut stream).unwrap();
        assert_eq!(request.method, "GET");
        assert_eq!(request.path, "/json/info");
        stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        drop(stream);
        until(|| output.snapshot().error.is_some());
        fixture.no_packets();
        fixture.assert_release(1);
        drop(output);
        fixture.assert_release(1);
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            ErrorKind::WouldBlock,
            "unacquired target must not be released"
        );
    }

    #[test]
    fn worker_sequences_wrap_per_frame_and_output_is_capped_at_thirty_fps() {
        let fixture = Fixture::new(1);
        let output = fixture.output();
        let mut first = None;
        for index in 0..17 {
            output.submit(fixture.device(1), &[[index, 8, 9]], true, None);
            let packet = fixture.packet();
            first.get_or_insert_with(Instant::now);
            assert_eq!(packet[1], index % 15 + 1);
            assert_eq!(&packet[10..], &[index, 8, 9]);
        }
        assert!(
            first.unwrap().elapsed() >= Duration::from_millis(500),
            "17 frames must span at least sixteen 30-fps intervals (with observation tolerance)"
        );
        drop(output);
        fixture.assert_release(1);
    }

    #[test]
    fn failed_health_stops_output_and_never_replays_without_new_input() {
        let fixture = Fixture::new(1);
        let output = fixture.output();
        output.submit(fixture.device(1), &[[1, 2, 3]], true, None);
        fixture.packet();
        *lock(&fixture.state.info_override) = Some(Reply {
            status: "503 Service Unavailable",
            body: Vec::new(),
            declared_length: None,
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        while output.snapshot().error.is_none() {
            assert!(Instant::now() < deadline);
            output.submit(fixture.device(1), &[[4, 5, 6]], true, None);
            thread::sleep(Duration::from_millis(40));
        }
        until(|| fixture.requests("POST") == 1);
        assert!(
            output
                .snapshot()
                .error
                .unwrap()
                .contains("503 Service Unavailable")
        );
        let get_count = fixture.requests("GET");
        let sent_frames = output.snapshot().sent_frames;
        *lock(&fixture.state.info_override) = None;
        thread::sleep(Duration::from_millis(1100));
        assert_eq!(fixture.requests("GET"), get_count);
        assert_eq!(output.snapshot().sent_frames, sent_frames);
        // Drain datagrams emitted before the health failure, then prove a fresh
        // submission, not an old notification replay, is what resumes output.
        fixture.udp.set_nonblocking(true).unwrap();
        let mut buffer = [0; 1451];
        while fixture.udp.recv(&mut buffer).is_ok() {}
        fixture.udp.set_nonblocking(false).unwrap();
        output.submit(fixture.device(1), &[[7, 8, 9]], true, None);
        assert_eq!(&fixture.packet()[10..], &[7, 8, 9]);
        drop(output);
        fixture.assert_release(2);
    }

    #[test]
    fn uncertain_release_remains_visible_and_is_not_retried_against_another_owner() {
        let fixture = Fixture::new(1);
        let output = fixture.output();
        output.submit(fixture.device(1), &[[1, 2, 3]], true, None);
        fixture.packet();
        *lock(&fixture.state.release_override) = Some(Reply {
            status: "503 Service Unavailable",
            body: Vec::new(),
            declared_length: None,
        });
        output.submit(fixture.device(1), &[], false, None);
        until(|| output.snapshot().error.is_some());
        assert!(
            output
                .snapshot()
                .error
                .unwrap()
                .contains("Realtime release uncertain")
        );
        output.submit(fixture.device(1), &[], false, None);
        thread::sleep(Duration::from_millis(50));
        assert!(output.snapshot().error.is_some());
        output.submit(fixture.device(1), &[[4, 5, 6]], true, None);
        until(|| {
            output
                .snapshot()
                .error
                .is_some_and(|error| error.contains("live=true"))
        });
        fixture.no_packets();
        drop(output);
        fixture.assert_release(1);
    }
}
