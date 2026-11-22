//! Libraries loaded while the program runs (ADR-0016: the plugins' DLLs): open one, find an exported
//! function in it; dropping it unloads it. A library's dependencies are looked for beside it first.

pub use super::os::Library;

/// The file name of library `name` on this system (`name.dll`, `libname.so`, `libname.dylib`).
pub fn file_name(name: &str) -> String {
    format!(
        "{}{name}{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    )
}
