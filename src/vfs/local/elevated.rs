//! The requests the elevated helper carries out (ADR-0007 §3-4): the fixed set of primitive calls of
//! the local provider, on absolute paths, each answered with success or the OS error. Plain Rust over
//! any byte stream, so it is tested without elevation.
//!
//! Wire format. A frame is a `u32` length (little endian) and that many bytes, at most [`MAX_FRAME`].
//! Its first byte is [`VERSION`], the second the kind. Anything malformed - a frame too long or
//! truncated, another version, an unknown kind, a path that is not absolute, bytes left over - ends
//! the helper: [`serve`] returns `InvalidData` without carrying the request out.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf, Prefix};

use super::ops;
use crate::platform::elevation::{connect_pipe, decode_path, encode_path};
use crate::platform::files::{self, NewLink, Setting};

#[path = "elevated_setting.rs"]
mod setting;
use crate::vfs::Removal;

pub const VERSION: u8 = 1;
/// Paths only: two of the longest a path can be (32,767 UTF-16 units each) fit with room to spare.
pub const MAX_FRAME: usize = 1 << 18;

/// The program's first argument that makes it the helper: `--elevated <pipe> <process id>`.
pub const ELEVATED_ARGUMENT: &str = "--elevated";

const EXIT: u8 = 0;
const CREATE_DIRECTORY: u8 = 1;
const REMOVE_DIRECTORY: u8 = 2;
const DELETE_FILE: u8 = 3;
const MOVE_TO_TRASH: u8 = 4;
const MOVE_FILE: u8 = 5;
const COPY_FILE: u8 = 6;
const SET_READ_ONLY: u8 = 7;
const MAKE_LINK: u8 = 8;
const COPY_LINK: u8 = 9;
const SET: u8 = 10;

const DONE: u8 = 0x80;
const FAILED: u8 = 0x81;
const STOPPED: u8 = 0x82;
const PROGRESS: u8 = 0x83;
const GO_ON: u8 = 0x84;

/// One primitive call (ADR-0007 §1, the rows of its first step).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// One folder; fails if anything has the name, its parent is not created.
    CreateDirectory(PathBuf),
    /// An empty folder or a link to one (`ops::remove`: never a root, never followed).
    RemoveDirectory(PathBuf),
    /// A file or a link to one (`ops::remove`: never a root; a read-only file loses the attribute).
    DeleteFile(PathBuf),
    /// Into the Recycle Bin (`ops::remove`).
    MoveToTrash(PathBuf),
    /// `platform::files::move_file`.
    MoveFile {
        from: PathBuf,
        to: PathBuf,
        replace: bool,
    },
    /// `platform::files::copy_file`: never over an existing file; answered with progress on the way.
    CopyFile { from: PathBuf, to: PathBuf },
    /// `platform::files::set_read_only`.
    SetReadOnly { path: PathBuf, read_only: bool },
    /// `platform::files::make_link`.
    MakeLink {
        kind: NewLink,
        target: PathBuf,
        link: PathBuf,
    },
    /// `platform::files::copy_link`.
    CopyLink {
        source: PathBuf,
        destination: PathBuf,
    },
    /// One change of Ctrl+A's "Set" (`Setting::apply`).
    Set { path: PathBuf, setting: Setting },
}

fn malformed() -> io::Error {
    io::Error::from(io::ErrorKind::InvalidData)
}

fn write_frame(stream: &mut impl Write, kind: u8, fields: &[u8]) -> io::Result<()> {
    let length = 2 + fields.len();
    if length > MAX_FRAME {
        return Err(io::Error::from(io::ErrorKind::InvalidFilename));
    }
    let mut frame = Vec::with_capacity(4 + length);
    frame.extend_from_slice(
        &u32::try_from(length)
            .map_err(|_| malformed())?
            .to_le_bytes(),
    );
    frame.push(VERSION);
    frame.push(kind);
    frame.extend_from_slice(fields);
    stream.write_all(&frame)?;
    stream.flush()
}

/// Reads one frame; `None` when the stream ended before it began.
fn read_frame(stream: &mut impl Read) -> io::Result<Option<(u8, Vec<u8>)>> {
    let mut header = [0u8; 4];
    let mut got = 0;
    while got < header.len() {
        match stream.read(&mut header[got..])? {
            0 if got == 0 => return Ok(None),
            0 => return Err(malformed()),
            read => got += read,
        }
    }
    let length = u32::from_le_bytes(header) as usize;
    if !(2..=MAX_FRAME).contains(&length) {
        return Err(malformed());
    }
    let mut body = vec![0u8; length];
    stream.read_exact(&mut body).map_err(|error| {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            malformed()
        } else {
            error
        }
    })?;
    if body[0] != VERSION {
        return Err(malformed());
    }
    let kind = body[1];
    body.drain(..2);
    Ok(Some((kind, body)))
}

/// The fields of one frame.
struct Fields<'a>(&'a [u8]);

impl Fields<'_> {
    fn bytes(&mut self, count: usize) -> io::Result<&[u8]> {
        if self.0.len() < count {
            return Err(malformed());
        }
        let (head, tail) = self.0.split_at(count);
        let head: &[u8] = head;
        self.0 = tail;
        Ok(head)
    }

    fn byte(&mut self) -> io::Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    fn flag(&mut self) -> io::Result<bool> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(malformed()),
        }
    }

    fn number(&mut self) -> io::Result<u64> {
        let bytes: [u8; 8] = self.bytes(8)?.try_into().map_err(|_| malformed())?;
        Ok(u64::from_le_bytes(bytes))
    }

    fn path(&mut self) -> io::Result<PathBuf> {
        let length: [u8; 4] = self.bytes(4)?.try_into().map_err(|_| malformed())?;
        let bytes = self.bytes(u32::from_le_bytes(length) as usize)?;
        decode_path(bytes)
            .filter(|path| is_plain_absolute(path))
            .ok_or_else(malformed)
    }

    fn finish(&self) -> io::Result<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(malformed())
        }
    }
}

/// An absolute path to a file or folder: `..` would get past the refusal of a drive root, and only
/// a drive, a share or a volume name files: not `\\.\PhysicalDrive0`, `\\?\GLOBALROOT\...`, `\\?\CON`.
fn is_plain_absolute(path: &Path) -> bool {
    path.is_absolute()
        && path.components().all(|component| match component {
            // `\\?\C:\.` is a drive root the refusal of roots would not recognise.
            Component::ParentDir | Component::CurDir => false,
            Component::Prefix(prefix) => match prefix.kind() {
                Prefix::Disk(_)
                | Prefix::UNC(..)
                | Prefix::VerbatimDisk(_)
                | Prefix::VerbatimUNC(..) => true,
                Prefix::Verbatim(name) => name
                    .to_string_lossy()
                    .to_ascii_lowercase()
                    .starts_with("volume{"),
                Prefix::DeviceNS(_) => false,
            },
            _ => true,
        })
}

/// A path the helper would end over is refused here, before it costs the helper.
fn put_path(fields: &mut Vec<u8>, path: &Path) -> io::Result<()> {
    if !is_plain_absolute(path) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the helper takes only full paths of files and folders",
        ));
    }
    let bytes = encode_path(path);
    let length = u32::try_from(bytes.len()).map_err(|_| malformed())?;
    fields.extend_from_slice(&length.to_le_bytes());
    fields.extend_from_slice(&bytes);
    Ok(())
}

fn link_kind_code(kind: NewLink) -> u8 {
    match kind {
        NewLink::Hard => 0,
        NewLink::Junction => 1,
        NewLink::Symbolic => 2,
        NewLink::SymbolicFile => 3,
        NewLink::SymbolicFolder => 4,
    }
}

fn link_kind(code: u8) -> io::Result<NewLink> {
    match code {
        0 => Ok(NewLink::Hard),
        1 => Ok(NewLink::Junction),
        2 => Ok(NewLink::Symbolic),
        3 => Ok(NewLink::SymbolicFile),
        4 => Ok(NewLink::SymbolicFolder),
        _ => Err(malformed()),
    }
}

impl Request {
    /// Its kind and fields; a path too long for a frame fails when it is written.
    fn encode(&self) -> io::Result<(u8, Vec<u8>)> {
        let mut fields = Vec::new();
        let kind = match self {
            Self::CreateDirectory(path) => {
                put_path(&mut fields, path)?;
                CREATE_DIRECTORY
            }
            Self::RemoveDirectory(path) => {
                put_path(&mut fields, path)?;
                REMOVE_DIRECTORY
            }
            Self::DeleteFile(path) => {
                put_path(&mut fields, path)?;
                DELETE_FILE
            }
            Self::MoveToTrash(path) => {
                put_path(&mut fields, path)?;
                MOVE_TO_TRASH
            }
            Self::MoveFile { from, to, replace } => {
                put_path(&mut fields, from)?;
                put_path(&mut fields, to)?;
                fields.push(u8::from(*replace));
                MOVE_FILE
            }
            Self::CopyFile { from, to } => {
                put_path(&mut fields, from)?;
                put_path(&mut fields, to)?;
                COPY_FILE
            }
            Self::SetReadOnly { path, read_only } => {
                put_path(&mut fields, path)?;
                fields.push(u8::from(*read_only));
                SET_READ_ONLY
            }
            Self::MakeLink { kind, target, link } => {
                fields.push(link_kind_code(*kind));
                put_path(&mut fields, target)?;
                put_path(&mut fields, link)?;
                MAKE_LINK
            }
            Self::CopyLink {
                source,
                destination,
            } => {
                put_path(&mut fields, source)?;
                put_path(&mut fields, destination)?;
                COPY_LINK
            }
            Self::Set { path, setting } => {
                put_path(&mut fields, path)?;
                setting::put_setting(&mut fields, setting)?;
                SET
            }
        };
        Ok((kind, fields))
    }

    fn decode(kind: u8, bytes: &[u8]) -> io::Result<Self> {
        let mut fields = Fields(bytes);
        let request = match kind {
            CREATE_DIRECTORY => Self::CreateDirectory(fields.path()?),
            REMOVE_DIRECTORY => Self::RemoveDirectory(fields.path()?),
            DELETE_FILE => Self::DeleteFile(fields.path()?),
            MOVE_TO_TRASH => Self::MoveToTrash(fields.path()?),
            MOVE_FILE => Self::MoveFile {
                from: fields.path()?,
                to: fields.path()?,
                replace: fields.flag()?,
            },
            COPY_FILE => Self::CopyFile {
                from: fields.path()?,
                to: fields.path()?,
            },
            SET_READ_ONLY => Self::SetReadOnly {
                path: fields.path()?,
                read_only: fields.flag()?,
            },
            MAKE_LINK => Self::MakeLink {
                kind: link_kind(fields.byte()?)?,
                target: fields.path()?,
                link: fields.path()?,
            },
            COPY_LINK => Self::CopyLink {
                source: fields.path()?,
                destination: fields.path()?,
            },
            SET => Self::Set {
                path: fields.path()?,
                setting: fields.setting()?,
            },
            _ => return Err(malformed()),
        };
        fields.finish()?;
        Ok(request)
    }
}

/// The local provider's primitive for `request`: the same call the program would make itself.
fn carry_out(request: &Request, progress: &mut dyn FnMut(u64, u64) -> bool) -> io::Result<()> {
    match request {
        Request::CreateDirectory(path) => fs::create_dir(files::exact_path(path)),
        Request::RemoveDirectory(path) => ops::remove(path, Removal::EmptyDir),
        Request::DeleteFile(path) => ops::remove(path, Removal::File),
        Request::MoveToTrash(path) => ops::remove(path, Removal::Trash),
        Request::MoveFile { from, to, replace } => files::move_file(from, to, *replace),
        Request::CopyFile { from, to } => files::copy_file(from, to, progress),
        Request::SetReadOnly { path, read_only } => files::set_read_only(path, *read_only),
        Request::MakeLink { kind, target, link } => files::make_link(*kind, target, link),
        Request::CopyLink {
            source,
            destination,
        } => files::copy_link(source, destination),
        Request::Set { path, setting } => setting.apply(path),
    }
}

/// The helper's loop: reads requests from `stream` and answers each until the client says exit (`Ok`)
/// or the stream ends (`Ok` as well: the program is gone). Malformed input: `InvalidData`.
pub fn serve(stream: &mut (impl Read + Write)) -> io::Result<()> {
    while let Some((kind, fields)) = read_frame(stream)? {
        if kind == EXIT {
            return Fields(&fields).finish();
        }
        let request = Request::decode(kind, &fields)?;
        answer(stream, &request)?;
    }
    Ok(())
}

/// Carries `request` out and answers. What the client answers to progress is read on the way.
fn answer(stream: &mut (impl Read + Write), request: &Request) -> io::Result<()> {
    let mut broken = None;
    let result = carry_out(
        request,
        &mut |done, total| match report_progress(stream, done, total) {
            Ok(go_on) => go_on,
            Err(error) => {
                broken = Some(error);
                false
            }
        },
    );
    if let Some(error) = broken {
        return Err(error);
    }
    match result {
        Ok(()) => write_frame(stream, DONE, &[]),
        Err(error) if error.kind() == io::ErrorKind::Interrupted => {
            write_frame(stream, STOPPED, &[])
        }
        // A refusal of this program's own (a drive root) has no system error: 0.
        Err(error) => write_frame(
            stream,
            FAILED,
            &error.raw_os_error().unwrap_or(0).to_le_bytes(),
        ),
    }
}

fn report_progress(stream: &mut (impl Read + Write), done: u64, total: u64) -> io::Result<bool> {
    let mut fields = Vec::with_capacity(16);
    fields.extend_from_slice(&done.to_le_bytes());
    fields.extend_from_slice(&total.to_le_bytes());
    write_frame(stream, PROGRESS, &fields)?;
    let (kind, bytes) = read_frame(stream)?.ok_or_else(malformed)?;
    if kind != GO_ON {
        return Err(malformed());
    }
    let mut answer = Fields(&bytes);
    let go_on = answer.flag()?;
    answer.finish()?;
    Ok(go_on)
}

/// The program's side of a helper.
#[derive(Debug)]
pub struct Client<S> {
    stream: S,
}

impl<S: Read + Write> Client<S> {
    pub fn new(stream: S) -> Self {
        Self { stream }
    }

    /// Has `request` carried out. A failure comes back with the helper's OS error
    /// (`raw_os_error`). `progress` is told `(done, total)` of a copy; false stops it, which ends
    /// as `ErrorKind::Interrupted` and leaves no partial target.
    pub fn call(
        &mut self,
        request: &Request,
        progress: &mut dyn FnMut(u64, u64) -> bool,
    ) -> io::Result<()> {
        let (kind, fields) = request.encode()?;
        write_frame(&mut self.stream, kind, &fields)?;
        loop {
            let Some((kind, bytes)) = read_frame(&mut self.stream)? else {
                return Err(io::Error::from(io::ErrorKind::BrokenPipe));
            };
            let mut fields = Fields(&bytes);
            match kind {
                DONE => return fields.finish(),
                STOPPED => {
                    fields.finish()?;
                    return Err(io::Error::from(io::ErrorKind::Interrupted));
                }
                FAILED => {
                    let code: [u8; 4] = fields.bytes(4)?.try_into().map_err(|_| malformed())?;
                    fields.finish()?;
                    return Err(helper_error(i32::from_le_bytes(code)));
                }
                PROGRESS => {
                    let (done, total) = (fields.number()?, fields.number()?);
                    fields.finish()?;
                    let go_on = progress(done, total);
                    write_frame(&mut self.stream, GO_ON, &[u8::from(go_on)])?;
                }
                _ => return Err(malformed()),
            }
        }
    }

    /// Tells the helper to end.
    pub fn exit(mut self) -> io::Result<()> {
        write_frame(&mut self.stream, EXIT, &[])
    }
}

fn helper_error(code: i32) -> io::Error {
    if code == 0 {
        io::Error::new(io::ErrorKind::InvalidInput, "refused by the helper")
    } else {
        io::Error::from_raw_os_error(code)
    }
}

/// The helper's whole life: `arguments` are what follows [`ELEVATED_ARGUMENT`]. Returns the process's
/// exit code; nothing but the pipe's first and only client is ever served.
pub fn run_helper(arguments: &[String]) -> i32 {
    let [pipe, process_id] = arguments else {
        return 1;
    };
    let Ok(process_id) = process_id.parse() else {
        return 1;
    };
    match connect_pipe(pipe, process_id).and_then(|mut stream| serve(&mut stream)) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

#[cfg(test)]
#[path = "elevated_tests.rs"]
mod tests;
