//! One running copy per user, so there is only ever one tray icon.
//!
//! The first copy locks a file in the data folder and listens on a local
//! port. A later launch finds the lock taken, asks that copy to show its
//! window and exits. While the running copy is closing for an update, it
//! answers so, and the new version waits for it to finish and takes over.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const LOCK_FILE: &str = "know-giver.lock";
const PORT_FILE: &str = "know-giver.port";

/// Messages between copies, one line each.
const SHOW: &str = "show";
const STAY_HIDDEN: &str = "hidden";
const DONE: &str = "ok";
const CLOSING: &str = "closing";

/// How long a new launch keeps trying to reach a copy that is still starting.
const REACH_TIMEOUT: Duration = Duration::from_secs(5);
/// How long the new version waits for the old one to close after an update.
const HANDOVER_TIMEOUT: Duration = Duration::from_secs(60);
const RETRY: Duration = Duration::from_millis(200);
const IO_TIMEOUT: Duration = Duration::from_secs(1);

/// Proof that this launch is the only running copy, from [`claim_instance`].
#[derive(Debug)]
pub struct SingleInstance(pub(crate) Instance);

/// Makes this launch the running copy, or asks the copy already running to
/// show its window (unless `start_hidden`) and returns `None`.
pub fn claim_instance(start_hidden: bool) -> Option<SingleInstance> {
    match claim(&kg_engine::AppPaths::detect().data_dir, !start_hidden) {
        Launch::First(instance) => Some(SingleInstance(instance)),
        Launch::AlreadyRunning => {
            tracing::info!("Know Giver is already running; handed over to it");
            None
        }
    }
}

/// What this launch should do.
#[derive(Debug)]
pub(crate) enum Launch {
    /// No other copy: open the app.
    First(Instance),
    /// Another copy is running and was asked to show itself.
    AlreadyRunning,
}

/// Held by the running copy for its whole life.
#[derive(Debug)]
pub(crate) struct Instance {
    _lock: Option<File>,
    listener: Option<TcpListener>,
    show: Arc<AtomicBool>,
    closing: Arc<AtomicBool>,
}

impl Instance {
    fn unguarded() -> Self {
        Self {
            _lock: None,
            listener: None,
            show: Arc::default(),
            closing: Arc::default(),
        }
    }

    /// Answers later launches from a background thread, waking `ctx` when
    /// one asks for the window.
    pub(crate) fn listen(&mut self, ctx: egui::Context) {
        let Some(listener) = self.listener.take() else {
            return;
        };
        let show = Arc::clone(&self.show);
        let closing = Arc::clone(&self.closing);
        let spawned = std::thread::Builder::new()
            .name("single-instance".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    match stream {
                        Ok(stream) => answer(stream, &show, &closing, &ctx),
                        Err(err) => tracing::debug!(%err, "launch request failed"),
                    }
                }
            });
        if let Err(err) = spawned {
            tracing::warn!(%err, "could not listen for later launches");
        }
    }

    /// True once after a later launch asked for the window.
    pub(crate) fn take_show_request(&self) -> bool {
        self.show.swap(false, Ordering::AcqRel)
    }

    /// Set while an update is replacing this copy: new launches wait for it
    /// to close instead of handing over to it.
    pub(crate) fn set_closing(&self, closing: bool) {
        self.closing.store(closing, Ordering::Release);
    }
}

fn answer(stream: TcpStream, show: &AtomicBool, closing: &AtomicBool, ctx: &egui::Context) {
    if let Err(err) = stream.set_read_timeout(Some(IO_TIMEOUT)) {
        tracing::debug!(%err, "launch request");
    }
    let mut line = String::new();
    if BufReader::new(&stream)
        .take(64)
        .read_line(&mut line)
        .is_err()
    {
        return;
    }
    let reply = if closing.load(Ordering::Acquire) {
        CLOSING
    } else {
        if line.trim() == SHOW {
            show.store(true, Ordering::Release);
            ctx.request_repaint();
        }
        DONE
    };
    if let Err(err) = (&stream).write_all(format!("{reply}\n").as_bytes()) {
        tracing::debug!(%err, "launch reply");
    }
}

/// Becomes the running copy, or hands over to the one already running.
/// `show_window` is false for background starts (sign-in), which then leave
/// the running copy as it is.
pub(crate) fn claim(data_dir: &Path, show_window: bool) -> Launch {
    if let Err(err) = std::fs::create_dir_all(data_dir) {
        tracing::warn!(%err, "no data folder; more than one copy may run");
        return Launch::First(Instance::unguarded());
    }
    let lock_path = data_dir.join(LOCK_FILE);
    let port_path = data_dir.join(PORT_FILE);
    let started = Instant::now();
    let mut handing_over = false;
    loop {
        let file = match OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
        {
            Ok(file) => file,
            Err(err) => {
                tracing::warn!(%err, "could not open the instance lock; more than one copy may run");
                return Launch::First(Instance::unguarded());
            }
        };
        match file.try_lock() {
            Ok(()) => return Launch::First(first(file, &port_path)),
            Err(TryLockError::Error(err)) => {
                tracing::warn!(%err, "could not lock the instance file; more than one copy may run");
                return Launch::First(Instance::unguarded());
            }
            Err(TryLockError::WouldBlock) => {}
        }
        drop(file);

        let message = if show_window { SHOW } else { STAY_HIDDEN };
        match ask_running(&port_path, message) {
            Some(reply) if reply == CLOSING => handing_over = true,
            Some(_) => return Launch::AlreadyRunning,
            None => {}
        }
        let limit = if handing_over {
            HANDOVER_TIMEOUT
        } else {
            REACH_TIMEOUT
        };
        if started.elapsed() > limit {
            // A copy holds the lock but does not answer. A second tray icon
            // would be worse than this launch doing nothing.
            tracing::warn!("another copy is running but did not answer");
            return Launch::AlreadyRunning;
        }
        std::thread::sleep(RETRY);
    }
}

fn first(lock: File, port_path: &Path) -> Instance {
    let mut instance = Instance::unguarded();
    instance._lock = Some(lock);
    match TcpListener::bind((Ipv4Addr::LOCALHOST, 0)) {
        Ok(listener) => match listener.local_addr() {
            Ok(addr) => {
                if let Err(err) = std::fs::write(port_path, addr.port().to_string()) {
                    tracing::warn!(%err, "could not save the instance port");
                } else {
                    instance.listener = Some(listener);
                }
            }
            Err(err) => tracing::warn!(%err, "could not read the instance port"),
        },
        Err(err) => tracing::warn!(%err, "could not listen for later launches"),
    }
    instance
}

/// Asks the running copy to show its window (from the reminder popup).
pub(crate) fn show_running() {
    let port_path = kg_engine::AppPaths::detect().data_dir.join(PORT_FILE);
    if ask_running(&port_path, SHOW).is_none() {
        tracing::warn!("could not reach the running copy");
    }
}

/// Sends `message` to the running copy and returns its reply.
fn ask_running(port_path: &Path, message: &str) -> Option<String> {
    let port: u16 = std::fs::read_to_string(port_path)
        .ok()?
        .trim()
        .parse()
        .ok()?;
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let stream = TcpStream::connect_timeout(&addr, IO_TIMEOUT).ok()?;
    stream.set_read_timeout(Some(IO_TIMEOUT)).ok()?;
    (&stream)
        .write_all(format!("{message}\n").as_bytes())
        .ok()?;
    let mut reply = String::new();
    BufReader::new(&stream)
        .take(64)
        .read_line(&mut reply)
        .ok()?;
    Some(reply.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_of(launch: Launch) -> Instance {
        match launch {
            Launch::First(instance) => instance,
            Launch::AlreadyRunning => panic!("expected the first copy"),
        }
    }

    #[test]
    fn a_second_launch_shows_the_running_copy() {
        let dir = tempfile::tempdir().unwrap();
        let mut running = first_of(claim(dir.path(), true));
        running.listen(egui::Context::default());

        assert!(matches!(claim(dir.path(), true), Launch::AlreadyRunning));
        let deadline = Instant::now() + Duration::from_secs(2);
        while !running.take_show_request() {
            assert!(Instant::now() < deadline, "the window was not asked for");
            std::thread::sleep(Duration::from_millis(10));
        }

        // A background start leaves the running copy hidden.
        assert!(matches!(claim(dir.path(), false), Launch::AlreadyRunning));
        std::thread::sleep(Duration::from_millis(100));
        assert!(!running.take_show_request());
    }

    #[test]
    fn an_update_hands_over_once_the_old_copy_closes() {
        let dir = tempfile::tempdir().unwrap();
        let mut old = first_of(claim(dir.path(), true));
        old.listen(egui::Context::default());
        old.set_closing(true);

        let path = dir.path().to_path_buf();
        let new = std::thread::spawn(move || matches!(claim(&path, true), Launch::First(_)));
        std::thread::sleep(Duration::from_millis(500));
        assert!(!new.is_finished(), "the new version waits for the old one");
        drop(old);
        assert!(new.join().unwrap(), "the new version takes over");
    }

    #[test]
    fn the_lock_is_free_again_after_the_copy_closes() {
        let dir = tempfile::tempdir().unwrap();
        drop(first_of(claim(dir.path(), true)));
        assert!(matches!(claim(dir.path(), true), Launch::First(_)));
    }
}
