//! Paths put into the temporary panel (FAR's TmpPanel `PutFiles`, `TmpClass.cpp:161-221`): "Sending files
//! to temporary panel" while they go in, each added by its whole path, a folder with its contents as
//! CopyContents says - 0 without, 1 with, 2 asked once for the put at its first folder ("Warning", "Copy
//! folder contents?", Yes / No; Esc is No: FAR's `!Message(...)`). A put asked to stop (Esc on its window,
//! ADR-0019 §5) leaves the list as it was before it: half a folder is no list the user asked for.

use std::path::PathBuf;

use nc_plugin_sdk::{ButtonSet, Error, Message};

use crate::{ITEM, TmpPanel};

/// CopyContents' third state: asked.
const ASK: i64 = 2;

impl TmpPanel {
    /// `sources` added to the list the panel `instance` shows.
    pub(crate) fn put_paths(&mut self, instance: &str, sources: &[PathBuf]) -> Result<(), Error> {
        self.list(instance)?;
        // TmpPanel's `BeginPutFiles`: its message first, kept while the paths go in;
        // `Opt.SelectedCopyContents = Opt.CopyContents` for this put.
        self.host.busy(ITEM, &["Sending files to temporary panel"]);
        let mut contents = match self.options.copy_contents {
            0 => Some(false),
            ASK => None,
            _ => Some(true),
        };
        let host = self.host;
        let before = self.list(instance)?.clone();
        // Esc on the busy window stops a long walk (its window shows no count).
        let mut go_on = |_: usize, _: &std::path::Path| !host.cancelled();
        for source in sources {
            if host.cancelled() {
                *self.list(instance)? = before;
                return Ok(());
            }
            if !source.is_dir() {
                self.list(instance)?.add(source.clone());
                continue;
            }
            // `PutDirectoryContents`: asked at the first folder, the answer kept for the rest.
            let with = *contents.get_or_insert_with(|| ask_contents(host));
            let list = self.list(instance)?;
            if with {
                if !list.add_contents(source, &mut go_on) {
                    *list = before;
                    return Ok(());
                }
            } else {
                list.add(source.clone());
            }
        }
        Ok(())
    }
}

/// FAR's question (`TmpClass.cpp:195-199`, `TmpEng.lng` `MWarning`, `MCopyContensMsg`): Yes only.
fn ask_contents(host: nc_plugin_sdk::Host) -> bool {
    let message = Message::new("Warning", &["Copy folder contents?"]).button_set(ButtonSet::YesNo);
    host.show(&message) == Some(0)
}
