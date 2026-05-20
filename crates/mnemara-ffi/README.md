# mnemara-ffi

C ABI bindings for embedding Mnemara's sled-backed memory store in non-Rust runtimes.

The ABI accepts and returns JSON payloads matching `mnemara-core` request and report types. Callers own returned strings and must release them with `mnemara_ffi_free_string`.
