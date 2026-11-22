//! A plugin's thread (ADR-0016): the plugin started and what it tells, then the jobs the program sends,
//! one at a time, each answered as an event; a configuration (FAR's `ConfigureW`) answered with the
//! plugin's items read again when it says its settings changed (FAR's `ConfigureCurrent`).

use std::path::Path;
use std::sync::mpsc::{Receiver, Sender};

use super::calls::Call;

use super::ffi::{
    HostContext, KeyAnswer, Loaded, PLACE_COMMANDS, PLACE_CONFIG, PLACE_DISKS, PLACE_EDITOR,
    PLACE_EDITOR_KEYS, PLACE_FILE_MASKS, PLACE_FIND_LIST, PLACE_PREFIXES,
};
use super::{Event, Job, PluginInfo, PluginItems};
use crate::vfs::VfsPath;

/// What a plugin tells as it starts.
pub(super) struct Told {
    pub info: PluginInfo,
    pub items: PluginItems,
}

pub(super) type Started = Result<Told, String>;

/// Its items in the program's menus, as it tells them now; asked quietly (FAR's `GetPluginInfo` shows
/// no windows).
fn items(loaded: &Loaded) -> PluginItems {
    PluginItems {
        commands: loaded.quiet_items(PLACE_COMMANDS),
        disks: loaded.quiet_items(PLACE_DISKS),
        configs: loaded.quiet_items(PLACE_CONFIG),
        prefixes: loaded.quiet_items(PLACE_PREFIXES),
        file_masks: loaded.quiet_items(PLACE_FILE_MASKS),
        find_list: !loaded.quiet_items(PLACE_FIND_LIST).is_empty(),
        editor: loaded.quiet_items(PLACE_EDITOR),
        editor_keys: !loaded.quiet_items(PLACE_EDITOR_KEYS).is_empty(),
    }
}

/// The plugin's thread: start, tell, run the jobs; the plugin released when the jobs end.
pub(super) fn run_plugin(
    library: &Path,
    context: HostContext,
    jobs: &Receiver<Call>,
    started: &Sender<Started>,
    events: &Sender<Event>,
) {
    let plugin = context.plugin.clone();
    let loaded = match Loaded::start(library, context) {
        Ok(loaded) => loaded,
        Err(reason) => {
            let _ = started.send(Err(reason));
            return;
        }
    };
    // Its messages from `new`, `info` and its items come while the program waits for this: Esc at once.
    let _ = started.send(Ok(Told {
        info: loaded.info(),
        items: items(&loaded),
    }));
    loaded.listen();
    while let Ok(Call { id, job }) = jobs.recv() {
        // Let go (P6.0e): the jobs sent before are not done (their answers dropped), its panels closed.
        if loaded.let_go() && !matches!(job, Job::Close(_)) {
            continue;
        }
        loaded.begin_call(id);
        let event = run_job(&loaded, &plugin, job);
        loaded.end_call();
        // Let go meanwhile (P6.0e): the program hears nothing more of it, a plugin of its name may be
        // there anew.
        if loaded.let_go() {
            continue;
        }
        if let Some(event) = event {
            let _ = events.send(event);
        }
        if loaded.take_busy() {
            let _ = events.send(Event::Done {
                plugin: plugin.clone(),
            });
        }
    }
}

/// One job on the plugin's thread; what the program hears of it.
fn run_job(loaded: &Loaded, plugin: &str, job: Job) -> Option<Event> {
    loaded.new_job();
    match job {
        Job::Run {
            place,
            index,
            target,
            serial,
        } => ran(loaded, plugin, (place, index), (target, serial)),
        Job::List {
            instance,
            path,
            reply,
        } => {
            let _ = reply.send(loaded.list(&instance, &path));
            None
        }
        Job::Look {
            instance,
            path,
            reply,
        } => {
            let _ = reply.send(loaded.panel_look(&instance, &path));
            None
        }
        Job::Close(instance) => {
            loaded.close_panel(&instance);
            None
        }
        Job::EditorKey { key, serial } => Some(editor_key(loaded, plugin, &key, serial)),
        Job::Synchro(param) => {
            loaded.synchro(param);
            None
        }
        Job::Items { reply } => {
            let _ = reply.send(items(loaded));
            None
        }
        Job::File {
            instance,
            path,
            call,
            reply,
        } => {
            let _ = reply.send(loaded.file(&instance, &path, call));
            None
        }
        job @ (Job::Key { .. }
        | Job::Command { .. }
        | Job::OpenFile { .. }
        | Job::FindList { .. }) => asked(loaded, plugin, job),
        job @ (Job::Get { .. }
        | Job::Delete { .. }
        | Job::MakeFolder { .. }
        | Job::Put { .. }
        | Job::HostFile { .. }) => Some(own_files(loaded, plugin, job)),
        Job::Create {
            files,
            side,
            serial,
        } => Some(created(loaded, plugin, &files, (side, serial))),
        Job::SearchOpen { path, reply } => {
            super::search_open::answer(loaded, &path, &reply);
            None
        }
        Job::DiskFile {
            path,
            purpose,
            folder,
            side,
            serial,
        } => Some(super::disk_files::offered(
            loaded,
            plugin,
            (&path, &purpose, folder.as_deref()),
            (side, serial),
        )),
    }
}

/// The editor's key told to the plugin (FAR's `ProcessEditorInputW`): ran, the keys held meanwhile go on.
fn editor_key(loaded: &Loaded, plugin: &str, key: &str, serial: u64) -> Event {
    match loaded.editor_key(key) {
        Ok(()) => Event::Ran {
            plugin: plugin.to_string(),
            serial,
        },
        Err(error) => Event::Failed {
            plugin: plugin.to_string(),
            error,
            serial: Some(serial),
        },
    }
}

/// Shift+F1 on the plugin's thread (FAR's `PluginPutFilesToNew`): a new panel of its own, the files put
/// into it, the panel closed; none made, the press passed on to the next plugin.
fn created(
    loaded: &Loaded,
    plugin: &str,
    files: &super::ffi::PutFiles,
    (side, serial): (usize, u64),
) -> Event {
    let instance = match loaded.open_new() {
        Ok(Some(instance)) => instance,
        Ok(None) => {
            let passed = KeyAnswer {
                pass: true,
                ..KeyAnswer::default()
            };
            return answered(plugin, (side, serial), Ok(passed));
        }
        Err(error) => {
            return Event::Failed {
                plugin: plugin.to_string(),
                error,
                serial: Some(serial),
            };
        }
    };
    let put = loaded.put(&instance, "/", files);
    loaded.close_panel(&instance);
    match put {
        Ok(done) => Event::Created {
            plugin: plugin.to_string(),
            serial,
            done,
        },
        Err(error) => Event::Failed {
            plugin: plugin.to_string(),
            error,
            serial: Some(serial),
        },
    }
}

/// A panel's files written out, deleted, made or taken in by the plugin itself, or handed to its
/// commands on the panel's file (FAR's `GetFilesW`, `DeleteFilesW`, `MakeDirectoryW`, `PutFilesW`,
/// `ProcessHostFileW`): its end as the program hears it.
fn own_files(loaded: &Loaded, plugin: &str, job: Job) -> Event {
    let plugin = plugin.to_string();
    match job {
        Job::Get {
            instance,
            path,
            files,
            serial,
        } => Event::Got {
            plugin,
            serial,
            result: loaded.get(&instance, &path, &files),
        },
        Job::Delete {
            instance,
            path,
            names,
            serial,
        } => Event::Got {
            plugin,
            serial,
            result: loaded.delete_files(&instance, &path, &names, 0),
        },
        Job::MakeFolder {
            instance,
            path,
            serial,
        } => Event::Made {
            plugin,
            serial,
            result: loaded.make_folder(&instance, &path, ""),
        },
        Job::HostFile {
            instance,
            path,
            names,
            serial,
        } => Event::Processed {
            plugin,
            serial,
            result: loaded.process_host_file(&instance, &path, &names, 0),
        },
        Job::Put {
            instance,
            path,
            files,
        } => match loaded.put(&instance, &path, &files) {
            Ok(done) => Event::Put { plugin, done },
            Err(error) => Event::Failed {
                plugin,
                error,
                serial: None,
            },
        },
        other => Event::Failed {
            plugin,
            error: format!("Not a job on its files: {other:?}"),
            serial: None,
        },
    }
}

/// A press the program waits for the answer to: a key of a panel, a command, a file opened.
fn asked(loaded: &Loaded, plugin: &str, job: Job) -> Option<Event> {
    let (to, answer) = match job {
        Job::Key {
            instance,
            path,
            press,
            side,
            serial,
        } => ((side, serial), loaded.panel_key(&instance, &path, &press)),
        Job::Command {
            prefix,
            command,
            folder,
            side,
            serial,
        } => (
            (side, serial),
            loaded.open_command(&prefix, &command, folder.as_deref()),
        ),
        Job::OpenFile {
            path,
            alternative,
            folder,
            side,
            serial,
        } => (
            (side, serial),
            loaded.open_file(&path, u32::from(alternative), folder.as_deref()),
        ),
        Job::FindList {
            found,
            side,
            serial,
        } => ((side, serial), loaded.find_list(&found)),
        _ => return None,
    };
    Some(answered(plugin, to, answer))
}

/// The answer to the press `serial` on the panel `side` (a key, a command, a file opened), or the call's
/// failure.
fn answered(
    plugin: &str,
    (side, serial): (usize, u64),
    answer: Result<KeyAnswer, String>,
) -> Event {
    // A panel named so it cannot be a path's part fails whatever opened it (review R47).
    let named = match &answer {
        Ok(KeyAnswer {
            go: Some(super::KeyGo::Open(instance)),
            ..
        }) => bad_instance(instance),
        _ => None,
    };
    let answer = named.map_or(answer, Err);
    match answer {
        Ok(answer) => Event::Key {
            plugin: plugin.to_string(),
            side,
            serial,
            answer,
        },
        // Its own wait ends, not every wait of the plugin (R7 review B3: a newer key's was ended, its
        // answer then dropped as not waited for).
        Err(error) => Event::Failed {
            plugin: plugin.to_string(),
            error,
            serial: Some(serial),
        },
    }
}

/// The item `index` of `place` chosen for the panel `target`: a panel it opens, a configuration's end.
fn ran(
    loaded: &Loaded,
    plugin: &str,
    (place, index): (u32, usize),
    (target, serial): (usize, u64),
) -> Option<Event> {
    if place == PLACE_CONFIG {
        return Some(configured(loaded, plugin, index, serial));
    }
    match loaded.run_item(place, index) {
        // The editor's commands open no panel (FAR's `OPEN_EDITOR` returns none).
        Ok(_) if place == PLACE_EDITOR => Some(Event::Ran {
            plugin: plugin.to_string(),
            serial,
        }),
        Ok(None) => Some(Event::Ran {
            plugin: plugin.to_string(),
            serial,
        }),
        Ok(Some(instance)) => Some(opened(plugin, &instance, target, serial)),
        Err(error) => Some(Event::Failed {
            plugin: plugin.to_string(),
            error,
            serial: Some(serial),
        }),
    }
}

/// Why the plugin's panel `instance` cannot be opened: a name with `/` or `\` is not one part of a path.
pub fn bad_instance(instance: &str) -> Option<String> {
    instance.contains(['/', '\\']).then(|| {
        format!("The plugin named its panel \"{instance}\": a name with / or \\ cannot be opened")
    })
}

/// The plugin's panel `instance` to open in the panel `target`, at its root; a name that cannot be a
/// path's part is the command's failure.
pub(super) fn opened(plugin: &str, instance: &str, target: usize, serial: u64) -> Event {
    if let Some(error) = bad_instance(instance) {
        return Event::Failed {
            plugin: plugin.to_string(),
            error,
            serial: Some(serial),
        };
    }
    Event::Panel {
        plugin: plugin.to_string(),
        target,
        path: VfsPath::in_session(plugin, instance, "/"),
        serial,
    }
}

/// The plugin's configuration `index` ended: its items read again when it changed the settings.
/// A failure is the configuration's own (not another call's of the plugin).
fn configured(loaded: &Loaded, plugin: &str, index: usize, serial: u64) -> Event {
    let result =
        (loaded.run_item(PLACE_CONFIG, index)).map(|changed| changed.map(|_| items(loaded)));
    Event::Configured {
        plugin: plugin.to_string(),
        result,
        serial,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_answer_opening_a_panel_named_with_a_slash_fails_as_a_command_does() {
        // Review R47: refused whatever opened it, not only for menu items.
        for name in ["a/b", r"a\b"] {
            let answer = KeyAnswer {
                go: Some(crate::plugin_host::KeyGo::Open(name.to_string())),
                ..KeyAnswer::default()
            };
            let event = answered("probe", (0, 7), Ok(answer));
            assert!(
                matches!(&event, Event::Failed { error, .. } if error.contains("cannot be opened")),
                "{name}: {event:?}"
            );
            // R7 review B3: the failure ends its own wait only.
            assert!(
                matches!(
                    &event,
                    Event::Failed {
                        serial: Some(7),
                        ..
                    }
                ),
                "{event:?}"
            );
        }
        let fine = KeyAnswer {
            go: Some(crate::plugin_host::KeyGo::Open("one".to_string())),
            ..KeyAnswer::default()
        };
        assert!(matches!(
            answered("probe", (0, 7), Ok(fine)),
            Event::Key { .. }
        ));
    }
}
