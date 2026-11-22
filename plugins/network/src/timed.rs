//! The network's listings waited for a while (owner's rule over FAR's, ADR-0014: FAR waits for a
//! workgroup that does not answer for minutes, the plugin's other calls with it): past the wait a
//! listing is an error ("does not answer yet") and goes on by itself; its answer, when it comes, is the
//! next listing of that resource's. A resource is asked once at a time: while its listing goes on, it
//! does not answer yet. The other calls are quick and asked as they are.

use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use nc_plugin_sdk::system::Waited;
use nc_plugin_sdk::system::network::{NetResource, Share};

use crate::browse::Network;

/// How long a listing is waited for.
pub const WAIT: Duration = Duration::from_secs(10);
/// How long the workgroup a first panel opens at is waited for before it stays at the top.
pub const CHECK_WAIT: Duration = Duration::from_secs(2);

/// The answers kept for the next listings and the resources being listed, by resource.
#[derive(Default)]
struct State {
    answers: HashMap<String, io::Result<Vec<NetResource>>>,
    asked: HashSet<String>,
}

type Shared = Arc<Mutex<State>>;

/// A network whose listings are waited for at most [`WAIT`].
pub struct Timed {
    inner: Arc<dyn Network + Send + Sync>,
    wait: Duration,
    state: Shared,
}

fn key(parent: Option<&NetResource>) -> String {
    parent.map_or(String::new(), |one| {
        format!("{}\u{0}{}", one.provider, one.remote_name).to_lowercase()
    })
}

/// Whether `error` is a listing's that does not answer yet (not the network's own refusal).
pub fn not_answering(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::TimedOut
}

fn not_answering_error(wait: Duration) -> io::Error {
    io::Error::new(
        io::ErrorKind::TimedOut,
        format!(
            "The network does not answer in {} seconds: it is still asked, Ctrl+R shows its \
             answer once it comes",
            wait.as_secs()
        ),
    )
}

fn lock(state: &Shared) -> std::sync::MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Timed {
    pub fn new(inner: Arc<dyn Network + Send + Sync>, wait: Duration) -> Self {
        Self {
            inner,
            wait,
            state: Shared::default(),
        }
    }

    /// An answer kept for the next listing of `parent`.
    pub fn keep(&self, parent: Option<&NetResource>, answer: io::Result<Vec<NetResource>>) {
        lock(&self.state).answers.insert(key(parent), answer);
    }

    /// The listing of `parent` waited for `wait`: an answer kept is taken, a resource being listed
    /// does not answer yet.
    pub fn enumerate_within(
        &self,
        parent: Option<&NetResource>,
        wait: Duration,
    ) -> io::Result<Vec<NetResource>> {
        let key = key(parent);
        {
            let mut state = lock(&self.state);
            if let Some(kept) = state.answers.remove(&key) {
                return kept;
            }
            if !state.asked.insert(key.clone()) {
                return Err(not_answering_error(wait));
            }
        }
        let inner = Arc::clone(&self.inner);
        let asked = parent.cloned();
        let shared = Arc::clone(&self.state);
        let late_key = key.clone();
        let late = move |answer| {
            let mut state = lock(&shared);
            state.asked.remove(&late_key);
            state.answers.insert(late_key, answer);
        };
        // A listing that breaks answers an error: the resource is never left marked as being listed.
        let call = move || {
            let listed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                inner.enumerate(asked.as_ref())
            }));
            listed.unwrap_or_else(|_| Err(io::Error::other("The network's listing broke")))
        };
        match nc_plugin_sdk::system::waited(wait, call, late) {
            Waited::Answered(answer) => {
                lock(&self.state).asked.remove(&key);
                answer
            }
            Waited::Late => Err(not_answering_error(wait)),
            // No thread could start: nothing listing it, so it is asked again next time.
            Waited::Failed => {
                lock(&self.state).asked.remove(&key);
                Err(io::Error::other("The network's listing could not start"))
            }
        }
    }
}

impl Network for Timed {
    fn enumerate(&self, parent: Option<&NetResource>) -> io::Result<Vec<NetResource>> {
        self.enumerate_within(parent, self.wait)
    }

    fn connected(&self) -> io::Result<Vec<NetResource>> {
        self.inner.connected()
    }

    fn information(&self, remote: &str) -> io::Result<NetResource> {
        self.inner.information(remote)
    }

    fn parent(&self, resource: &NetResource) -> io::Result<NetResource> {
        self.inner.parent(resource)
    }

    fn shares(&self, server: &str) -> Vec<Share> {
        self.inner.shares(server)
    }

    fn connect(&self, resource: &NetResource) -> io::Result<()> {
        self.inner.connect(resource)
    }

    fn keep(&self, parent: Option<&NetResource>, answer: io::Result<Vec<NetResource>>) {
        Timed::keep(self, parent, answer);
    }
}

#[cfg(test)]
#[path = "timed_tests.rs"]
mod tests;
