//! SCP's file system (WinSCP's `TSCPFileSystem`): folders listed by `ls -la` (with `--full-time` when the
//! server's `ls` has it, as WinSCP's `ScpLsFullTime` auto), an item by `ls -la -d`, items removed, moved,
//! linked, copied and given rights by WinSCP's commands, files received by `scp -f` and sent by `scp -t`
//! (their `C` records with the rights and the size, a `\0` acknowledging each step).
//!
//! WinSCP runs the commands in one shell, its output told apart by markers; here each command is an exec
//! channel of its own: no markers to look for, no shell's prompt or startup text to pass by.

use std::sync::atomic::{AtomicU8, Ordering};

use russh::ChannelMsg;
use russh::client::Msg;
use russh_sftp::protocol::FileAttributes;

use crate::scp_listing::{Listed, listing};
use crate::session_options::{LISTING, Options};
use crate::sftp::{RemoteEntry, child};
use crate::shell::{Commands, quoted};

/// `--full-time` not tried yet, read, refused.
const UNKNOWN: u8 = 0;
const FULL_TIME: u8 = 1;
const PLAIN: u8 = 2;

/// WinSCP's `MSG_SCP_EMPTY_LINE`, `MSG_SCP_INVALID_CONTROL_RECORD`.
const EMPTY_LINE: &str = "Unexpected empty line from server.";

/// The SCP side of a session.
pub struct Scp {
    commands: Commands,
    listing: AtomicU8,
    /// The session's listing command (`ls -la`).
    listing_command: String,
    ignore_warnings: bool,
    national: bool,
    /// The session's time zone offset in seconds.
    shift: i64,
}

/// WinSCP's `NationalVars`.
const NATIONAL_VARS: [&str; 12] = [
    "LANG",
    "LANGUAGE",
    "LC_CTYPE",
    "LC_COLLATE",
    "LC_MONETARY",
    "LC_NUMERIC",
    "LC_TIME",
    "LC_MESSAGES",
    "LC_ALL",
    "HUMAN_BLOCKS",
    "BLOCK_SIZE",
    "LS_BLOCK_SIZE",
];

/// A listing's times read without their zone taken as the server's wall clock in the client's zone,
/// shifted by the session's time zone offset (`shift` seconds; WinSCP's `ShiftTimeInSeconds`).
pub fn zone_fixed(listed: &mut [Listed], shift: i64) {
    for one in listed.iter_mut().filter(|one| !one.zoned) {
        one.entry.modified = (one.entry.modified)
            .map(|seconds| local_to_utc(i64::from(seconds)) + shift)
            .and_then(|seconds| u32::try_from(seconds).ok());
    }
}

/// Today's date the listings' times without a year are read against.
pub fn today_utc() -> (i64, i64, i64) {
    today()
}

/// Seconds of a wall clock read as UTC turned into the true UTC of that wall clock in the client's zone.
pub fn local_to_utc(seconds: i64) -> i64 {
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    let of_day = seconds.rem_euclid(86_400);
    let part = |value: i64| u16::try_from(value).unwrap_or(0);
    let date = [part(year), part(month), part(day)];
    let time = [
        part(of_day / 3600),
        part(of_day / 60 % 60),
        part(of_day % 60),
    ];
    match nc_plugin_sdk::system::dos_time_ticks(date, time) {
        Some(ticks) => ticks / 10_000_000 - 11_644_473_600,
        None => seconds,
    }
}

/// Today's date (UTC) the listing's times without a year are read against.
fn today() -> (i64, i64, i64) {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64);
    civil_from_days(seconds.div_euclid(86_400))
}

/// The date of a day since 1970.
pub fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let of_era = z - era * 146_097;
    let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * of_year + 2) / 153;
    let day = of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// An entry's attributes as SFTP's.
pub fn attributes(entry: &RemoteEntry) -> FileAttributes {
    FileAttributes {
        size: Some(entry.size),
        user: entry.owner.clone(),
        group: entry.group.clone(),
        // No rights told: the kind kept all the same (a folder is still a folder).
        permissions: Some(match (entry.permissions, entry.is_dir) {
            (Some(rights), true) => (rights & 0o7777) | 0o040_000,
            (Some(rights), false) => rights,
            (None, true) => 0o040_000,
            (None, false) if entry.is_link => 0o120_000,
            (None, false) => 0o100_000,
        }),
        mtime: entry.modified,
        atime: entry.modified,
        ..FileAttributes::empty()
    }
}

/// `seconds` since 1970 as `touch -t`'s `CCYYMMDDhhmm.SS` (UTC).
pub fn touch_time(seconds: u32) -> String {
    let seconds = i64::from(seconds);
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    let of_day = seconds.rem_euclid(86_400);
    let (hour, minute, second) = (of_day / 3600, of_day / 60 % 60, of_day % 60);
    format!("{year:04}{month:02}{day:02}{hour:02}{minute:02}.{second:02}")
}

/// The commands setting `attributes` on `path` (WinSCP's `chmod`, `chown`, `chgrp`; the time by `touch`).
pub fn attribute_commands(path: &str, attributes: &FileAttributes) -> Vec<String> {
    let path = quoted(path);
    let mut commands = Vec::new();
    if let Some(rights) = attributes.permissions {
        commands.push(format!("chmod {:04o} {path}", rights & 0o7777));
    }
    let owner = (attributes.user.clone()).or_else(|| attributes.uid.map(|id| id.to_string()));
    if let Some(owner) = owner {
        commands.push(format!("chown {} {path}", quoted(&owner)));
    }
    let group = (attributes.group.clone()).or_else(|| attributes.gid.map(|id| id.to_string()));
    if let Some(group) = group {
        commands.push(format!("chgrp {} {path}", quoted(&group)));
    }
    if let Some(modified) = attributes.mtime {
        commands.push(format!(
            "TZ=UTC touch -m -t {} {path}",
            touch_time(modified)
        ));
    }
    commands
}

/// The `C` record sending a file of `size` bytes named `name` with `rights`.
pub fn file_record(rights: u16, size: u64, name: &str) -> String {
    format!("C{:04o} {size} {name}\n", rights & 0o7777)
}

/// The size a `C` record announces; its failure told as the server's (`\x01`, `\x02` lines).
pub fn record_size(record: &str) -> Result<u64, String> {
    match record.as_bytes().first() {
        None => Err(EMPTY_LINE.to_string()),
        Some(1 | 2) => Err(record[1..].trim().to_string()),
        Some(b'C') => {
            let mut fields = record[1..].split(' ');
            let _rights = fields.next();
            (fields.next().and_then(|size| size.parse().ok()))
                .ok_or_else(|| format!("Received invalid control record ({record})."))
        }
        Some(_) => Err(format!("Received invalid control record ({record}).")),
    }
}

impl Scp {
    /// SCP over `commands` with the session's `options` (its SCP/Shell and Environment tabs).
    pub fn new(commands: Commands, options: &Options) -> Self {
        let listing = if options.ls_full_time { UNKNOWN } else { PLAIN };
        let command = options.listing_command.trim();
        Self {
            commands,
            listing: AtomicU8::new(listing),
            listing_command: if command.is_empty() { LISTING } else { command }.to_string(),
            ignore_warnings: options.ignore_ls_warnings,
            national: options.unset_national_vars,
            shift: i64::from(options.time_difference) * 60,
        }
    }

    pub fn commands(&self) -> &Commands {
        &self.commands
    }

    /// The listing command with `extra` options on `path`, `--full-time` while the server reads it.
    async fn ls(&self, extra: &str, path: &str) -> Result<Vec<Listed>, String> {
        let base = format!("{}{extra}", self.listing_command);
        let path = quoted(path);
        if self.listing.load(Ordering::Relaxed) != PLAIN {
            let output = self.run_ls(&format!("{base} --full-time {path}")).await?;
            if let Some(text) = self.accepted(&output) {
                self.listing.store(FULL_TIME, Ordering::Relaxed);
                return Ok(self.read(text));
            }
            if self.listing.load(Ordering::Relaxed) == FULL_TIME {
                return Err(output.failure());
            }
        }
        let output = self.run_ls(&format!("{base} {path}")).await?;
        let text = self.accepted(&output).ok_or_else(|| output.failure())?;
        self.listing.store(PLAIN, Ordering::Relaxed);
        Ok(self.read(text))
    }

    /// `command` run with the national variables cleared when the session says so (WinSCP's
    /// `UnsetNationalVars`: month names in English for the listing).
    async fn run_ls(&self, command: &str) -> Result<crate::shell::Output, String> {
        match self.national {
            true => {
                (self.commands)
                    .run(&format!("unset {} ; {command}", NATIONAL_VARS.join(" ")))
                    .await
            }
            false => self.commands.run(command).await,
        }
    }

    /// The output read as a listing: a success, or with "Ignore LS warnings" any output at all.
    fn accepted<'a>(&self, output: &'a crate::shell::Output) -> Option<&'a str> {
        let read = output.status == 0 || (self.ignore_warnings && !output.out.trim().is_empty());
        read.then_some(output.out.as_str())
    }

    /// The listing's lines, a time without its zone taken as the server's wall clock in the client's
    /// zone and shifted by the session's time zone offset (WinSCP's `ShiftTimeInSeconds`).
    fn read(&self, text: &str) -> Vec<Listed> {
        let mut read = listing(text, today());
        zone_fixed(&mut read, self.shift);
        read
    }

    /// The item `path` itself (`follow`: what a link points to).
    pub async fn entry(&self, path: &str, follow: bool) -> Result<Listed, String> {
        let extra = if follow { " -L -d" } else { " -d" };
        let mut listed = self.ls(extra, path).await?;
        let mut one = listed
            .pop()
            .ok_or_else(|| format!("{path}: No such file"))?;
        one.entry.name = path.rsplit('/').next().unwrap_or(path).to_string();
        Ok(one)
    }

    /// The folder's items; links to folders taken as folders when `follow`.
    pub async fn list(&self, path: &str, follow: bool) -> Result<Vec<RemoteEntry>, String> {
        let listed = self.ls("", path).await?;
        let mut items: Vec<RemoteEntry> = listed.into_iter().map(|one| one.entry).collect();
        if follow {
            for item in items.iter_mut().filter(|item| item.is_link) {
                if let Ok(target) = self.entry(&child(path, &item.name), true).await {
                    item.is_dir = target.entry.is_dir;
                }
            }
        }
        Ok(items)
    }

    /// `command` run, its failure the error.
    pub async fn run(&self, command: &str) -> Result<(), String> {
        self.commands.check(command).await.map(|_| ())
    }

    /// The server's full path of `path` (relative ones under the home folder).
    pub async fn canonicalize(&self, path: &str) -> Result<String, String> {
        let output = self
            .commands
            .check(&format!("cd {} && pwd", quoted(path)))
            .await?;
        Ok(output.out.trim_end_matches(['\r', '\n']).to_string())
    }

    pub async fn set_attributes(
        &self,
        path: &str,
        attributes: &FileAttributes,
    ) -> Result<(), String> {
        for command in attribute_commands(path, attributes) {
            self.run(&command).await?;
        }
        Ok(())
    }

    /// The file `path` coming down (`scp -f`), its record read.
    pub async fn receive(&self, path: &str) -> Result<Receiving, String> {
        let channel = self
            .commands
            .open(&format!("scp -f {}", quoted(path)))
            .await?;
        let mut receiving = Receiving {
            stream: Stream::new(channel),
            left: 0,
        };
        receiving.stream.send(b"\0").await?;
        let record = receiving.stream.line().await?;
        receiving.left = record_size(&record)?;
        receiving.stream.send(b"\0").await?;
        Ok(receiving)
    }

    /// The file `path` going up (`scp -t`), `size` bytes with `rights`.
    pub async fn send(&self, path: &str, size: u64, rights: u16) -> Result<Sending, String> {
        let channel = self
            .commands
            .open(&format!("scp -t {}", quoted(path)))
            .await?;
        let mut sending = Sending {
            stream: Stream::new(channel),
            left: size,
        };
        sending.stream.acknowledged().await?;
        let name = path.rsplit('/').next().unwrap_or(path);
        let record = file_record(rights, size, name);
        sending.stream.send(record.as_bytes()).await?;
        sending.stream.acknowledged().await?;
        Ok(sending)
    }
}

/// An SCP channel's bytes as they come.
pub struct Stream {
    channel: russh::Channel<Msg>,
    pending: Vec<u8>,
    /// The command's error output, its failure's text when it ends early.
    err: Vec<u8>,
    ended: bool,
}

impl Stream {
    fn new(channel: russh::Channel<Msg>) -> Self {
        Self {
            channel,
            pending: Vec::new(),
            err: Vec::new(),
            ended: false,
        }
    }

    /// More bytes read into `pending`; false at the channel's end.
    async fn fill(&mut self) -> bool {
        while !self.ended {
            match self.channel.wait().await {
                Some(ChannelMsg::Data { data }) => {
                    self.pending.extend_from_slice(&data);
                    return true;
                }
                Some(ChannelMsg::ExtendedData { data, .. }) => self.err.extend_from_slice(&data),
                Some(ChannelMsg::Eof | ChannelMsg::Close) | None => self.ended = true,
                Some(_) => {}
            }
        }
        false
    }

    fn ended_early(&self) -> String {
        match String::from_utf8_lossy(&self.err).trim() {
            "" => "Connection has been unexpectedly closed.".to_string(),
            text => text.to_string(),
        }
    }

    async fn send(&mut self, data: &[u8]) -> Result<(), String> {
        (self.channel.data_bytes(data.to_vec()).await).map_err(|error| error.to_string())
    }

    /// One line (its `\n` dropped).
    async fn line(&mut self) -> Result<String, String> {
        loop {
            if let Some(at) = self.pending.iter().position(|byte| *byte == b'\n') {
                let line: Vec<u8> = self.pending.drain(..=at).collect();
                return Ok(String::from_utf8_lossy(&line[..at]).into_owned());
            }
            if !self.fill().await {
                return Err(self.ended_early());
            }
        }
    }

    /// The other side's `\0`; its `\x01` / `\x02` line is its failure.
    async fn acknowledged(&mut self) -> Result<(), String> {
        if self.pending.is_empty() && !self.fill().await {
            return Err(self.ended_early());
        }
        match self.pending[0] {
            0 => {
                self.pending.remove(0);
                Ok(())
            }
            _ => {
                let line = self.line().await?;
                Err(record_size(&line).err().unwrap_or(line))
            }
        }
    }

    /// The channel closed, the command's end awaited.
    async fn finish(&mut self) {
        let _ = self.channel.eof().await;
        while self.fill().await {}
        let _ = self.channel.close().await;
    }
}

/// A file coming down.
pub struct Receiving {
    stream: Stream,
    left: u64,
}

impl Receiving {
    /// Up to `buffer`'s length read; 0 at the file's end (its `\0` read and acknowledged).
    pub async fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.left == 0 {
            return Ok(0);
        }
        if self.stream.pending.is_empty() && !self.stream.fill().await {
            return Err(std::io::Error::other(self.stream.ended_early()));
        }
        let wanted = buffer
            .len()
            .min(usize::try_from(self.left).unwrap_or(usize::MAX));
        let taken = wanted.min(self.stream.pending.len());
        buffer[..taken].copy_from_slice(&self.stream.pending[..taken]);
        self.stream.pending.drain(..taken);
        self.left -= taken as u64;
        if self.left == 0 {
            let ended = self.stream.acknowledged().await;
            ended.map_err(std::io::Error::other)?;
            self.stream
                .send(b"\0")
                .await
                .map_err(std::io::Error::other)?;
            self.stream.finish().await;
        }
        Ok(taken)
    }
}

/// A file going up.
pub struct Sending {
    stream: Stream,
    left: u64,
}

impl Sending {
    pub async fn write_all(&mut self, data: &[u8]) -> std::io::Result<()> {
        if data.len() as u64 > self.left {
            return Err(std::io::Error::other("The file grew while being sent."));
        }
        self.stream
            .send(data)
            .await
            .map_err(std::io::Error::other)?;
        self.left -= data.len() as u64;
        Ok(())
    }

    /// The file's end sent and acknowledged.
    pub async fn shutdown(&mut self) -> std::io::Result<()> {
        if self.left != 0 {
            return Err(std::io::Error::other("The file shrank while being sent."));
        }
        self.stream
            .send(b"\0")
            .await
            .map_err(std::io::Error::other)?;
        let sent = self.stream.acknowledged().await;
        self.stream.finish().await;
        sent.map_err(std::io::Error::other)
    }
}

#[cfg(test)]
#[path = "scp_tests.rs"]
mod tests;
