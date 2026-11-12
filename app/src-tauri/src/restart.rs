//! Tray "Restart": flag the relaunch and quit; the new process is started
//! only after this one has let go of the single-instance lock. Started any
//! earlier, it would find the lock held, hand its (empty) arguments to the
//! exiting daemon and quit, leaving nothing running.

use std::io;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Manager, Runtime};

use crate::instance;

/// How long to wait for the lock to disappear before giving up.
const LOCK_POLLS: u32 = 50;
const LOCK_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Set by the tray, read once the event loop has returned.
#[derive(Clone, Default)]
pub(crate) struct Restart(Arc<AtomicBool>);

impl Restart {
    fn request(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub(crate) fn is_requested(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum RestartError {
    #[error("the single-instance lock was still held; not relaunching")]
    LockStillHeld,
    #[error("could not start the new process")]
    Spawn(#[source] io::Error),
}

/// Flags the relaunch and quits the event loop.
pub(crate) fn request<R: Runtime>(app: &AppHandle<R>) {
    app.state::<Restart>().request();
    app.exit(0);
}

/// Runs after the event loop has returned and the lock was released.
pub(crate) fn relaunch_if_requested(restart: &Restart) -> Result<(), RestartError> {
    if !restart.is_requested() {
        return Ok(());
    }
    relaunch_when_unlocked(
        instance::daemon_running,
        || std::thread::sleep(LOCK_POLL_INTERVAL),
        LOCK_POLLS,
        spawn_self,
    )
}

/// Polls `lock_held` until it reports the lock gone, then spawns; never
/// spawns while it is held.
fn relaunch_when_unlocked(
    mut lock_held: impl FnMut() -> bool,
    mut wait: impl FnMut(),
    polls: u32,
    spawn: impl FnOnce() -> io::Result<()>,
) -> Result<(), RestartError> {
    for poll in 0..polls {
        if !lock_held() {
            return spawn().map_err(RestartError::Spawn);
        }
        if poll + 1 < polls {
            wait();
        }
    }
    Err(RestartError::LockStillHeld)
}

fn spawn_self() -> io::Result<()> {
    Command::new(std::env::current_exe()?)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn nothing_is_spawned_unless_requested() {
        let restart = Restart::default();
        assert!(!restart.is_requested());
        assert!(relaunch_if_requested(&restart).is_ok());
    }

    #[test]
    fn the_flag_is_shared_between_clones() {
        let restart = Restart::default();
        restart.clone().request();
        assert!(restart.is_requested());
    }

    #[test]
    fn spawns_only_after_the_lock_is_released() {
        let events = RefCell::new(Vec::new());
        let mut held_for = 2;
        let result = relaunch_when_unlocked(
            || {
                events.borrow_mut().push("check");
                let held = held_for > 0;
                held_for -= 1;
                held
            },
            || events.borrow_mut().push("wait"),
            10,
            || {
                events.borrow_mut().push("spawn");
                Ok(())
            },
        );
        assert!(result.is_ok());
        assert_eq!(
            *events.borrow(),
            ["check", "wait", "check", "wait", "check", "spawn"]
        );
    }

    #[test]
    fn a_lock_that_stays_held_means_no_spawn() {
        let mut spawned = false;
        let result = relaunch_when_unlocked(|| true, || {}, 5, || {
            spawned = true;
            Ok(())
        });
        assert!(matches!(result, Err(RestartError::LockStillHeld)));
        assert!(!spawned);
    }

    #[test]
    fn a_failed_spawn_is_reported() {
        let result = relaunch_when_unlocked(|| false, || {}, 1, || {
            Err(io::Error::other("denied"))
        });
        assert!(matches!(result, Err(RestartError::Spawn(_))));
    }
}
