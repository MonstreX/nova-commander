use std::sync::mpsc;

use super::*;

/// A network whose listing tells it has begun and then waits until the test lets it go.
struct Held {
    entered: Mutex<mpsc::Sender<()>>,
    release: Mutex<mpsc::Receiver<()>>,
}

/// A network held by the test: its `Timed` waited for `wait`, the listings' beginnings heard, the
/// sender that lets each go.
fn held(wait: Duration) -> (Timed, mpsc::Receiver<()>, mpsc::Sender<()>) {
    let (entered, began) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let net = Held {
        entered: Mutex::new(entered),
        release: Mutex::new(held),
    };
    (Timed::new(Arc::new(net), wait), began, release)
}

impl Network for Held {
    fn enumerate(&self, _parent: Option<&NetResource>) -> io::Result<Vec<NetResource>> {
        // The test may have heard all it wanted: a beginning nobody hears changes nothing.
        let _ = (self.entered.lock().unwrap_or_else(PoisonError::into_inner)).send(());
        let release = self.release.lock().unwrap_or_else(PoisonError::into_inner);
        release.recv().ok();
        Ok(vec![NetResource {
            remote_name: "answered".to_string(),
            ..NetResource::default()
        }])
    }

    fn connected(&self) -> io::Result<Vec<NetResource>> {
        Ok(Vec::new())
    }

    fn information(&self, _remote: &str) -> io::Result<NetResource> {
        Err(io::Error::from(io::ErrorKind::NotFound))
    }

    fn parent(&self, _resource: &NetResource) -> io::Result<NetResource> {
        Err(io::Error::from(io::ErrorKind::NotFound))
    }

    fn shares(&self, _server: &str) -> Vec<Share> {
        Vec::new()
    }

    fn connect(&self, _resource: &NetResource) -> io::Result<()> {
        Ok(())
    }
}

/// A listing past its wait is "does not answer yet"; asked again while it goes on, it does not answer
/// yet at once (no second listing); its answer, once it came, is the next listing's.
#[test]
fn a_late_answer_is_the_next_listings() {
    let (net, began, release) = held(Duration::ZERO);
    let first = net.enumerate(None);
    assert_eq!(
        first.map_err(|error| error.kind()).err(),
        Some(io::ErrorKind::TimedOut)
    );
    began.recv().unwrap();
    // Still held (not let go yet): only the resource being listed can answer so.
    let again = net.enumerate(None);
    assert_eq!(
        again.map_err(|error| error.kind()).err(),
        Some(io::ErrorKind::TimedOut)
    );
    release.send(()).unwrap();
    // The late answer lands in the kept answers: looked at until it is there.
    let shared = Arc::clone(&net.state);
    let landed = (0..1_000_000).any(|_| {
        std::thread::yield_now();
        !lock(&shared).answers.is_empty()
    });
    assert!(landed, "the late answer is kept");
    let second = net.enumerate(None).unwrap();
    assert_eq!(second[0].remote_name, "answered");
    assert!(began.try_recv().is_err(), "listed once");
}

/// An answer kept ahead is the next listing's; the one after asks again.
#[test]
fn a_kept_answer_is_taken_once() {
    let (net, _began, release) = held(Duration::from_secs(30));
    let resource = NetResource {
        remote_name: "WORKGROUP".to_string(),
        ..NetResource::default()
    };
    net.keep(Some(&resource), Ok(Vec::new()));
    assert!(net.enumerate(Some(&resource)).unwrap().is_empty());
    release.send(()).unwrap();
    assert_eq!(
        net.enumerate(Some(&resource)).unwrap().len(),
        1,
        "asked again"
    );
}
