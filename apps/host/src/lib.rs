#[macro_use]
extern crate log;

mod config;
mod error;
mod package;

#[cfg(any(
    all(
        target_arch = "x86_64",
        any(target_os = "linux", target_os = "darwin", target_os = "windows")
    ),
    all(target_arch = "aarch64", target_os = "linux")
))]
pub mod runtime;

pub use config::Config;
pub use error::Error;
