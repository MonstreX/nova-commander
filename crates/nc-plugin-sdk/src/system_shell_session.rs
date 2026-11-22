//! What [`super::Shell`]'s thread holds and does (FAR's emenu `Plugin.cpp`): This PC's items with the
//! file system's first (`SelectDrive`), the context menu of files of one folder or of one of This PC's
//! items (`GetUIObjectOf`, `QueryContextMenu` with ids from 3, `CMF_CANRENAME`, Shift's verbs), its items
//! (a submenu filled as it opens: `WM_INITMENUPOPUP`), a command's verb, a command run (the clipboard
//! flushed after it, as FAR does, so that what it put there outlives the thread).

use std::collections::HashSet;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::ptr::null_mut;

use windows_sys::Win32::System::Ole::OleFlushClipboard;
use windows_sys::Win32::System::SystemServices::SFGAO_FILESYSTEM;
use windows_sys::Win32::UI::Shell::Common::{ITEMIDLIST, STRRET};
use windows_sys::Win32::UI::Shell::{
    CMF_CANRENAME, CMF_EXTENDEDVERBS, CSIDL_DRIVES, GCS_HELPTEXTW, GCS_VERBW, ILFindLastID,
    SHBindToParent, SHCONTF_FOLDERS, SHCONTF_INCLUDEHIDDEN, SHCONTF_NONFOLDERS, SHGDN_NORMAL,
    SHGetDesktopFolder, SHGetSpecialFolderLocation, SHParseDisplayName,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, DestroyMenu, GetMenuItemCount, GetMenuItemInfoW, GetMenuStringW, GetSubMenu,
    HMENU, MENUITEMINFOW, MF_BYPOSITION, MFS_CHECKED, MFS_DISABLED, MFT_MENUBARBREAK,
    MFT_MENUBREAK, MFT_OWNERDRAW, MFT_RADIOCHECK, MFT_SEPARATOR, MIIM_FTYPE, MIIM_ID, MIIM_STATE,
    MIIM_SUBMENU, WM_INITMENUPOPUP,
};

use super::com::{
    Com, ContextMenu, EnumTable, FolderTable, IID_CONTEXT_MENU, IID_SHELL_FOLDER, Pidl, Raw, check,
    strret_text,
};
use super::{ShellItem, Strings};

/// The first command's id (FAR's `MENUID_CMDOFFSET`; below it: cancelled, back, Send To done).
pub(super) const FIRST: u32 = 3;
/// The last command's id FAR gives.
const LAST: u32 = 0x7FFF;

/// A menu of the system's, destroyed when dropped.
#[derive(Debug)]
struct Popup(HMENU);

impl Drop for Popup {
    fn drop(&mut self) {
        // SAFETY: a menu this made, destroyed once with its submenus.
        unsafe { DestroyMenu(self.0) };
    }
}

/// This PC's folder and its items' lists, in the order shown.
#[derive(Debug)]
struct Computer {
    folder: Com,
    items: Vec<Pidl>,
}

/// The context menu open now: the object, its items, the submenus already filled, and what its items'
/// lists point into.
#[derive(Debug)]
struct Open {
    menu: ContextMenu,
    popup: Popup,
    filled: HashSet<usize>,
    _folder: Com,
    _lists: Vec<Pidl>,
}

/// The thread's objects.
#[derive(Debug, Default)]
pub(super) struct Session {
    computer: Option<Computer>,
    open: Option<Open>,
}

fn no(what: &str) -> io::Error {
    io::Error::other(what.to_string())
}

/// `text`'s wide characters, NUL-terminated.
fn wide(text: &std::ffi::OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

/// The display name of `item` in `folder`.
fn display_name(folder: &Com, item: *const ITEMIDLIST) -> Option<String> {
    let mut name = STRRET::default();
    // SAFETY: an `IShellFolder` and one of its items; the answer's place lives through it.
    let result = unsafe {
        (folder.table::<FolderTable>().get_display_name_of)(
            folder.raw(),
            item,
            SHGDN_NORMAL,
            &raw mut name,
        )
    };
    (result >= 0)
        .then(|| strret_text(&mut name, item))
        .flatten()
}

/// Whether `item` of `folder` is of the file system.
fn file_system(folder: &Com, item: *const ITEMIDLIST) -> bool {
    let mut attributes = SFGAO_FILESYSTEM;
    let items = [item];
    // SAFETY: an `IShellFolder`, one of its items, the attributes asked written back.
    let result = unsafe {
        (folder.table::<FolderTable>().get_attributes_of)(
            folder.raw(),
            1,
            items.as_ptr(),
            &raw mut attributes,
        )
    };
    result >= 0 && attributes & SFGAO_FILESYSTEM != 0
}

/// The items of `folder`, each with its name.
fn enumerate(folder: &Com) -> io::Result<Vec<(Pidl, String)>> {
    let mut raw: Raw = null_mut();
    let flags =
        u32::try_from(SHCONTF_FOLDERS | SHCONTF_NONFOLDERS | SHCONTF_INCLUDEHIDDEN).unwrap_or(0);
    // SAFETY: an `IShellFolder`; no window; the enumerator's place lives through it.
    check(unsafe {
        (folder.table::<FolderTable>().enum_objects)(folder.raw(), null_mut(), flags, &raw mut raw)
    })?;
    // `S_FALSE` with no enumerator: nothing in it.
    let Some(items) = Com::take(raw) else {
        return Ok(Vec::new());
    };
    let mut found = Vec::new();
    loop {
        let mut item: *mut ITEMIDLIST = null_mut();
        let mut fetched = 0u32;
        // SAFETY: an `IEnumIDList`; one item asked, its place and the count live through it.
        let result = unsafe {
            (items.table::<EnumTable>().next)(items.raw(), 1, &raw mut item, &raw mut fetched)
        };
        if result != 0 || fetched != 1 || item.is_null() {
            return Ok(found);
        }
        let item = Pidl(item);
        if let Some(name) = display_name(folder, item.0) {
            found.push((item, name));
        }
    }
}

/// The text of the item at `position` of `menu`.
fn menu_text(menu: HMENU, position: u32) -> String {
    // SAFETY: the length asked first (no buffer), then a buffer of it plus the NUL.
    unsafe {
        let length = GetMenuStringW(menu, position, null_mut(), 0, MF_BYPOSITION);
        let Ok(units) = usize::try_from(length) else {
            return String::new();
        };
        let mut text = vec![0u16; units + 1];
        let written = GetMenuStringW(menu, position, text.as_mut_ptr(), length + 1, MF_BYPOSITION);
        text.truncate(usize::try_from(written).unwrap_or(0));
        String::from_utf16_lossy(&text)
    }
}

/// The item at `position` of `menu`, its verb and help from `object`.
fn read_item(
    object: &ContextMenu,
    menu: HMENU,
    position: u32,
    strings: Strings,
) -> io::Result<ShellItem> {
    let mut info = MENUITEMINFOW {
        cbSize: u32::try_from(size_of::<MENUITEMINFOW>()).unwrap_or(0),
        fMask: MIIM_FTYPE | MIIM_ID | MIIM_SUBMENU | MIIM_STATE,
        ..MENUITEMINFOW::default()
    };
    // SAFETY: a live menu; the information's place, of the size it says, lives through it.
    if unsafe { GetMenuItemInfoW(menu, position, 1, &raw mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let separator = info.fType & (MFT_SEPARATOR | MFT_MENUBARBREAK | MFT_MENUBREAK) != 0;
    let owner_drawn = info.fType & MFT_OWNERDRAW != 0;
    let submenu = !info.hSubMenu.is_null();
    let command = !separator && !submenu && info.wID >= FIRST;
    let string = |kind, wanted: bool| {
        (command && (wanted || owner_drawn))
            .then(|| object.command_string(info.wID - FIRST, kind))
            .flatten()
    };
    Ok(ShellItem {
        id: if command { info.wID } else { 0 },
        text: if separator || owner_drawn {
            String::new()
        } else {
            menu_text(menu, position)
        },
        separator,
        submenu,
        checked: info.fState & MFS_CHECKED != 0,
        radio: info.fType & MFT_RADIOCHECK != 0,
        disabled: info.fState & MFS_DISABLED != 0,
        owner_drawn,
        verb: string(GCS_VERBW, strings.verb),
        help: string(GCS_HELPTEXTW, strings.help),
    })
}

impl Session {
    /// This PC's name and its items' names, the file system's first (FAR's `SelectDrive`).
    pub fn computer(&mut self) -> io::Result<(String, Vec<String>)> {
        // A menu made of This PC's items goes before their lists do.
        self.open = None;
        let mut list: *mut ITEMIDLIST = null_mut();
        // SAFETY: no window; the list's place lives through it, freed by its owner.
        check(unsafe {
            SHGetSpecialFolderLocation(
                null_mut(),
                i32::try_from(CSIDL_DRIVES).unwrap_or(0),
                &raw mut list,
            )
        })?;
        let list = Pidl(list);
        let mut desktop: Raw = null_mut();
        // SAFETY: the answer's place lives through it.
        check(unsafe { SHGetDesktopFolder(&raw mut desktop) })?;
        let desktop = Com::take(desktop).ok_or_else(|| no("no desktop"))?;
        let title = display_name(&desktop, list.0).unwrap_or_default();
        let mut folder: Raw = null_mut();
        // SAFETY: the desktop's `IShellFolder`, an item of it, the interface's id and place.
        check(unsafe {
            (desktop.table::<FolderTable>().bind_to_object)(
                desktop.raw(),
                list.0,
                null_mut(),
                &IID_SHELL_FOLDER,
                &raw mut folder,
            )
        })?;
        let folder = Com::take(folder).ok_or_else(|| no("no This PC"))?;
        let (mut first, mut rest) = (Vec::new(), Vec::new());
        for (item, name) in enumerate(&folder)? {
            match file_system(&folder, item.0) {
                true => first.push((item, name)),
                false => rest.push((item, name)),
            }
        }
        first.append(&mut rest);
        let names = first.iter().map(|(_, name)| name.clone()).collect();
        let items = first.into_iter().map(|(item, _)| item).collect();
        self.computer = Some(Computer { folder, items });
        Ok((title, names))
    }

    /// The context menu of `paths` (whole, all in one folder), `extended` with Shift's verbs.
    pub fn for_paths(&mut self, paths: &[PathBuf], extended: bool) -> io::Result<()> {
        self.open = None;
        let mut lists = Vec::new();
        for path in paths {
            let name = wide(path.as_os_str());
            let mut list: *mut ITEMIDLIST = null_mut();
            // SAFETY: a NUL-terminated name; no bind context; the list's place lives through it.
            check(unsafe {
                SHParseDisplayName(name.as_ptr(), null_mut(), &raw mut list, 0, null_mut())
            })?;
            lists.push(Pidl(list));
        }
        let first = lists.first().ok_or_else(|| no("nothing given"))?;
        let mut folder: Raw = null_mut();
        // SAFETY: a whole item list; the folder's interface id and place; the last part not asked.
        check(unsafe { SHBindToParent(first.0, &IID_SHELL_FOLDER, &raw mut folder, null_mut()) })?;
        let folder = Com::take(folder).ok_or_else(|| no("no folder"))?;
        // SAFETY: each list is whole and lives in `lists` as long as the menu does.
        let children = lists
            .iter()
            .map(|list| unsafe { ILFindLastID(list.0) }.cast_const())
            .collect::<Vec<_>>();
        self.open_menu(folder, &children, lists, extended)
    }

    /// The context menu of This PC's item `index` (as [`Session::computer`] gave them).
    pub fn for_computer(&mut self, index: usize, extended: bool) -> io::Result<()> {
        self.open = None;
        let computer = self.computer.as_ref().ok_or_else(|| no("no This PC"))?;
        let item = computer
            .items
            .get(index)
            .ok_or_else(|| no("no such item"))?;
        let (folder, child) = (computer.folder.clone(), item.0.cast_const());
        self.open_menu(folder, &[child], Vec::new(), extended)
    }

    fn open_menu(
        &mut self,
        folder: Com,
        children: &[*const ITEMIDLIST],
        lists: Vec<Pidl>,
        extended: bool,
    ) -> io::Result<()> {
        self.open = None;
        let count = u32::try_from(children.len()).unwrap_or(0);
        let mut raw: Raw = null_mut();
        // SAFETY: an `IShellFolder` and items of it; the interface's id and place live through it.
        check(unsafe {
            (folder.table::<FolderTable>().get_ui_object_of)(
                folder.raw(),
                null_mut(),
                count,
                children.as_ptr(),
                &IID_CONTEXT_MENU,
                null_mut(),
                &raw mut raw,
            )
        })?;
        let menu = ContextMenu::new(Com::take(raw).ok_or_else(|| no("no menu"))?);
        // SAFETY: a new empty menu, owned by `Popup`.
        let popup = Popup(unsafe { CreatePopupMenu() });
        if popup.0.is_null() {
            return Err(io::Error::last_os_error());
        }
        let flags = CMF_CANRENAME | if extended { CMF_EXTENDEDVERBS } else { 0 };
        menu.fill(popup.0, FIRST, LAST, flags)?;
        self.open = Some(Open {
            menu,
            popup,
            filled: HashSet::new(),
            _folder: folder,
            _lists: lists,
        });
        Ok(())
    }

    /// The open menu and its items' menu.
    pub fn open(&self) -> io::Result<(&ContextMenu, HMENU)> {
        let open = self.open.as_ref().ok_or_else(|| no("no menu"))?;
        Ok((&open.menu, open.popup.0))
    }

    /// The items of the submenu at `at` (positions from the top; none: the top), each submenu filled
    /// by its object the first time it opens.
    pub fn items(&mut self, at: &[usize], strings: Strings) -> io::Result<Vec<ShellItem>> {
        let open = self.open.as_mut().ok_or_else(|| no("no menu"))?;
        let mut menu = open.popup.0;
        for &position in at {
            let place = i32::try_from(position).unwrap_or(i32::MAX);
            // SAFETY: a live menu; a position outside it gives null.
            let submenu = unsafe { GetSubMenu(menu, place) };
            if submenu.is_null() {
                return Err(no("no such submenu"));
            }
            if open.filled.insert(submenu as usize) {
                let position = isize::try_from(position).unwrap_or(0);
                open.menu
                    .handle(WM_INITMENUPOPUP, submenu as usize, position);
            }
            menu = submenu;
        }
        // SAFETY: a live menu.
        let count = unsafe { GetMenuItemCount(menu) };
        let count = u32::try_from(count).map_err(|_| io::Error::last_os_error())?;
        (0..count)
            .map(|position| read_item(&open.menu, menu, position, strings))
            .collect()
    }

    /// The verb of the command `id` (`rename`, `delete`, ...); none when it has none.
    pub fn verb(&self, id: u32) -> io::Result<Option<String>> {
        let (menu, _) = self.open()?;
        Ok((id >= FIRST)
            .then(|| menu.command_string(id - FIRST, GCS_VERBW))
            .flatten())
    }

    /// The command `id` run; the clipboard flushed after it (FAR's `OleFlushClipboard`).
    pub fn invoke(&self, id: u32) -> io::Result<()> {
        let (menu, _) = self.open()?;
        if id < FIRST {
            return Err(no("no such command"));
        }
        let ran = menu.invoke(id - FIRST);
        // SAFETY: on the thread that initialized OLE; nothing of it may be on the clipboard (E_FAIL).
        let _ = unsafe { OleFlushClipboard() };
        ran
    }
}
