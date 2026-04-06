//! Stable API boundary for games and native plugins.
//!
//! # Safety
//! Functions exported with `#[no_mangle] extern "C"` are `unsafe` to call from hosts
//! that do not uphold documented invariants (valid pointers, lifetimes, threading).

/// Opaque engine handle for plugin callbacks.
#[repr(C)]
pub struct EngineHandle {
    _private: [u8; 0],
}

/// Plugin entry: called once after dynamic load. Returns 0 on success.
///
/// # Safety
/// `_engine` must be a valid pointer supplied by the host when such a handle exists, or null only
/// if the host documents that as acceptable.
#[no_mangle]
pub unsafe extern "C" fn voxel_plugin_init(_engine: *mut EngineHandle) -> i32 {
    0
}

/// Plugin shutdown before unload.
///
/// # Safety
/// Same pointer rules as [`voxel_plugin_init`].
#[no_mangle]
pub unsafe extern "C" fn voxel_plugin_shutdown(_engine: *mut EngineHandle) -> i32 {
    0
}
