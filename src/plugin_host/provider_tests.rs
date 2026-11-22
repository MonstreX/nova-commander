//! The plugin's provider against a stand-in for the plugin's thread: the calls it sends, the answers,
//! a reading stopped while the plugin is busy, a plugin gone.

use std::io::{Read, Write};
use std::time::Instant;

use super::*;
use crate::vfs::{Entry, PanelLook};

/// The stand-in's file `/a`, "hello": the answers to the calls on any file.
fn file_answer(call: FileCall) -> io::Result<FileAnswer> {
    const DATA: &[u8] = b"hello";
    match call {
        FileCall::Stat => Ok(FileAnswer::Entry(Entry {
            name: "a".to_string(),
            size: DATA.len() as u64,
            ..Entry::default()
        })),
        FileCall::Read { offset, size } => {
            let from = usize::try_from(offset)
                .unwrap_or(usize::MAX)
                .min(DATA.len());
            let to = from.saturating_add(size).min(DATA.len());
            Ok(FileAnswer::Data(DATA[from..to].to_vec()))
        }
        FileCall::Write { data, .. } if data == b"refuse" => {
            Err(io::Error::new(io::ErrorKind::StorageFull, "full"))
        }
        _ => Ok(FileAnswer::Done),
    }
}

/// A provider and the jobs it sends (their calls' numbers left out; the line closed ends them).
fn provider(name: &str) -> (PluginProvider, Receiver<Job>, Arc<Calls>) {
    let (sender, calls) = channel::<super::super::calls::Call>();
    let line = Arc::new(Calls::new(sender, Arc::default()));
    let (forward, jobs) = channel();
    std::thread::spawn(move || {
        while let Ok(call) = calls.recv() {
            if forward.send(call.job).is_err() {
                return;
            }
        }
    });
    (PluginProvider::new(name, line.clone()), jobs, line)
}

/// Answers each job as the plugin would: entries named after the instance and path, a title.
fn answering(jobs: Receiver<Job>) -> std::thread::JoinHandle<Vec<String>> {
    std::thread::spawn(move || {
        let mut heard = Vec::new();
        while let Ok(job) = jobs.recv() {
            heard.push(format!("{job:?}"));
            match job {
                Job::List {
                    instance,
                    path,
                    reply,
                } => {
                    let answer = if path == "/gone" {
                        Err(io::Error::new(io::ErrorKind::NotFound, "no such"))
                    } else if path == "/odd" {
                        let named = |name: &str| Entry {
                            name: name.to_string(),
                            ..Entry::default()
                        };
                        Ok(["", ".", "..", "a/b", "a\\b", "ok"].map(named).to_vec())
                    } else {
                        Ok(vec![Entry {
                            name: format!("{instance}:{}", path.trim_start_matches('/')),
                            ..Entry::default()
                        }])
                    };
                    let _ = reply.send(answer);
                }
                Job::Look {
                    instance,
                    path,
                    reply,
                } => {
                    let _ = reply.send(PanelLook {
                        title: Some(format!("{instance} at {path}")),
                        ..PanelLook::default()
                    });
                }
                Job::File { call, reply, .. } => {
                    let _ = reply.send(file_answer(call));
                }
                Job::Run { .. }
                | Job::Synchro(_)
                | Job::Close(_)
                | Job::Put { .. }
                | Job::Get { .. }
                | Job::Delete { .. }
                | Job::MakeFolder { .. }
                | Job::HostFile { .. }
                | Job::DiskFile { .. }
                | Job::Create { .. }
                | Job::Key { .. }
                | Job::Command { .. }
                | Job::OpenFile { .. }
                | Job::FindList { .. }
                | Job::Items { .. }
                | Job::SearchOpen { .. }
                | Job::EditorKey { .. } => {}
            }
        }
        heard
    })
}

/// The thread's result, waited for at most 10 s: a reading that never stops fails the test instead of
/// hanging it.
fn within<T>(thread: std::thread::JoinHandle<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !thread.is_finished() {
        assert!(Instant::now() < deadline, "the reading did not end in time");
        std::thread::yield_now();
    }
    thread.join().unwrap()
}

fn listed(provider: &PluginProvider, dir: &VfsPath) -> io::Result<Vec<String>> {
    let mut names = Vec::new();
    provider.list(dir, &AtomicBool::new(false), &mut |entry| {
        names.push(entry.name);
    })?;
    Ok(names)
}

#[test]
fn a_folder_and_a_title_are_asked_of_the_plugins_thread_by_instance_and_path() {
    let (provider, jobs, line) = provider("Probe");
    assert_eq!(provider.id(), "probe", "the paths' scheme");
    assert!(provider.leads_back(), "its root's \"..\" leads back");
    let plugin = answering(jobs);
    let docs = VfsPath::in_session("probe", "one", "/docs");
    assert_eq!(listed(&provider, &docs).unwrap(), ["one:docs"]);
    assert_eq!(
        provider
            .look(&docs, &AtomicBool::new(false))
            .title
            .as_deref(),
        Some("one at /docs")
    );
    let error = listed(&provider, &VfsPath::in_session("probe", "one", "/gone")).unwrap_err();
    assert_eq!(
        (error.kind(), error.to_string()),
        (io::ErrorKind::NotFound, "no such".to_string())
    );
    line.close();
    let heard = plugin.join().unwrap();
    assert_eq!(heard, ["List(/docs)", "Look(/docs)", "List(/gone)"]);
}

#[test]
fn names_that_would_lead_elsewhere_are_not_the_folders() {
    let (provider, jobs, line) = provider("probe");
    let plugin = answering(jobs);
    let odd = VfsPath::in_session("probe", "one", "/odd");
    assert_eq!(listed(&provider, &odd).unwrap(), ["a\\b", "ok"]);
    line.close();
    plugin.join().unwrap();
}

#[test]
fn a_title_asked_while_the_plugin_is_busy_stops_waiting_with_the_reading() {
    let (provider, jobs, _line) = provider("probe");
    let stop = Arc::new(AtomicBool::new(false));
    let asking = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            provider
                .look(&VfsPath::in_session("probe", "one", "/"), &stop)
                .title
        })
    };
    let held: Job = jobs.recv().unwrap();
    stop.store(true, Ordering::Relaxed);
    assert_eq!(within(asking), None);
    drop(held);
}

#[test]
fn a_path_that_is_no_plugins_panel_is_not_sent() {
    let (provider, jobs, _line) = provider("probe");
    let local = VfsPath::local(r"C:\");
    let error = listed(&provider, &local).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    assert_eq!(provider.look(&local, &AtomicBool::new(false)).title, None);
    assert!(jobs.try_recv().is_err(), "nothing asked");
}

#[test]
fn a_reading_stopped_while_the_plugin_is_busy_stops_waiting() {
    let (provider, jobs, line) = provider("probe");
    let stop = Arc::new(AtomicBool::new(false));
    let reading = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            let dir = VfsPath::in_session("probe", "one", "/");
            provider
                .list(&dir, &stop, &mut |_| {})
                .map_err(|error| error.kind())
        })
    };
    // The plugin has the question and does not answer.
    let held: Job = jobs.recv().unwrap();
    line.state.begin(1);
    let asked = Instant::now();
    stop.store(true, Ordering::Relaxed);
    assert_eq!(within(reading), Err(io::ErrorKind::Interrupted));
    assert!(asked.elapsed() < Duration::from_secs(5));
    assert!(line.state.cancelled(), "the plugin hears it (ADR-0019 §5)");
    if let Job::List { reply, .. } = held {
        let _ = reply.send(Ok(Vec::new()));
    }
}

#[test]
fn a_plugin_gone_or_dropping_the_question_answers_that_it_is_gone() {
    let (provider, jobs, line) = provider("probe");
    let dir = VfsPath::in_session("probe", "one", "/");
    let dropping = std::thread::spawn(move || {
        while let Ok(job) = jobs.recv() {
            drop(job);
        }
    });
    let error = listed(&provider, &dir).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotConnected);
    assert_eq!(error.to_string(), "The plugin probe is no longer loaded");
    assert_eq!(provider.look(&dir, &AtomicBool::new(false)).title, None);
    line.close();
    dropping.join().unwrap();
    assert_eq!(
        listed(&provider, &dir).unwrap_err().kind(),
        io::ErrorKind::NotConnected
    );
    line.close();
}

#[test]
fn a_plugin_slower_than_a_look_is_waited_for_until_it_answers() {
    let (provider, jobs, line) = provider("probe");
    let slow = std::thread::spawn(move || {
        while let Ok(job) = jobs.recv() {
            // Several looks pass before the answer: a reading not asked to stop waits on.
            std::thread::sleep(LOOK * 5);
            match job {
                Job::List { reply, .. } => {
                    let _ = reply.send(Ok(vec![Entry {
                        name: "late".to_string(),
                        ..Entry::default()
                    }]));
                }
                Job::Look { reply, .. } => {
                    let _ = reply.send(PanelLook {
                        title: Some("Late".to_string()),
                        ..PanelLook::default()
                    });
                }
                Job::Run { .. }
                | Job::Synchro(_)
                | Job::Close(_)
                | Job::Put { .. }
                | Job::Get { .. }
                | Job::Delete { .. }
                | Job::MakeFolder { .. }
                | Job::HostFile { .. }
                | Job::DiskFile { .. }
                | Job::Create { .. }
                | Job::File { .. }
                | Job::Key { .. }
                | Job::Command { .. }
                | Job::OpenFile { .. }
                | Job::FindList { .. }
                | Job::Items { .. }
                | Job::SearchOpen { .. }
                | Job::EditorKey { .. } => {}
            }
        }
    });
    let dir = VfsPath::in_session("probe", "one", "/");
    assert_eq!(listed(&provider, &dir).unwrap(), ["late"]);
    assert_eq!(
        provider
            .look(&dir, &AtomicBool::new(false))
            .title
            .as_deref(),
        Some("Late")
    );
    line.close();
    within(slow);
}

#[test]
fn a_file_is_read_whole_and_anywhere_its_length_its_entry() {
    let (provider, jobs, line) = provider("probe");
    let plugin = answering(jobs);
    let file = VfsPath::in_session("probe", "one", "/a");
    assert_eq!(provider.stat(&file).unwrap().size, 5);
    let mut whole = String::new();
    io::Read::read_to_string(&mut *provider.open_read(&file).unwrap(), &mut whole).unwrap();
    assert_eq!(whole, "hello");
    let mut random = provider.open_random(&file).unwrap();
    let mut buffer = [0; 3];
    assert_eq!(random.read_at(1, &mut buffer).unwrap(), 3);
    assert_eq!(&buffer, b"ell");
    assert_eq!(
        random.read_at(4, &mut buffer).unwrap(),
        1,
        "short at the end"
    );
    assert_eq!(random.read_at(9, &mut buffer).unwrap(), 0, "past it");
    assert_eq!(random.len().unwrap(), 5);
    line.close();
    let heard = plugin.join().unwrap();
    assert_eq!(heard[0], "File(/a, Stat)");
    assert!(
        heard.contains(&"File(/a, Read { offset: 1, size: 3 })".to_string()),
        "{heard:?}"
    );
}

#[test]
fn a_file_written_new_is_made_at_once_then_appended_to() {
    let (provider, jobs, line) = provider("probe");
    let plugin = answering(jobs);
    let file = VfsPath::in_session("probe", "one", "/b");
    let mut written = provider.open_write(&file, WriteMode::New).unwrap();
    assert_eq!(written.write(b"one").unwrap(), 3);
    assert_eq!(written.write(b"").unwrap(), 0, "nothing sent");
    written.flush().unwrap();
    let error = written.write(b"refuse").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::StorageFull);
    let mut appended = provider.open_write(&file, WriteMode::Append).unwrap();
    appended.write_all(b"two").unwrap();
    line.close();
    let heard = plugin.join().unwrap();
    assert_eq!(
        heard,
        [
            "File(/b, Write { append: false, data: [] })",
            "File(/b, Write { append: true, data: [111, 110, 101] })",
            // Flushed: the copy's last step, the file whole (API 9), told once.
            "File(/b, End)",
            "File(/b, Write { append: true, data: [114, 101, 102, 117, 115, 101] })",
            "File(/b, Write { append: true, data: [116, 119, 111] })",
        ]
    );
}

#[test]
fn folders_renames_and_removals_are_the_plugins_and_stay_in_its_panel() {
    let (provider, jobs, line) = provider("probe");
    let plugin = answering(jobs);
    let at = |path: &str| VfsPath::in_session("probe", "one", path);
    provider.make_dir(&at("/new")).unwrap();
    provider.rename(&at("/a"), &at("/b"), true).unwrap();
    provider.remove(&at("/a"), Removal::File).unwrap();
    provider.remove(&at("/new"), Removal::EmptyDir).unwrap();
    let other = VfsPath::in_session("probe", "two", "/b");
    let across = provider.rename(&at("/a"), &other, false).unwrap_err();
    assert_eq!(across.kind(), io::ErrorKind::CrossesDevices);
    let local = provider
        .rename(&at("/a"), &VfsPath::local(r"C:\b"), false)
        .unwrap_err();
    assert_eq!(local.kind(), io::ErrorKind::CrossesDevices);
    let trash = provider.remove(&at("/a"), Removal::Trash).unwrap_err();
    assert_eq!(trash.kind(), io::ErrorKind::Unsupported);
    line.close();
    assert_eq!(
        plugin.join().unwrap(),
        [
            "File(/new, MakeDir)",
            "File(/a, Rename { to: \"/b\", replace: true })",
            "File(/a, Remove { folder: false })",
            "File(/new, Remove { folder: true })",
        ]
    );
}

#[test]
fn a_file_of_a_plugin_gone_says_so() {
    let (provider, _jobs, line) = provider("probe");
    line.close();
    let file = VfsPath::in_session("probe", "one", "/a");
    assert_eq!(
        provider.stat(&file).unwrap_err().kind(),
        io::ErrorKind::NotConnected
    );
    assert!(provider.open_random(&file).is_err());
    assert!(provider.open_write(&file, WriteMode::New).is_err());
    let mut reader = provider.open_read(&file).unwrap();
    let mut buffer = [0; 4];
    assert_eq!(
        reader.read(&mut buffer).unwrap_err().kind(),
        io::ErrorKind::NotConnected
    );
}

#[test]
fn a_file_call_stops_with_the_work_that_asks_and_the_plugin_hears_it() {
    // Review R14: F7 on a plugin's panel, a copy out of it.
    let (provider, jobs, line) = provider("probe");
    let stop = Arc::new(AtomicBool::new(false));
    let making = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            let path = VfsPath::in_session("probe", "one", "/new");
            crate::vfs::stop::stopped_by(stop, || provider.make_dir(&path))
                .map_err(|error| crate::vfs::stop::is_stopped(&error))
        })
    };
    let held: Job = jobs.recv().unwrap();
    line.state.begin(1);
    stop.store(true, Ordering::Relaxed);
    // Not `Interrupted`: readers and writers would send the call again and again (review of P2).
    assert_eq!(within(making), Err(true));
    assert!(line.state.cancelled(), "the plugin hears it");
    drop(held);
}

/// Once the work is stopped no file call is sent and a write is not tried again; the writer let go
/// then (never flushed) still tells the plugin the file was abandoned (API 9's `WRITE_ABORT`, the
/// cleanup past the stop), and nothing else.
#[test]
fn once_the_work_is_stopped_no_file_call_is_sent_and_a_write_is_not_tried_again() {
    let (provider, jobs, line) = provider("probe");
    let stop = Arc::new(AtomicBool::new(true));
    let path = VfsPath::in_session("probe", "one", "/a");
    let (made, written, writer) = crate::vfs::stop::stopped_by(stop.clone(), || {
        let made = provider.make_dir(&path);
        let mut writer = plugin_files::Writer::new(&provider.line, &path);
        let written = writer.write_all(&[1; 64]);
        let flushed = writer.flush();
        assert!(flushed.is_err_and(|error| crate::vfs::stop::is_stopped(&error)));
        (made, written, writer)
    });
    assert!(made.is_err_and(|error| crate::vfs::stop::is_stopped(&error)));
    assert!(written.is_err_and(|error| crate::vfs::stop::is_stopped(&error)));
    std::thread::sleep(Duration::from_millis(50));
    assert!(jobs.try_recv().is_err(), "nothing sent to the plugin");
    let plugin = answering(jobs);
    crate::vfs::stop::stopped_by(stop, || drop(writer));
    line.close();
    assert_eq!(plugin.join().unwrap(), ["File(/a, Abort)"]);
}
