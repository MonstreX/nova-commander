//! What kind of optical drive a CD drive is (CD-RW, DVD-ROM, BD-RE, ...), as FAR's `get_cdrom_type`
//! (`far/cddrv.cpp`): the profiles the drive reports to SCSI GET CONFIGURATION, else its MODE SENSE
//! capabilities page, else the words in its product name; "CD-ROM" when none of them tells.

use std::fs::{File, OpenOptions};
use std::mem::offset_of;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;

use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, HANDLE};
use windows_sys::Win32::Storage::IscsiDisc::{
    IOCTL_SCSI_PASS_THROUGH, SCSI_IOCTL_DATA_IN, SCSI_PASS_THROUGH,
};
use windows_sys::Win32::System::IO::DeviceIoControl;

use super::storage::product_id;
use crate::platform::volumes::CdType;

/// FAR's `cdrom_device_capabilities`.
mod caps {
    pub const CDROM: u16 = 1 << 0;
    pub const CDR: u16 = 1 << 1;
    pub const CDRW: u16 = 1 << 2;
    pub const DVDROM: u16 = 1 << 3;
    pub const DVDR: u16 = 1 << 4;
    pub const DVDRW: u16 = 1 << 5;
    pub const DVDRAM: u16 = 1 << 6;
    pub const BDROM: u16 = 1 << 7;
    pub const BDR: u16 = 1 << 8;
    pub const BDRE: u16 = 1 << 9;
    pub const HDDVDROM: u16 = 1 << 10;
    pub const HDDVDR: u16 = 1 << 11;
    pub const HDDVDRW: u16 = 1 << 12;
    pub const HDDVDRAM: u16 = 1 << 13;
}

/// The kind of the CD drive at `root` (`D:\` or `\\?\Volume{...}\`).
pub fn cd_type(root: &str) -> CdType {
    let trimmed = root.trim_end_matches('\\');
    let device = match trimmed.strip_prefix(r"\\?\") {
        Some(rest) => format!(r"\\.\{rest}"),
        None => format!(r"\\.\{trimmed}"),
    };
    // FAR's `file_share_all`: read, write and delete, as `std` shares by default.
    let opened = OpenOptions::new()
        .access_mode(GENERIC_READ | GENERIC_WRITE)
        .open(device);
    let Ok(device) = opened else {
        return CdType::CdRom;
    };
    let found = [from_configuration, from_mode_sense, from_product_id]
        .iter()
        .map(|read| read(&device))
        .find(|&found| found != 0)
        .unwrap_or(0);
    type_of(found)
}

/// FAR's `SCSI_PASS_THROUGH_WITH_BUFFERS`.
#[repr(C)]
struct PassThrough {
    header: SCSI_PASS_THROUGH,
    sense: [u8; 32],
    data: [u8; 512],
}

/// Sends a 6- or 10-byte command that reads into `data`; the bytes it gave, or `None`.
fn pass_through(device: &File, cdb: &[u8]) -> Option<Vec<u8>> {
    let mut command = PassThrough {
        header: SCSI_PASS_THROUGH {
            Length: u16::try_from(size_of::<SCSI_PASS_THROUGH>()).ok()?,
            ScsiStatus: 0,
            PathId: 0,
            TargetId: 1,
            Lun: 0,
            CdbLength: u8::try_from(cdb.len()).ok()?,
            SenseInfoLength: 32,
            DataIn: u8::try_from(SCSI_IOCTL_DATA_IN).ok()?,
            DataTransferLength: 512,
            TimeOutValue: 2,
            DataBufferOffset: offset_of!(PassThrough, data),
            SenseInfoOffset: u32::try_from(offset_of!(PassThrough, sense)).ok()?,
            Cdb: [0; 16],
        },
        sense: [0; 32],
        data: [0; 512],
    };
    command.header.Cdb[..cdb.len()].copy_from_slice(cdb);
    let mut returned = 0u32;
    let input = u32::try_from(size_of::<SCSI_PASS_THROUGH>()).ok()?;
    let output = u32::try_from(size_of::<PassThrough>()).ok()?;
    let buffer = (&raw mut command).cast::<core::ffi::c_void>();
    // SAFETY: an open device; `command` is the input (its header) and the output (all of it), and
    // outlives the synchronous call; `returned` is a valid out value.
    let ok = unsafe {
        DeviceIoControl(
            device.as_raw_handle() as HANDLE,
            IOCTL_SCSI_PASS_THROUGH,
            buffer,
            input,
            buffer,
            output,
            &raw mut returned,
            std::ptr::null_mut(),
        )
    };
    // SCSISTAT_GOOD is 0.
    if ok == 0 || command.header.ScsiStatus != 0 {
        return None;
    }
    let length = usize::try_from(command.header.DataTransferLength).ok()?;
    Some(command.data[..length.min(command.data.len())].to_vec())
}

/// SCSI GET CONFIGURATION for the profile list (feature 0), every profile the drive can use.
fn from_configuration(device: &File) -> u16 {
    // Operation 46h, request type "one feature", starting feature 0, allocation length 512.
    let cdb = [0x46, 0x02, 0, 0, 0, 0, 0, 0x02, 0x00, 0];
    pass_through(device, &cdb).map_or(0, |data| profiles(&data))
}

/// The capabilities of a GET CONFIGURATION answer: an 8-byte header (its data length first), then the
/// profile list feature (code 0) of 4-byte profile descriptors.
fn profiles(data: &[u8]) -> u16 {
    let big_endian = |bytes: &[u8]| {
        bytes
            .iter()
            .fold(0usize, |value, &byte| value << 8 | usize::from(byte))
    };
    if data.len() < 12 || data.len() < 4 + big_endian(&data[..4]) || big_endian(&data[8..10]) != 0 {
        return 0;
    }
    let length = usize::from(data[11]);
    data[12..]
        .as_chunks::<4>()
        .0
        .iter()
        .take(length / 4)
        .map(|profile| from_profile(u16::from_be_bytes([profile[0], profile[1]])))
        .fold(0, |found, more| found | more)
}

/// FAR's `profile_to_capabilities` (MMC profile numbers).
fn from_profile(profile: u16) -> u16 {
    match profile {
        0x08 | 0x20 => caps::CDROM,
        0x09 | 0x21 => caps::CDR,
        0x0A | 0x22 => caps::CDRW,
        0x10 => caps::DVDROM,
        0x11 | 0x18 | 0x1B | 0x2B => caps::DVDR,
        0x12 => caps::DVDRAM,
        0x13..=0x17 | 0x1A | 0x2A => caps::DVDRW,
        0x40 => caps::BDROM,
        0x41 | 0x42 => caps::BDR,
        0x43 => caps::BDRE,
        0x50 => caps::HDDVDROM,
        0x51 | 0x58 => caps::HDDVDR,
        0x52 => caps::HDDVDRAM,
        0x53 | 0x5A => caps::HDDVDRW,
        _ => 0,
    }
}

/// SCSI MODE SENSE of the CD/DVD capabilities page (2Ah), for drives older than GET CONFIGURATION.
fn from_mode_sense(device: &File) -> u16 {
    // Operation 1Ah, no block descriptors, current values of page 2Ah; FAR's allocation length: the
    // 4-byte MODE_PARAMETER_HEADER and the 24-byte CDVD_CAPABILITIES_PAGE.
    let cdb = [0x1A, 0x08, 0x2A, 0, 4 + 24, 0];
    pass_through(device, &cdb).map_or(0, |data| capabilities_page(&data))
}

/// FAR's reading of `CDVD_CAPABILITIES_PAGE` after the 4-byte mode parameter header: byte 2 says what
/// it reads, byte 3 what it writes; a drive that answers reads CDs at least.
fn capabilities_page(data: &[u8]) -> u16 {
    let (Some(&read), Some(&write)) = (data.get(4 + 2), data.get(4 + 3)) else {
        return caps::CDROM;
    };
    let bit = |byte: u8, at: u8, then: u16| if byte & (1 << at) != 0 { then } else { 0 };
    caps::CDROM
        | bit(read, 3, caps::DVDROM)
        | bit(read, 4, caps::DVDROM)
        | bit(read, 5, caps::DVDRAM)
        | bit(write, 0, caps::CDR)
        | bit(write, 1, caps::CDRW)
        | bit(write, 4, caps::DVDR)
        | bit(write, 5, caps::DVDRAM)
}

/// The words in the drive's product name ("DVD-RW", "BD-RE"), FAR's last resort.
fn from_product_id(device: &File) -> u16 {
    product_id(device).map_or(0, |name| from_name(&name))
}

/// FAR's `product_id_to_capatibilities`: the letters of the name, each pattern with the letters that
/// must not stand before or after it ("CDR" in "CDROM" is not a writer).
fn from_name(name: &str) -> u16 {
    let letters: String = name.chars().filter(char::is_ascii_alphabetic).collect();
    let patterns: [(&str, &[&str], &[&str], u16); 14] = [
        ("CDROM", &[], &[], caps::CDROM),
        ("CDR", &[], &["OM", "W"], caps::CDR),
        ("CDRW", &[], &[], caps::CDRW),
        ("DVDROM", &["HD"], &[], caps::DVDROM),
        ("DVDR", &["HD"], &["OM", "W", "AM"], caps::DVDR),
        ("DVDRW", &["HD"], &[], caps::DVDRW),
        ("DVDRAM", &["HD"], &[], caps::DVDRAM),
        ("BDROM", &[], &[], caps::BDROM),
        ("BDR", &[], &["OM", "E"], caps::BDR),
        ("BDRE", &[], &[], caps::BDRE),
        ("HDDVDROM", &[], &[], caps::HDDVDROM),
        ("HDDVDR", &[], &["OM", "W", "AM"], caps::HDDVDR),
        ("HDDVDRW", &[], &[], caps::HDDVDRW),
        ("HDDVDRAM", &[], &[], caps::HDDVDRAM),
    ];
    patterns
        .iter()
        .filter(|(pattern, before, after, _)| {
            letters.find(pattern).is_some_and(|at| {
                let (prefix, suffix) = (&letters[..at], &letters[at + pattern.len()..]);
                !before.iter().any(|text| prefix.ends_with(text))
                    && !after.iter().any(|text| suffix.starts_with(text))
            })
        })
        .fold(0, |found, (.., value)| found | value)
}

/// FAR's `get_cd_type`: the most capable kind whose capabilities are all there.
fn type_of(found: u16) -> CdType {
    let kinds = [
        (CdType::HdDvdRam, caps::HDDVDRAM),
        (CdType::HdDvdRw, caps::HDDVDRW),
        (CdType::HdDvdR, caps::HDDVDR),
        (CdType::HdDvdRom, caps::HDDVDROM),
        (CdType::BdRe, caps::BDRE),
        (CdType::BdR, caps::BDR),
        (CdType::BdRom, caps::BDROM),
        (CdType::DvdRam, caps::DVDRAM),
        (CdType::DvdRw, caps::DVDRW),
        (CdType::CdRwDvd, caps::CDRW | caps::DVDROM),
        (CdType::DvdRom, caps::DVDROM),
        (CdType::CdRw, caps::CDRW),
        (CdType::CdRom, caps::CDROM),
    ];
    kinds
        .iter()
        .find(|&&(_, needed)| found & needed == needed)
        .map_or(CdType::CdRom, |&(kind, _)| kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_most_capable_kind_wins() {
        assert_eq!(type_of(0), CdType::CdRom);
        assert_eq!(
            type_of(caps::CDROM | caps::CDR),
            CdType::CdRom,
            "FAR has no CD-R row"
        );
        assert_eq!(type_of(caps::CDROM | caps::CDRW), CdType::CdRw);
        assert_eq!(type_of(caps::CDRW | caps::DVDROM), CdType::CdRwDvd);
        assert_eq!(
            type_of(caps::CDRW | caps::DVDROM | caps::DVDRW),
            CdType::DvdRw
        );
        assert_eq!(type_of(caps::DVDRW | caps::BDROM), CdType::BdRom);
        assert_eq!(type_of(caps::BDRE | caps::HDDVDROM), CdType::HdDvdRom);
    }

    #[test]
    fn a_profile_list_names_every_profile() {
        // Header: data length 16 (4 + 16 bytes given), current profile; feature 0, 8 bytes of profiles.
        let answer = [
            0, 0, 0, 16, 0, 0, 0, 0x10, //
            0, 0, 0x03, 8, //
            0, 0x10, 1, 0, //
            0, 0x0A, 0, 0,
        ];
        assert_eq!(profiles(&answer), caps::DVDROM | caps::CDRW);
        assert_eq!(profiles(&answer[..10]), 0, "too short");
        let mut other_feature = answer;
        other_feature[9] = 1;
        assert_eq!(profiles(&other_feature), 0, "not the profile list");
        let mut claims_more = answer;
        claims_more[3] = 40;
        assert_eq!(profiles(&claims_more), 0, "longer than given");
    }

    #[test]
    fn profiles_map_as_far_maps_them() {
        assert_eq!(from_profile(0x08), caps::CDROM);
        assert_eq!(from_profile(0x14), caps::DVDRW);
        assert_eq!(from_profile(0x1B), caps::DVDR);
        assert_eq!(from_profile(0x43), caps::BDRE);
        assert_eq!(from_profile(0x5A), caps::HDDVDRW);
        assert_eq!(from_profile(0xFFFF), 0);
    }

    #[test]
    fn the_capabilities_page_reads_its_bits() {
        let page = |read: u8, write: u8| [0, 0, 0, 0, 0x2A, 0x18, read, write];
        assert_eq!(capabilities_page(&page(0, 0)), caps::CDROM);
        assert_eq!(
            capabilities_page(&page(1 << 3, 1 << 1)),
            caps::CDROM | caps::DVDROM | caps::CDRW
        );
        assert_eq!(capabilities_page(&[0; 4]), caps::CDROM, "no page");
    }

    #[test]
    fn product_names_as_in_fars_own_test() {
        let cases = [
            ("CD-ROM", caps::CDROM),
            ("CD+R", caps::CDR),
            ("CD+-RW Foo", caps::CDRW),
            ("DVD ROM", caps::DVDROM),
            ("DVD_R_Bar", caps::DVDR),
            ("123+DVD+RW", caps::DVDRW),
            ("DVD+RAM", caps::DVDRAM),
            ("BD_ROM", caps::BDROM),
            ("UberBDR", caps::BDR),
            ("HDBD/RE", caps::BDRE),
            ("HD-DVD-ROM", caps::HDDVDROM),
            ("HDDVDR", caps::HDDVDR),
            ("HDDVD RW", caps::HDDVDRW),
            ("HD/DVD+RAM", caps::HDDVDRAM),
        ];
        for (name, expected) in cases {
            assert_eq!(from_name(name), expected, "{name}");
        }
    }

    #[test]
    fn product_names_tell_writers_from_readers() {
        assert_eq!(from_name("Virtual DVD-ROM"), caps::DVDROM);
        assert_eq!(
            from_name("CD-ROM"),
            caps::CDROM,
            "CDR before OM is not a writer"
        );
        assert_eq!(from_name("DVD+-RW DH16AERSH"), caps::DVDRW);
        assert_eq!(from_name("BD-RE  BH16NS40"), caps::BDRE);
        assert_eq!(
            from_name("HD DVD-ROM"),
            caps::HDDVDROM,
            "no DVD-ROM after HD"
        );
        assert_eq!(from_name("Disk"), 0);
    }
}
