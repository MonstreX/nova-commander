//! "Access denied" in Ctrl+A's "Set" (ADR-0007) on real files whose attributes the user may not write:
//! rights set by the test on its own temporary files only, and taken back before they are removed.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::tests::{Script, hidden, hide};
use super::*;
use crate::jobs::elevation::Verb;
use crate::platform::files::{self, Flag};
use crate::vfs::local::elevated::Request as Elevated;
use crate::vfs::local::elevated_session::not_started;

fn icacls(path: &Path, arguments: &[&str]) {
    let done = Command::new("icacls")
        .arg(path)
        .args(arguments)
        .output()
        .unwrap();
    assert!(done.status.success(), "{done:?}");
}

/// Files whose attributes cannot be written while the guard lives (Everyone, by its SID: the name
/// differs by the system's language, is denied "write attributes").
struct Protected {
    _temp: tempfile::TempDir,
    files: Vec<PathBuf>,
}

impl Protected {
    fn new(count: usize) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let files: Vec<PathBuf> = (0..count)
            .map(|at| temp.path().join(format!("{at}.txt")))
            .collect();
        for file in &files {
            std::fs::write(file, "x").unwrap();
            icacls(file, &["/deny", "*S-1-1-0:(WA)"]);
        }
        Self { _temp: temp, files }
    }

    fn paths(&self) -> Vec<&Path> {
        self.files.iter().map(PathBuf::as_path).collect()
    }
}

impl Drop for Protected {
    fn drop(&mut self) {
        for file in &self.files {
            icacls(file, &["/remove:d", "*S-1-1-0"]);
        }
    }
}

fn helping(replies: &[Reply]) -> Script {
    Script {
        replies: replies.iter().copied().collect::<VecDeque<_>>(),
        elevate: Some(|_| Ok(())),
        ..Script::default()
    }
}

fn hidden_as(path: &Path) -> Elevated {
    let attributes = files::file_properties(path).unwrap().attributes;
    Elevated::Set {
        path: path.to_path_buf(),
        setting: Change::Attributes(attributes.with(Flag::Hidden, true)),
    }
}

#[test]
fn a_denied_change_asks_and_goes_through_the_helper_for_the_rest_too() {
    let denied = Protected::new(2);
    let mut script = helping(&[Reply::Elevate { for_all: true }]);

    let report = run(&hide(&denied.paths(), ""), &mut script);

    assert!(report.failures.is_empty(), "{report:?}");
    assert!(
        matches!(
            &script.asked[..],
            [Question::Elevate { verb: Verb::SetAttributes, path }] if *path == denied.files[0]
        ),
        "asked once: {:?}",
        script.asked
    );
    let wanted: Vec<Elevated> = denied.files.iter().map(|file| hidden_as(file)).collect();
    assert_eq!(script.elevated, wanted);
}

#[test]
fn skip_goes_on_to_the_usual_error_question() {
    let denied = Protected::new(1);
    let mut script = helping(&[Reply::Refuse { for_all: false }, Reply::Skip]);

    let report = run(&hide(&denied.paths(), ""), &mut script);

    assert!(script.elevated.is_empty());
    assert!(matches!(
        &script.asked[..],
        [
            Question::Elevate { .. },
            Question::Failed {
                failure: "Cannot set attributes for",
                ..
            }
        ]
    ));
    assert_eq!(report.failures.len(), 1);
    assert!(!hidden(&denied.files[0]));
}

#[test]
fn esc_stops_the_task() {
    let denied = Protected::new(2);
    let mut script = helping(&[Reply::Cancel]);

    let report = run(&hide(&denied.paths(), ""), &mut script);

    assert!(report.cancelled);
    assert_eq!(script.asked.len(), 1, "nothing more after Esc");
    assert!(script.progress.finished.is_empty());
}

#[test]
fn a_helper_that_could_not_start_leaves_the_error_and_is_not_offered_again() {
    let denied = Protected::new(2);
    let mut script = Script {
        replies: VecDeque::from([Reply::Elevate { for_all: false }, Reply::SkipAll]),
        elevate: Some(|_| Err(not_started(std::io::Error::other("refused")))),
        ..Script::default()
    };

    let report = run(&hide(&denied.paths(), ""), &mut script);

    assert_eq!(script.elevated.len(), 1, "tried once");
    assert!(matches!(
        &script.asked[..],
        [Question::Elevate { .. }, Question::Failed { .. }]
    ));
    assert_eq!(report.failures.len(), 2, "{report:?}");
}

#[test]
fn a_change_the_helper_also_fails_is_the_usual_error_and_retry_goes_through_it_again() {
    let denied = Protected::new(1);
    let mut script = Script {
        replies: VecDeque::from([Reply::Elevate { for_all: true }, Reply::Retry, Reply::Skip]),
        elevate: Some(|_| Err(std::io::Error::from_raw_os_error(5))),
        ..Script::default()
    };

    run(&hide(&denied.paths(), ""), &mut script);

    assert_eq!(script.elevated.len(), 2, "Retry: through the helper again");
    assert_eq!(script.asked.len(), 3, "{:?}", script.asked);
}

#[test]
fn each_change_says_why_it_wants_the_rights_as_far_does() {
    let cases = [
        (Change::Owner("x".into()), Verb::SetOwner),
        (Change::Encrypted(true), Verb::Encrypt),
        (Change::Encrypted(false), Verb::Decrypt),
        (Change::Attributes(Default::default()), Verb::SetAttributes),
        (Change::Times(Default::default()), Verb::Open),
        (Change::LinkTarget("x".into()), Verb::Open),
        (Change::Compressed(true), Verb::Open),
        (Change::Sparse(true), Verb::Open),
        (Change::RemoveReparsePoint, Verb::Open),
    ];
    for (change, wanted) in cases {
        assert_eq!(verb(&change), wanted, "{change:?}");
    }
    assert_eq!(
        failure(&Change::LinkTarget("x".into())),
        "Cannot create the link"
    );
}

/// The Administrators group by its name in the system's language.
fn administrators() -> String {
    let script = "[Console]::OutputEncoding = [Text.Encoding]::UTF8; (New-Object System.Security.Principal.SecurityIdentifier 'S-1-5-32-544')\
                  .Translate([System.Security.Principal.NTAccount]).Value";
    let out = Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn an_owner_the_user_may_not_give_asks_access_denied_as_far_does() {
    // FAR's `ElevationRequired`: `ERROR_INVALID_OWNER` too, which a user without the owner privileges
    // gets for any owner but themselves.
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("a.txt");
    std::fs::write(&file, "a").unwrap();
    let owner = administrators();
    let mut script = helping(&[Reply::Elevate { for_all: false }]);
    let mut request = hide(&[&file], &owner);
    request.wanted = [None; 20];

    let report = run(&request, &mut script);

    assert!(report.failures.is_empty(), "{report:?}");
    match &script.asked[..] {
        [
            Question::Elevate {
                verb: Verb::SetOwner,
                ..
            },
        ] => assert_eq!(
            script.elevated,
            [Elevated::Set {
                path: file.clone(),
                setting: Change::Owner(owner),
            }]
        ),
        // A run with the administrator's rights gives it itself.
        [] => assert!(script.elevated.is_empty()),
        other => panic!("{other:?}"),
    }
}
