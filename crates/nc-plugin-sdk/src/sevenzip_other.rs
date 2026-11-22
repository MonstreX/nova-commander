//! Other systems until the Linux port finds 7-Zip's `7z.so` (S7, ADR-0022 §6): no library, no format.

use std::io;
use std::path::Path;

use super::{Format, IsArc, Module, Opening, Prop, PropInfo};

/// No library.
#[derive(Clone, Debug, Default)]
pub struct Library;

impl Library {
    pub fn find(_plugin_folder: &Path) -> Self {
        Self
    }

    pub fn folder(&self) -> Option<std::path::PathBuf> {
        None
    }

    pub fn sfx_modules(&self) -> Vec<std::path::PathBuf> {
        Vec::new()
    }

    pub fn set_code_pages(&self, _oem: u32, _ansi: u32) {}

    pub fn modules(&self) -> Vec<Module> {
        Vec::new()
    }

    pub fn formats(&self) -> Vec<Format> {
        Vec::new()
    }

    pub fn is_arc(&self, _format: usize, _data: &[u8]) -> IsArc {
        IsArc::No
    }

    pub fn open(
        &self,
        _format: usize,
        _stream: &InStream,
        _path: Option<&Path>,
        _allow_tail: bool,
        _asker: &mut dyn Opening,
    ) -> io::Result<Option<Archive>> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }

    pub fn physical_size(
        &self,
        _format: usize,
        _stream: &InStream,
        _max_start: u64,
    ) -> Option<u64> {
        None
    }

    pub fn create_archive(
        &self,
        _format: usize,
        _count: u32,
        _properties: &[(String, Prop)],
        _sink: &mut dyn super::Updating,
    ) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
}

/// A stream: none can be made.
#[derive(Clone, Debug)]
pub struct InStream(());

impl InStream {
    pub fn file(_path: &Path) -> io::Result<Self> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }

    pub fn from_offset(&self, _offset: u64) -> io::Result<Self> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }

    pub fn read(&self, _buffer: &mut [u8]) -> io::Result<usize> {
        Ok(0)
    }

    pub fn seek(&self, _to: io::SeekFrom) -> io::Result<u64> {
        Ok(0)
    }
}

/// An archive: none can be opened.
#[derive(Debug)]
pub struct Archive(());

impl Archive {
    pub fn count(&self) -> u32 {
        0
    }

    pub fn property(&self, _index: u32, _id: u32) -> Prop {
        Prop::Empty
    }

    pub fn archive_property(&self, _id: u32) -> Prop {
        Prop::Empty
    }

    pub fn properties(&self) -> Vec<PropInfo> {
        Vec::new()
    }

    pub fn archive_properties(&self) -> Vec<PropInfo> {
        Vec::new()
    }

    pub fn stream(&self, _index: u32) -> Option<InStream> {
        None
    }

    pub fn main_file(&self) -> Option<u32> {
        None
    }

    pub fn extract(
        &self,
        _indices: &[u32],
        _test: bool,
        _sink: &mut dyn super::Extracting,
    ) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }

    pub fn volumes(&self) -> &[String] {
        &[]
    }

    pub fn update(
        &self,
        _count: u32,
        _properties: &[(String, Prop)],
        _sink: &mut dyn super::Updating,
    ) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
}
