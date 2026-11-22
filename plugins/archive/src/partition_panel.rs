//! A disk's partition entered from its panel (arclite's `ProcessPanelInputW` with Enter or Ctrl+PgDn,
//! `set_partition`): the partition under the cursor at the disk's root opened by its file system on a
//! panel over the disk's, which ".." leaves back to it (arclite's `panel_go_to_part`); any other entry
//! left to the program.

use nc_plugin_sdk::{Error, ErrorKind, KeyAnswer, KeyGo, KeyPress};

use crate::opened::Opened;
use crate::{ArchivePlugin, open_asking::OpenAsks};

/// The keys a disk's panel takes (arclite's Enter and Ctrl+PgDn).
pub const KEYS: [&str; 2] = ["Enter", "Ctrl+PgDn"];

impl ArchivePlugin {
    /// Enter or Ctrl+PgDn on `instance`'s folder `path`: the partition under the cursor on a panel of its
    /// own; else the key is the program's.
    pub(crate) fn enter_partition(
        &mut self,
        instance: &str,
        path: &str,
        press: &KeyPress,
    ) -> Result<KeyAnswer, Error> {
        let Some(name) = press.current.as_deref() else {
            return Ok(KeyAnswer::pass());
        };
        let opened = self.archive(instance)?;
        let item = (opened.tree.find(&format!("/{name}")))
            .filter(|&node| path.trim_matches('/').is_empty() && !opened.tree.nodes[node].is_dir)
            .and_then(|node| opened.tree.nodes[node].item);
        let item = item
            .filter(|&item| (opened.items.get(item)).is_some_and(crate::partition::is_partition));
        let (Some(item), Some(_)) = (item, opened.part_mode) else {
            return Ok(KeyAnswer::pass());
        };
        let Some(partition) = self.partition_of(instance, item, name)? else {
            return Ok(KeyAnswer::pass());
        };
        self.opened += 1;
        let made = self.opened.to_string();
        self.panels.insert(made.clone(), partition);
        Ok(KeyAnswer::go(KeyGo::Open(made)))
    }

    /// arclite's `set_partition` of `item` (named `name`) of `instance`'s disk; none when it does not
    /// open.
    fn partition_of(
        &self,
        instance: &str,
        item: usize,
        name: &str,
    ) -> Result<Option<Opened>, Error> {
        let opened = self.archive(instance)?;
        let Some((chain, _)) = opened.library() else {
            return Ok(None);
        };
        let library = self.library();
        let formats = library.formats();
        let index = u32::try_from(item).unwrap_or(u32::MAX);
        let mut asker = OpenAsks::new(self.host, &opened.path, opened.password.clone());
        let failed = |error: std::io::Error| Error::new(ErrorKind::Other, error.to_string());
        let Some(inner) =
            crate::partition::open(library, chain, (index, name), &mut asker).map_err(failed)?
        else {
            return Ok(None);
        };
        let format = inner.name(&formats);
        let default = crate::library_items::default_name(name, None);
        let mut partition =
            Opened::from_library(&opened.path, inner, (format, &default), asker.password())
                .map_err(failed)?;
        partition.partition = Some(index);
        Ok(Some(partition))
    }
}
