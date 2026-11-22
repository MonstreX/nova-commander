//! What FAR's proclist reads in another process's memory (`ipc.hpp`, `PrintModules`): its current folder
//! and environment from its process parameters, and its modules - as the system lists them
//! (`EnumProcessModulesEx`, every bitness), else from the loader's list in its PEB. A WOW64 process is
//! read through its 32-bit PEB (FAR's `bitness::x86`); the program is 64-bit.

use windows_sys::Wdk::System::Threading::{
    NtQueryInformationProcess, ProcessBasicInformation, ProcessWow64Information,
};
use windows_sys::Win32::Foundation::{HANDLE, HMODULE};
use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows_sys::Win32::System::ProcessStatus::{
    EnumProcessModulesEx, GetModuleFileNameExW, GetModuleInformation, LIST_MODULES_ALL, MODULEINFO,
};
use windows_sys::Win32::System::Threading::{
    IsWow64Process, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
};

use super::process_open::{Opened, open};

/// What a process's parameters tell (FAR's `PrintNTCurDirAndEnv`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcessMemory {
    pub current_folder: Option<String>,
    /// Its environment's `NAME=value` strings in their order; empty when not asked or not read.
    pub environment: Vec<String>,
}

/// One module of a process.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Module {
    pub base: u64,
    pub size: u32,
    /// Its file's whole path; empty when not told.
    pub path: String,
}

/// The places of what is read, for a PEB of one bitness (the structures FAR declares in `ipc.hpp`).
struct Layout {
    pointer: usize,
    peb_loader: u64,
    peb_parameters: u64,
    current_folder: u64,
    environment: u64,
    environment_size: u64,
    /// `PEB_LDR_DATA.InMemoryOrderModuleList`.
    loader_in_memory: u64,
    /// `LDR_MODULE.InMemoryOrderModuleList`.
    module_links: u64,
    module_base: u64,
    module_size: u64,
    module_name: u64,
    /// `UNICODE_STRING.Buffer`.
    text_address: u64,
}

const LAYOUT_64: Layout = Layout {
    pointer: 8,
    peb_loader: 0x18,
    peb_parameters: 0x20,
    current_folder: 0x38,
    environment: 0x80,
    environment_size: 0x3F0,
    loader_in_memory: 0x20,
    module_links: 0x10,
    module_base: 0x30,
    module_size: 0x40,
    module_name: 0x48,
    text_address: 8,
};

const LAYOUT_32: Layout = Layout {
    pointer: 4,
    peb_loader: 0x0C,
    peb_parameters: 0x10,
    current_folder: 0x24,
    environment: 0x48,
    environment_size: 0x290,
    loader_in_memory: 0x14,
    module_links: 0x08,
    module_base: 0x18,
    module_size: 0x20,
    module_name: 0x24,
    text_address: 4,
};

/// The most of an environment read (a broken size never reads the whole address space).
const MOST_ENVIRONMENT: usize = 16 * 1024 * 1024;
/// The most modules walked in the loader's list (a broken list never loops for ever).
const MOST_MODULES: usize = 16 * 1024;

/// A process opened to read its memory, with the layout of its PEB.
struct Reader {
    process: Opened,
    layout: &'static Layout,
    peb: u64,
}

impl Reader {
    fn new(pid: u32) -> Option<Self> {
        let process = open(pid, PROCESS_QUERY_INFORMATION | PROCESS_VM_READ).ok()?;
        let mut wow64 = 0;
        // SAFETY: the process is open; the answer goes to a local.
        let wow64 = unsafe { IsWow64Process(process.0, &raw mut wow64) } != 0 && wow64 != 0;
        let peb = if wow64 {
            // The 32-bit PEB's address (a `ULONG_PTR`).
            query_word(process.0, ProcessWow64Information, 1, 0)?
        } else {
            // `PROCESS_BASIC_INFORMATION` (six words): its PEB's address is its second.
            query_word(process.0, ProcessBasicInformation, 6, 1)?
        };
        let layout = if wow64 { &LAYOUT_32 } else { &LAYOUT_64 };
        (peb != 0).then_some(Self {
            process,
            layout,
            peb,
        })
    }

    fn bytes(&self, address: u64, length: usize) -> Option<Vec<u8>> {
        let mut buffer = vec![0u8; length];
        let mut read = 0usize;
        let at: *const std::ffi::c_void =
            std::ptr::with_exposed_provenance(usize::try_from(address).ok()?);
        // SAFETY: the process is open to read; the buffer is writable for `length` bytes; the address
        // is the other process's, never this one's memory.
        let done = unsafe {
            ReadProcessMemory(
                self.process.0,
                at,
                buffer.as_mut_ptr().cast(),
                length,
                &raw mut read,
            )
        };
        (done != 0 && read == length).then_some(buffer)
    }

    /// A pointer of the process's bitness at `address`.
    fn pointer(&self, address: u64) -> Option<u64> {
        let bytes = self.bytes(address, self.layout.pointer)?;
        let mut word = [0u8; 8];
        word[..bytes.len()].copy_from_slice(&bytes);
        Some(u64::from_le_bytes(word))
    }

    /// The text of the `UNICODE_STRING` at `address`.
    fn text(&self, address: u64) -> Option<String> {
        let length = usize::from(u16::from_le_bytes(self.bytes(address, 2)?.try_into().ok()?));
        let buffer = self.pointer(address.checked_add(self.layout.text_address)?)?;
        if length == 0 || buffer == 0 {
            return Some(String::new());
        }
        Some(utf16(&self.bytes(buffer, length)?))
    }

    fn parameters(&self) -> Option<u64> {
        self.pointer(self.peb.checked_add(self.layout.peb_parameters)?)
            .filter(|at| *at != 0)
    }

    /// The environment block: the size its parameters give, else FAR's doubling reads until it ends.
    fn environment(&self, parameters: u64) -> Vec<String> {
        let field = |offset: u64| parameters.checked_add(offset);
        let Some(block) = field(self.layout.environment).and_then(|at| self.pointer(at)) else {
            return Vec::new();
        };
        let said = (field(self.layout.environment_size).and_then(|at| self.pointer(at)))
            .and_then(|size| usize::try_from(size).ok())
            .filter(|size| (2..=MOST_ENVIRONMENT).contains(size));
        let bytes = match said {
            Some(size) => self.bytes(block, size),
            None => self.doubled(block),
        };
        bytes.map_or_else(Vec::new, |bytes| strings(&utf16_units(&bytes)))
    }

    /// FAR's read of an environment of no known size: 4 KB, doubled until two NULs end it.
    fn doubled(&self, block: u64) -> Option<Vec<u8>> {
        let mut size = 4096;
        while size <= MOST_ENVIRONMENT {
            let bytes = self.bytes(block, size)?;
            if utf16_units(&bytes).windows(2).any(|pair| pair == [0, 0]) {
                return Some(bytes);
            }
            size *= 2;
        }
        None
    }

    /// The loader's modules from its PEB (FAR's `print_modules` of `ipc.hpp`), the first one first.
    fn loader_modules(&self) -> Vec<Module> {
        let layout = self.layout;
        let Some(loader) = self
            .pointer(self.peb.saturating_add(layout.peb_loader))
            .filter(|at| *at != 0)
        else {
            return Vec::new();
        };
        let Some(end) = loader.checked_add(layout.loader_in_memory) else {
            return Vec::new();
        };
        let mut modules = Vec::new();
        let mut link = self.pointer(end);
        while let Some(at) = link.filter(|at| *at != 0 && *at != end) {
            if modules.len() >= MOST_MODULES {
                break;
            }
            // Addresses read from the other process: a broken list ends the walk, never overflows.
            let Some(entry) = at.checked_sub(layout.module_links) else {
                break;
            };
            let field = |offset: u64| entry.checked_add(offset);
            let (Some(base), Some(size), Some(name)) = (
                field(layout.module_base).and_then(|at| self.pointer(at)),
                field(layout.module_size).and_then(|at| self.bytes(at, 4)),
                field(layout.module_name),
            ) else {
                break;
            };
            modules.push(Module {
                base,
                size: u32::from_le_bytes(size.try_into().unwrap_or_default()),
                path: self.text(name).unwrap_or_default(),
            });
            link = self.pointer(at);
        }
        modules
    }
}

/// Word `index` of the process's information `class`, `words` 8-byte words long (the program is 64-bit).
fn query_word(process: HANDLE, class: i32, words: usize, index: usize) -> Option<u64> {
    let mut buffer = [0u64; 6];
    let words = words.min(buffer.len());
    let size = u32::try_from(words * 8).unwrap_or(0);
    // SAFETY: the process is open; the buffer is writable for `size` bytes and aligned.
    let status = unsafe {
        NtQueryInformationProcess(
            process,
            class,
            buffer.as_mut_ptr().cast(),
            size,
            std::ptr::null_mut(),
        )
    };
    (status >= 0).then(|| buffer[index.min(words - 1)])
}

fn utf16_units(bytes: &[u8]) -> Vec<u16> {
    (bytes.chunks_exact(2))
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect()
}

fn utf16(bytes: &[u8]) -> String {
    String::from_utf16_lossy(&utf16_units(bytes))
}

/// The strings of an environment block, each ended by a NUL, the block by an empty one.
pub(crate) fn strings(units: &[u16]) -> Vec<String> {
    (units.split(|unit| *unit == 0))
        .take_while(|text| !text.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

/// `pid`'s current folder and, with `environment`, its environment; `None` when its memory cannot be
/// read.
pub fn process_memory(pid: u32, environment: bool) -> Option<ProcessMemory> {
    let reader = Reader::new(pid)?;
    let parameters = reader.parameters()?;
    Some(ProcessMemory {
        current_folder: (parameters.checked_add(reader.layout.current_folder))
            .and_then(|at| reader.text(at))
            .filter(|folder| !folder.is_empty()),
        environment: if environment {
            reader.environment(parameters)
        } else {
            Vec::new()
        },
    })
}

/// `pid`'s modules (FAR's `PrintModules`): as the system lists them, else as its loader's list in its
/// memory has them; `None` when it cannot be opened.
pub fn process_modules(pid: u32) -> Option<Vec<Module>> {
    let process = open(pid, PROCESS_QUERY_INFORMATION | PROCESS_VM_READ).ok()?;
    let listed = listed_modules(process.0);
    if !listed.is_empty() {
        return Some(listed);
    }
    drop(process);
    // FAR's other way (`print_modules` of `ipc.hpp`): the loader's list in its order.
    let modules = Reader::new(pid)?.loader_modules();
    Some(modules)
}

/// `EnumProcessModulesEx(LIST_MODULES_ALL)` grown until every module fits, each with its place and file.
fn listed_modules(process: HANDLE) -> Vec<Module> {
    let mut handles: Vec<HMODULE> = vec![std::ptr::null_mut(); 1024];
    loop {
        let size = u32::try_from(handles.len() * size_of::<HMODULE>()).unwrap_or(u32::MAX);
        let mut needed = 0u32;
        // SAFETY: the process is open; the array holds `size` bytes; the size needed goes to a local.
        let done = unsafe {
            EnumProcessModulesEx(
                process,
                handles.as_mut_ptr(),
                size,
                &raw mut needed,
                LIST_MODULES_ALL,
            )
        };
        if done == 0 {
            return Vec::new();
        }
        let count = usize::try_from(needed).unwrap_or(0) / size_of::<HMODULE>();
        if needed <= size {
            handles.truncate(count);
            break;
        }
        handles.resize(count, std::ptr::null_mut());
    }
    handles
        .iter()
        .map(|module| module_of(process, *module))
        .collect()
}

fn module_of(process: HANDLE, module: HMODULE) -> Module {
    let mut info = MODULEINFO {
        lpBaseOfDll: std::ptr::null_mut(),
        SizeOfImage: 0,
        EntryPoint: std::ptr::null_mut(),
    };
    let size = u32::try_from(size_of::<MODULEINFO>()).unwrap_or(0);
    // SAFETY: the process is open; the module is one it listed; the answer goes to a local.
    unsafe { GetModuleInformation(process, module, &raw mut info, size) };
    let mut name = vec![0u16; 32 * 1024];
    let capacity = u32::try_from(name.len()).unwrap_or(u32::MAX);
    // SAFETY: as above; the buffer holds `capacity` units.
    let length = unsafe { GetModuleFileNameExW(process, module, name.as_mut_ptr(), capacity) };
    let length = usize::try_from(length).unwrap_or(0).min(name.len());
    Module {
        base: info.lpBaseOfDll as u64,
        size: info.SizeOfImage,
        path: String::from_utf16_lossy(&name[..length]),
    }
}

#[cfg(test)]
#[path = "system_process_memory_tests.rs"]
mod tests;
