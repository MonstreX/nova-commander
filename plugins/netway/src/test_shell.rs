//! The tests' server's commands (ADR-0023 §4): the commands NetWay's SCP and copying on the server send -
//! `ls -la [-d] [-L] [--full-time]`, `cd ... && pwd`, `rm -f`, `rmdir`, `mkdir`, `mv -f`, `cp -p -r -f`,
//! `chmod`, `chown`, `chgrp`, `touch -m -t`, `ln` - done over the server's root folder, and `scp -f` /
//! `scp -t` speaking SCP's records; anything else "not found" (127), as a shell answers.

use std::path::{Path, PathBuf};

use russh::server::Msg;
use russh::{Channel, ChannelMsg};
use russh_sftp::protocol::FileAttributes;

use crate::scp::civil_from_days;
use crate::scp_listing::seconds_of;
use crate::test_server::{attributes, keep_rights, set_times};

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// A command's outcome: its status, output and error output.
type Ran = (u32, String, String);

/// The command's words, its double quotes and their escapes read as a shell reads them.
pub fn words(command: &str) -> Vec<String> {
    let (mut words, mut word) = (Vec::new(), String::new());
    let (mut in_word, mut quoted) = (false, false);
    let mut chars = command.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' if quoted => word.extend(chars.next()),
            '"' => (quoted, in_word) = (!quoted, true),
            ' ' if !quoted => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            other => {
                word.push(other);
                in_word = true;
            }
        }
    }
    if in_word {
        words.push(word);
    }
    words
}

/// `path` under `cwd`, its "." and ".." gone.
pub fn resolve(cwd: &str, path: &str) -> String {
    let joined = match path.starts_with('/') {
        true => path.to_string(),
        false => format!("{}/{path}", cwd.trim_end_matches('/')),
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => drop(parts.pop()),
            part => parts.push(part),
        }
    }
    format!("/{}", parts.join("/"))
}

struct Shell {
    root: PathBuf,
    cwd: String,
}

fn failed(text: String) -> Ran {
    (1, String::new(), format!("{text}\n"))
}

fn done() -> Ran {
    (0, String::new(), String::new())
}

impl Shell {
    fn local(&self, path: &str) -> PathBuf {
        self.root
            .join(resolve(&self.cwd, path).trim_start_matches('/'))
    }

    /// The commands joined by `&&`, each after the one before succeeded.
    fn run(&mut self, words: &[String]) -> Ran {
        let mut ran = done();
        for command in words.split(|word| word == "&&" || word == ";") {
            let command: Vec<&str> = (command.iter())
                .map(String::as_str)
                .skip_while(|word| word.contains('='))
                .collect();
            let (status, out, err) = self.one(&command);
            ran = (status, ran.1 + &out, ran.2 + &err);
            if status != 0 {
                break;
            }
        }
        ran
    }

    fn one(&mut self, command: &[&str]) -> Ran {
        let (name, args) = command
            .split_first()
            .map_or(("", &[][..]), |(n, a)| (*n, a));
        let plain: Vec<&str> = (args.iter().copied())
            .filter(|arg| !arg.starts_with('-'))
            .collect();
        match (name, plain.as_slice()) {
            ("pwd", _) => (0, format!("{}\n", self.cwd), String::new()),
            ("unset", _) => done(),
            ("echo", _) => (0, format!("{}\n", args.join(" ")), String::new()),
            ("cd", [path]) => self.cd(path),
            ("ls", [path]) => self.ls(args, path),
            ("rm", [path]) => self.disk(name, path, |at| match at.exists() {
                true => std::fs::remove_file(at),
                false => Ok(()),
            }),
            ("rmdir", [path]) => self.disk(name, path, |at| std::fs::remove_dir(at)),
            ("mkdir", [path]) => self.disk(name, path, |at| std::fs::create_dir(at)),
            ("mv", [from, to]) => self.two(name, from, to, |a, b| std::fs::rename(a, b)),
            ("cp", [from, to]) => self.two(name, from, to, copy_all),
            ("ln", [from, to]) => self.two(name, from, to, |a, b| std::fs::hard_link(a, b)),
            ("chmod", [mode, path]) => self.chmod(mode, path),
            ("chown" | "chgrp", [_, path]) => self.disk(name, path, |at| at.metadata().map(drop)),
            ("touch", [stamp, path]) => self.touch(stamp, path),
            _ => (127, String::new(), format!("sh: 1: {name}: not found\n")),
        }
    }

    fn cd(&mut self, path: &str) -> Ran {
        if !self.local(path).is_dir() {
            return failed(format!("sh: 1: cd: can't cd to {path}"));
        }
        self.cwd = resolve(&self.cwd, path);
        done()
    }

    fn disk(&self, name: &str, path: &str, act: impl Fn(&Path) -> std::io::Result<()>) -> Ran {
        match act(&self.local(path)) {
            Ok(()) => done(),
            Err(error) => failed(format!("{name}: '{path}': {error}")),
        }
    }

    /// `from` to `to`, into `to` when it is a folder.
    fn two(
        &self,
        name: &str,
        from: &str,
        to: &str,
        act: impl Fn(&Path, &Path) -> std::io::Result<()>,
    ) -> Ran {
        let (source, mut target) = (self.local(from), self.local(to));
        if target.is_dir() {
            target = target.join(source.file_name().unwrap_or_default());
        }
        match act(&source, &target) {
            Ok(()) => done(),
            Err(error) => failed(format!("{name}: '{from}' -> '{to}': {error}")),
        }
    }

    fn chmod(&self, mode: &str, path: &str) -> Ran {
        let at = self.local(path);
        let Ok(rights) = u32::from_str_radix(mode, 8) else {
            return failed(format!("chmod: invalid mode: '{mode}'"));
        };
        match at.exists() && keep_rights(&at, rights) {
            true => done(),
            false => failed(format!(
                "chmod: changing permissions of '{path}': Operation not permitted"
            )),
        }
    }

    /// `touch -m -t CCYYMMDDhhmm.SS` read as UTC.
    fn touch(&self, stamp: &str, path: &str) -> Ran {
        let digits: String = stamp.chars().filter(char::is_ascii_digit).collect();
        let field = |from: usize, to: usize| digits.get(from..to).and_then(|f| f.parse().ok());
        let date = (field(0, 4), field(4, 6), field(6, 8));
        let time = (field(8, 10), field(10, 12), field(12, 14));
        let (Some(year), Some(month), Some(day)) = date else {
            return failed(format!("touch: invalid date format '{stamp}'"));
        };
        let time = (
            time.0.unwrap_or(0),
            time.1.unwrap_or(0),
            time.2.unwrap_or(0),
        );
        let seconds = u32::try_from(seconds_of((year, month, day), time)).ok();
        let attrs = FileAttributes {
            mtime: seconds,
            ..FileAttributes::empty()
        };
        match set_times(&self.local(path), &attrs) {
            Ok(()) => done(),
            Err(_) => failed(format!("touch: cannot touch '{path}'")),
        }
    }

    fn ls(&self, options: &[&str], path: &str) -> Ran {
        let at = self.local(path);
        if !at.exists() {
            let text = format!("ls: cannot access '{path}': No such file or directory");
            return (2, String::new(), format!("{text}\n"));
        }
        let full = options.contains(&"--full-time");
        let itself = options
            .iter()
            .any(|option| option.starts_with("-l") && option.contains('d'))
            || options.contains(&"-d");
        if itself || !at.is_dir() {
            return (0, format!("{}\n", ls_line(&at, path, full)), String::new());
        }
        let mut out = String::from("total 8\n");
        out += &(ls_line(&at, ".", full) + "\n");
        out += &(ls_line(&at, "..", full) + "\n");
        let mut names: Vec<String> = (std::fs::read_dir(&at).into_iter().flatten().flatten())
            .map(|item| item.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        for name in names {
            out += &(ls_line(&at.join(&name), &name, full) + "\n");
        }
        (0, out, String::new())
    }
}

/// `from` copied to `to` with its folders' items, the times kept (`cp -p -r`).
fn copy_all(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for item in std::fs::read_dir(from)? {
            let item = item?;
            copy_all(&item.path(), &to.join(item.file_name()))?;
        }
        return Ok(());
    }
    std::fs::copy(from, to)?;
    let modified = from.metadata()?.modified()?;
    let file = std::fs::OpenOptions::new().write(true).open(to)?;
    file.set_times(std::fs::FileTimes::new().set_modified(modified))
}

/// One line of `ls -la` for the disk's `at` shown as `name`.
fn ls_line(at: &Path, name: &str, full: bool) -> String {
    let attrs = attributes(at).unwrap_or_else(|_| FileAttributes::empty());
    let is_dir = at.is_dir();
    let rights = attrs.permissions.unwrap_or(0o644) & 0o777;
    let kind = if is_dir { 'd' } else { '-' };
    let text = crate::copy_params::rights_text(u16::try_from(rights).unwrap_or(0o644));
    let size = if is_dir {
        4096
    } else {
        attrs.size.unwrap_or(0)
    };
    let seconds = i64::from(attrs.mtime.unwrap_or(0));
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    let of_day = seconds.rem_euclid(86_400);
    let (hour, minute, second) = (of_day / 3600, of_day / 60 % 60, of_day % 60);
    let month_name = MONTHS[usize::try_from(month - 1).unwrap_or(0)];
    let time = match full {
        true => format!(
            "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}.000000000 +0000"
        ),
        false => format!("{month_name} {day:2}  {year}"),
    };
    format!("{kind}{text} 1 user users {size} {time} {name}")
}

/// The channel's bytes as the client sends them.
struct Input {
    channel: Channel<Msg>,
    pending: Vec<u8>,
}

impl Input {
    async fn fill(&mut self) -> bool {
        loop {
            match self.channel.wait().await {
                Some(ChannelMsg::Data { data }) => {
                    self.pending.extend_from_slice(&data);
                    return true;
                }
                Some(ChannelMsg::Eof | ChannelMsg::Close) | None => return false,
                Some(_) => {}
            }
        }
    }

    async fn take(&mut self, count: usize) -> Option<Vec<u8>> {
        while self.pending.len() < count {
            if !self.fill().await {
                return None;
            }
        }
        Some(self.pending.drain(..count).collect())
    }

    async fn line(&mut self) -> Option<String> {
        loop {
            if let Some(at) = self.pending.iter().position(|byte| *byte == b'\n') {
                let line: Vec<u8> = self.pending.drain(..=at).collect();
                return Some(String::from_utf8_lossy(&line[..at]).into_owned());
            }
            if !self.fill().await {
                return None;
            }
        }
    }

    async fn send(&self, data: &[u8]) {
        let _ = self.channel.data_bytes(data.to_vec()).await;
    }
}

/// `scp -f`: the file sent with its record.
async fn scp_from(input: &mut Input, local: &Path, path: &str) -> u32 {
    input.take(1).await;
    let Ok(data) = std::fs::read(local) else {
        let text = format!("\u{1}scp: {path}: No such file or directory\n");
        input.send(text.as_bytes()).await;
        return 1;
    };
    let rights = attributes(local).ok().and_then(|attrs| attrs.permissions);
    let name = local.file_name().unwrap_or_default().to_string_lossy();
    let record = format!(
        "C{:04o} {} {name}\n",
        rights.unwrap_or(0o644) & 0o7777,
        data.len()
    );
    input.send(record.as_bytes()).await;
    input.take(1).await;
    for piece in data.chunks(32 * 1024) {
        input.send(piece).await;
    }
    input.send(b"\0").await;
    input.take(1).await;
    0
}

/// `scp -t`: a file received by its record (a name with "locked" refused, as the server's rights).
async fn scp_to(input: &mut Input, local: &Path) -> u32 {
    input.send(b"\0").await;
    let Some(record) = input.line().await else {
        return 1;
    };
    let mut fields = record.trim_start_matches('C').splitn(3, ' ');
    let rights = fields
        .next()
        .and_then(|mode| u32::from_str_radix(mode, 8).ok());
    let size = fields.next().and_then(|size| size.parse::<usize>().ok());
    let target = match local.is_dir() {
        true => local.join(fields.next().unwrap_or_default()),
        false => local.to_path_buf(),
    };
    if target.to_string_lossy().contains("locked") {
        input.send(b"\x01scp: Permission denied\n").await;
        return 1;
    }
    input.send(b"\0").await;
    let data = input.take(size.unwrap_or(0)).await.unwrap_or_default();
    input.take(1).await;
    if std::fs::write(&target, data).is_err() {
        input.send(b"\x01scp: cannot write\n").await;
        return 1;
    }
    keep_rights(&target, rights.unwrap_or(0o644));
    input.send(b"\0").await;
    0
}

/// `command` run on `channel` over `root`, its outputs and status sent, the channel closed.
pub async fn serve(root: PathBuf, command: String, channel: Channel<Msg>) {
    let mut words = words(&command);
    // A shell named by the session: `shell -c "command"`.
    if words.len() == 3 && words[1] == "-c" {
        words = self::words(&words[2]);
    }
    let mut shell = Shell {
        root,
        cwd: "/".to_string(),
    };
    let mut input = Input {
        channel,
        pending: Vec::new(),
    };
    let status = match words
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["scp", .., "-f", path] => scp_from(&mut input, &shell.local(path), path).await,
        ["scp", .., "-t", path] => scp_to(&mut input, &shell.local(path)).await,
        _ => {
            let (status, out, err) = shell.run(&words);
            if !out.is_empty() {
                let _ = input.channel.data_bytes(out.into_bytes()).await;
            }
            if !err.is_empty() {
                let _ = input.channel.extended_data_bytes(1, err.into_bytes()).await;
            }
            status
        }
    };
    let _ = input.channel.exit_status(status).await;
    let _ = input.channel.eof().await;
    let _ = input.channel.close().await;
}

#[cfg(test)]
#[path = "test_shell_tests.rs"]
mod tests;
