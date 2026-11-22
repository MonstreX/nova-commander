//! `VfsPath` (ADR-0013 §1): a local path stays an OS path; a path inside a provider is text, `/`-separated,
//! absolute and normalized. Everything here is pure: no call touches a disk or a provider.

use std::fmt;
use std::path::{Component, Path, PathBuf};

use crate::platform::files::paths;

/// Where a panel is: a local folder, or a folder inside a provider.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VfsPath {
    /// `C:\work`, `\\server\share\dir`, `/home/user`.
    Local(PathBuf),
    /// `sftp://user@host/home`, `zip:D:\a.zip!/dir`.
    Mounted {
        provider: String,
        base: Base,
        /// Always starts with `/`; no `.`, `..` or empty parts.
        path: String,
    },
}

/// What a mounted provider works in.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Base {
    /// A connection, as `user@host:22`.
    Session(String),
    /// A container file, read through its own path (an archive inside an archive or on a server).
    File(Box<VfsPath>),
}

impl VfsPath {
    pub fn local(path: impl Into<PathBuf>) -> Self {
        Self::Local(path.into())
    }

    /// The OS path the terminal, the command line and the OS calls need; `None` inside a provider.
    pub fn as_local(&self) -> Option<&Path> {
        match self {
            Self::Local(path) => Some(path),
            Self::Mounted { .. } => None,
        }
    }

    /// The folder above; `None` at a root (a drive, a share, a provider's `/`).
    pub fn parent(&self) -> Option<Self> {
        match self {
            Self::Local(path) => path.parent().map(Self::local),
            Self::Mounted { path, .. } if path == "/" => None,
            Self::Mounted {
                provider,
                base,
                path,
            } => {
                let parent = path.rsplit_once('/').map_or("", |(parent, _)| parent);
                Some(mounted(provider, base, normalize(parent)))
            }
        }
    }

    /// Whether both name one folder or file: local paths as the system compares them (case aside on
    /// Windows, a separator at the end aside), a provider's as it gave them (`sftp` tells case).
    pub fn same_place(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Local(a), Self::Local(b)) => crate::platform::files::same_place(a, b),
            _ => self == other,
        }
    }

    /// Whether both are in one place: the local disks, or one provider's base (a plugin's panel instance).
    pub fn same_mount(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Local(_), Self::Local(_)) => true,
            (
                Self::Mounted { provider, base, .. },
                Self::Mounted {
                    provider: other_provider,
                    base: other_base,
                    ..
                },
            ) => provider == other_provider && base == other_base,
            _ => false,
        }
    }

    /// The root of this path's drive, share or provider (Ctrl+\).
    pub fn root(&self) -> Self {
        let mut root = self.clone();
        while let Some(parent) = root.parent() {
            root = parent;
        }
        root
    }

    /// The name of the last part; `None` at a root.
    pub fn file_name(&self) -> Option<String> {
        match self {
            Self::Local(path) => path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            Self::Mounted { path, .. } => path
                .rsplit('/')
                .next()
                .filter(|name| !name.is_empty())
                .map(str::to_string),
        }
    }

    /// Whether `name` can be one name in a folder here: what a provider's listing says must not lead
    /// anywhere else.
    pub fn accepts_name(&self, name: &str) -> bool {
        match self {
            Self::Local(_) => crate::platform::files::is_plain_name(name),
            Self::Mounted { .. } => !matches!(name, "" | "." | "..") && !name.contains('/'),
        }
    }

    /// The entry `name` in this folder; the name is taken as it is (a `\` in it is part of the name).
    pub fn join(&self, name: &str) -> Self {
        match self {
            Self::Local(path) => Self::Local(path.join(name)),
            Self::Mounted {
                provider,
                base,
                path,
            } => {
                let separator = if path.ends_with('/') { "" } else { "/" };
                mounted(provider, base, format!("{path}{separator}{name}"))
            }
        }
    }

    /// A path in a session of a provider (a plugin's panel instance): `provider://session/path`.
    pub fn in_session(provider: &str, session: &str, path: &str) -> Self {
        Self::Mounted {
            provider: provider.to_ascii_lowercase(),
            base: Base::Session(session.to_string()),
            path: normalize(path),
        }
    }

    /// The session and the `/` path of a path in a provider's session.
    pub fn session_path(&self) -> Option<(&str, &str)> {
        match self {
            Self::Mounted {
                base: Base::Session(session),
                path,
                ..
            } => Some((session, path)),
            _ => None,
        }
    }

    /// A path inside the container file `container` as the provider `provider` reads it:
    /// `provider:container!/path` (Alt+F7's found objects in an archive, FAR's `ArcListItem`).
    pub fn in_file(provider: &str, container: Self, path: &str) -> Self {
        Self::Mounted {
            provider: provider.to_ascii_lowercase(),
            base: Base::File(Box::new(container)),
            path: normalize(path),
        }
    }

    /// The container file and the `/` path inside it of a path inside a container file.
    pub fn file_path(&self) -> Option<(&Self, &str)> {
        match self {
            Self::Mounted {
                base: Base::File(container),
                path,
                ..
            } => Some((container, path)),
            _ => None,
        }
    }

    /// The path as Find file shows it: inside a container file FAR's `D:\a.zip:\dir` (`AddMenuRecord`'s
    /// archive's name, `:`, the path inside with `\`), any other as it is written.
    pub fn shown(&self) -> String {
        match self.file_path() {
            Some((container, inner)) => format!("{container}:{}", inner.replace('/', "\\")),
            None => self.to_string(),
        }
    }

    /// The provider of a mounted path.
    pub fn provider(&self) -> Option<&str> {
        match self {
            Self::Mounted { provider, .. } => Some(provider),
            Self::Local(_) => None,
        }
    }

    /// A path as the user or the settings file writes it: `sftp://user@host/dir`, `zip:D:\a.zip!/dir`,
    /// or else a local path, taken as it is (spaces at the ends are part of a name).
    pub fn parse(text: &str) -> Self {
        if let Some((scheme, rest)) = text.split_once("://")
            && is_scheme(scheme)
        {
            let (session, path) = rest.find('/').map_or((rest, "/"), |at| rest.split_at(at));
            return Self::Mounted {
                provider: scheme.to_ascii_lowercase(),
                base: Base::Session(session.to_string()),
                path: normalize(path),
            };
        }
        if let Some((scheme, rest)) = text.split_once(':')
            && is_scheme(scheme)
            && let Some((container, inner)) = rest.rsplit_once("!/")
        {
            return Self::Mounted {
                provider: scheme.to_ascii_lowercase(),
                base: Base::File(Box::new(Self::parse(container))),
                path: normalize(inner),
            };
        }
        Self::local(text)
    }

    /// What the user typed (a `cd`, a copy target), taken from this folder, lexically: `..` and `.` are
    /// folded; absolute paths, drive letters (`D:`) and provider paths replace this folder.
    pub fn resolve(&self, input: &str) -> Self {
        self.resolve_exact(input.trim().trim_matches('"'))
    }

    /// A provider's own folder as the provider `giver` gives it (a plugin's `FCTL_SETPANELDIRECTORY` on
    /// its own panel): an absolute path of this provider, `/` its only separator - its names may hold `\`
    /// (the Network plugin's `\\SERVER`), `..` and `.` folded; anything else, or another provider's
    /// panel, as [`resolve`](Self::resolve) takes it.
    pub fn resolve_given(&self, input: &str, giver: &str) -> Self {
        match self {
            Self::Mounted { provider, base, .. }
                if input.starts_with('/') && provider.eq_ignore_ascii_case(giver) =>
            {
                let mut names: Vec<&str> = Vec::new();
                for name in input.split('/') {
                    match name {
                        "" | "." => {}
                        ".." => {
                            names.pop();
                        }
                        name => names.push(name),
                    }
                }
                mounted(provider, base, format!("/{}", names.join("/")))
            }
            _ => self.resolve(input),
        }
    }

    /// [`resolve`](Self::resolve) of a name already freed of quotes and spaces around it: spaces that
    /// are left belong to it (`a ` is not `a`).
    pub fn resolve_exact(&self, input: &str) -> Self {
        let parsed = Self::parse(input);
        if parsed.as_local().is_none() {
            return parsed;
        }
        match self {
            Self::Local(base) => Self::Local(resolve_local(base, input)),
            Self::Mounted { .. } if has_os_prefix(input) => {
                Self::Local(resolve_local(Path::new(""), input))
            }
            Self::Mounted {
                provider,
                base,
                path,
            } => {
                // Typed input: `\` counts as `/`.
                let input = input.replace('\\', "/");
                let inner = if input.starts_with('/') {
                    normalize(&input)
                } else {
                    normalize(&format!("{path}/{input}"))
                };
                mounted(provider, base, inner)
            }
        }
    }
}

/// A path inside `provider` and `base`.
fn mounted(provider: &str, base: &Base, path: String) -> VfsPath {
    VfsPath::Mounted {
        provider: provider.to_string(),
        base: base.clone(),
        path,
    }
}

/// No path at all: what a progress shows before its first file.
impl Default for VfsPath {
    fn default() -> Self {
        Self::Local(PathBuf::new())
    }
}

impl fmt::Display for VfsPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local(path) => write!(formatter, "{}", path.display()),
            Self::Mounted {
                provider,
                base: Base::Session(session),
                path,
            } => write!(formatter, "{provider}://{session}{path}"),
            Self::Mounted {
                provider,
                base: Base::File(container),
                path,
            } => write!(formatter, "{provider}:{container}!{path}"),
        }
    }
}

/// Tests: a local `VfsPath` equals its OS path.
#[cfg(test)]
impl PartialEq<PathBuf> for VfsPath {
    fn eq(&self, other: &PathBuf) -> bool {
        self.as_local() == Some(other.as_path())
    }
}

#[cfg(test)]
impl PartialEq<&Path> for VfsPath {
    fn eq(&self, other: &&Path) -> bool {
        self.as_local() == Some(*other)
    }
}

/// A provider's name: two characters at least, so `C:` stays a drive.
fn is_scheme(text: &str) -> bool {
    text.len() >= 2
        && text.starts_with(|c: char| c.is_ascii_alphabetic())
        && text
            .chars()
            // `_`: a plugin's name may have one (its panels' paths, ADR-0016 §13).
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.' | '_'))
}

/// `C:`, `C:\x`, `\\server\share`: a path of the OS even on a provider's panel (as FAR's `cd`).
fn has_os_prefix(input: &str) -> bool {
    matches!(
        Path::new(input).components().next(),
        Some(Component::Prefix(_))
    )
}

/// `/a/./b/../c//d` -> `/a/c/d`; `\` counts as `/` (typed input and parsed text only).
fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    format!("/{}", parts.join("/"))
}

fn resolve_local(base: &Path, input: &str) -> PathBuf {
    let joined = paths::bare_drive_root(input).unwrap_or_else(|| base.join(input));
    let mut result = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(text: &str) -> VfsPath {
        VfsPath::local(text)
    }

    #[test]
    fn resolves_relative_and_parent_components() {
        let base = local("C:\\work\\project");
        assert_eq!(base.resolve("src"), local("C:\\work\\project\\src"));
        assert_eq!(base.resolve("src "), local("C:\\work\\project\\src"));
        assert_eq!(base.resolve_exact("src "), local("C:\\work\\project\\src "));
        assert_eq!(base.resolve(".."), local("C:\\work"));
        assert_eq!(base.resolve("..\\..\\.."), local("C:\\"));
        assert_eq!(base.resolve(".\\a\\..\\b"), local("C:\\work\\project\\b"));
    }

    #[test]
    fn absolute_paths_and_drives_replace_the_base() {
        let base = local("C:\\work");
        assert_eq!(base.resolve("D:\\data"), local("D:\\data"));
        assert_eq!(base.resolve("d:"), local("d:\\"));
        assert_eq!(base.resolve("\\"), local("C:\\"));
    }

    #[test]
    fn strips_quotes_and_spaces() {
        let base = local("C:\\work");
        assert_eq!(
            base.resolve("  \"My Files\"  "),
            local("C:\\work\\My Files")
        );
    }

    #[test]
    fn provider_paths_parse_and_show_as_written() {
        for text in [
            "sftp://user@host:22/home/user",
            "sftp://host/",
            "zip:D:\\a.zip!/dir/sub",
            "zip:sftp://user@host/a.zip!/dir",
            "zip:zip:D:\\a.zip!/b.zip!/",
        ] {
            let path = VfsPath::parse(text);
            assert!(path.as_local().is_none(), "{text}");
            assert_eq!(path.to_string(), text);
        }
        assert_eq!(
            VfsPath::parse("SFTP://host"),
            VfsPath::Mounted {
                provider: "sftp".to_string(),
                base: Base::Session("host".to_string()),
                path: "/".to_string(),
            }
        );
        // A drive letter, a share, a stream name, a `!` and end spaces in a local name stay local.
        for text in [
            "C:\\x",
            "d:",
            "\\\\server\\share\\dir",
            "notes.txt:stream",
            "C:\\a!\\b",
            "C:\\name ",
        ] {
            assert_eq!(VfsPath::parse(text), local(text), "{text}");
        }
    }

    #[test]
    fn a_plugins_panel_path_goes_there_and_back_through_its_text() {
        let made = VfsPath::in_session("Tmp_Panel", "one", "/a/../b");
        assert_eq!(made.to_string(), "tmp_panel://one/b");
        assert_eq!(
            VfsPath::parse(&made.to_string()),
            made,
            "typed, saved, in the history"
        );
        assert_eq!(made.session_path(), Some(("one", "/b")));
        assert_eq!(made.provider(), Some("tmp_panel"));
        assert_eq!(VfsPath::local("C:\\").session_path(), None);
        assert_eq!(VfsPath::local("C:\\").provider(), None);
        let archive = VfsPath::parse(r"zip:D:\a.zip!/dir");
        assert_eq!(archive.session_path(), None, "a container is no session");
        assert_eq!(archive.provider(), Some("zip"));
        let root = VfsPath::in_session("p", "one", "/");
        assert!(root.same_mount(&VfsPath::in_session("P", "one", "/deep")));
        assert!(!root.same_mount(&VfsPath::in_session("p", "two", "/")));
        assert!(!root.same_mount(&VfsPath::in_session("q", "one", "/")));
        assert!(!root.same_mount(&VfsPath::local("C:\\")));
        assert!(VfsPath::local(r"C:\").same_mount(&VfsPath::local(r"D:\x")));
    }

    #[test]
    fn a_providers_paths_are_one_place_only_as_it_spelt_them() {
        let docs = VfsPath::in_session("p", "one", "/Docs");
        assert!(docs.same_place(&VfsPath::in_session("p", "one", "/Docs")));
        assert!(!docs.same_place(&VfsPath::in_session("p", "one", "/docs")));
        assert!(!docs.same_place(&VfsPath::in_session("p", "two", "/Docs")));
        assert!(!docs.same_place(&local(r"C:\Docs")));
        let differ = crate::platform::files::NAMES_DIFFER_BY_CASE;
        assert_eq!(local(r"C:\Work\").same_place(&local(r"c:\work")), !differ);
    }

    #[test]
    fn provider_paths_move_inside_their_provider() {
        let home = VfsPath::parse("sftp://user@host/home/user");
        assert_eq!(home.parent().unwrap().to_string(), "sftp://user@host/home");
        assert_eq!(home.file_name().as_deref(), Some("user"));
        assert_eq!(
            home.join("docs").to_string(),
            "sftp://user@host/home/user/docs"
        );
        assert_eq!(
            home.join("a\\b").to_string(),
            "sftp://user@host/home/user/a\\b",
            "a name is joined as it is"
        );
        let root = VfsPath::parse("sftp://user@host/");
        assert_eq!(root.parent(), None);
        assert_eq!(root.file_name(), None);
        assert_eq!(root.join("x").to_string(), "sftp://user@host/x");
        let archive = VfsPath::parse("zip:D:\\a.zip!/dir");
        assert_eq!(archive.parent().unwrap().to_string(), "zip:D:\\a.zip!/");
    }

    #[test]
    fn typed_paths_resolve_inside_the_provider_or_leave_it_for_the_os() {
        let home = VfsPath::parse("sftp://user@host/home/user");
        assert_eq!(
            home.resolve("../x/./y").to_string(),
            "sftp://user@host/home/x/y"
        );
        assert_eq!(home.resolve("/etc").to_string(), "sftp://user@host/etc");
        assert_eq!(
            home.resolve("a\\b").to_string(),
            "sftp://user@host/home/user/a/b"
        );
        let root = VfsPath::parse("sftp://user@host/");
        assert_eq!(root.resolve("..").to_string(), "sftp://user@host/");
        assert_eq!(home.resolve("C:\\data"), local("C:\\data"));
        assert_eq!(
            home.resolve("\\\\server\\share\\dir"),
            local("\\\\server\\share\\dir")
        );
        assert_eq!(home.resolve("d:"), local("d:\\"));
    }

    /// A plugin's own folder given by the plugin: `\` in its names kept (typed, it parts names), `..` and
    /// `.` folded; another provider's panel, a disk's path or a relative one taken as typed.
    #[test]
    fn a_folder_the_provider_gives_keeps_its_backslashes() {
        let top = VfsPath::in_session("network", "1", "/");
        let server = top.resolve_given(r"/\\WSL.LOCALHOST", "network");
        assert_eq!(
            server,
            VfsPath::in_session("network", "1", "/").join(r"\\WSL.LOCALHOST")
        );
        assert_eq!(server.parent(), Some(top.clone()));
        assert_eq!(
            top.resolve_given("/a//b/./", "Network"),
            top.join("a").join("b")
        );
        assert_eq!(top.resolve_given("/a/../b/..", "network"), top);
        assert_eq!(top.resolve(r"/\\WSL.LOCALHOST"), top.join("WSL.LOCALHOST"));
        assert_eq!(
            top.resolve_given(r"/a\b", "commands"),
            top.join("a").join("b"),
            "another plugin's text is typed text"
        );
        assert_eq!(
            top.resolve_given(r"\\server\share", "network"),
            local(r"\\server\share")
        );
        assert_eq!(
            local("C:\\work").resolve_given("/x", "network"),
            local("C:\\x")
        );
    }

    #[test]
    fn a_path_inside_a_file_tells_the_file_and_its_place() {
        let inside = VfsPath::in_file("Archive", local(r"D:\a.zip"), "/dir/");
        assert_eq!(inside.file_path(), Some((&local(r"D:\a.zip"), "/dir")));
        assert_eq!(inside.provider(), Some("archive"));
        assert_eq!(inside, VfsPath::parse(r"archive:D:\a.zip!/dir"));
        let file = inside.join("x.txt");
        assert_eq!(file.file_path().map(|(_, path)| path), Some("/dir/x.txt"));
        assert_eq!(file.parent(), Some(inside.clone()));
        assert_eq!(inside.session_path(), None);
        assert_eq!(local("C:\\").file_path(), None);
        assert_eq!(VfsPath::in_session("archive", "1", "/").file_path(), None);
        assert_eq!(inside.shown(), r"D:\a.zip:\dir");
        assert_eq!(
            file.parent().map(|dir| dir.root().shown()),
            Some(r"D:\a.zip:\".into())
        );
        assert_eq!(local(r"C:\x").shown(), r"C:\x");
    }
}
