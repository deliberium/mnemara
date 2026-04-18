#![forbid(unsafe_code)]

pub use mnemara_core::*;

#[cfg(feature = "file")]
pub use mnemara_store_file::{FileMemoryStore, FileStoreConfig};

#[cfg(feature = "sled")]
pub use mnemara_store_sled::{SledMemoryStore, SledStoreConfig};

#[cfg(feature = "protocol")]
pub use mnemara_protocol::*;

#[cfg(feature = "server")]
pub use mnemara_server::*;

pub const CRATE_NAME: &str = "mnemara";
