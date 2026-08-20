//! Host-import wrappers (AR-4).
//!
//! A module may import up to two host functions from the `nexus` module
//! namespace. The host registers only the functions the module's manifest
//! whitelists; importing a non-whitelisted function fails instantiation. The
//! SDK never invents other host functions — importing a non-whitelisted
//! `nexus::*` function fails instantiation on the real host, and the SDK must
//! not make that state easy to reach.
//!
//! ## Memory-buffer ABI
//!
//! Both host functions follow the same convention:
//!
//! ```text
//! nexus::kb_read(id_ptr: u32, id_len: u32, out_ptr: u32, out_cap: u32) -> i64
//! nexus::narrative_query(q_ptr: u32, q_len: u32, out_ptr: u32, out_cap: u32) -> i64
//! ```
//!
//! The module owns its linear memory and passes a buffer it allocated for the
//! result. The host reads the request bytes from `[ptr, ptr+len)`, writes the
//! UTF-8 JSON response into `[out_ptr, out_ptr+written)`, and returns
//! `written` as a non-negative `i64`. On failure it returns a sentinel:
//!
//! | Return | Meaning |
//! | --- | --- |
//! | `>= 0` | Bytes written to `out`. |
//! | `-1`   | Not found / unsupported query. |
//! | `-2`   | `out_cap` too small for the response. |

/// Sentinel returned by host functions when the lookup yields nothing.
pub const RET_NOT_FOUND: i64 = -1;
/// Sentinel returned when the caller's output buffer is too small.
pub const RET_OVERFLOW: i64 = -2;

/// Errors returned by host functions, mapped from the ABI sentinels
/// (host.rs L41–43, ABI §6.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    /// `-1` — the lookup yielded nothing.
    NotFound,
    /// `-2` — the caller's output buffer was too small.
    OutputTooSmall,
    /// Any other negative return value.
    Unknown(i64),
}

// Raw host imports, whitelisted by the ABI (compute-module-abi.md §3).
// Declared in a private submodule so the safe wrappers below can use the
// same names.
mod raw {
    #[link(wasm_import_module = "nexus")]
    extern "C" {
        pub(super) fn kb_read(id_ptr: u32, id_len: u32, out_ptr: u32, out_cap: u32) -> i64;
        pub(super) fn narrative_query(q_ptr: u32, q_len: u32, out_ptr: u32, out_cap: u32) -> i64;
    }
}

/// Read a `KeyBlock` by ID from the invocation snapshot.
///
/// Allocates the request and output buffers via the module's own `alloc`,
/// passes `(ptr, len, out_ptr, out_cap)`, and reads the response. Request and
/// output buffers never overlap (ABI §6.4).
pub fn kb_read(id: &str, out_cap: u32) -> Result<Vec<u8>, HostError> {
    call_host(
        |ptr, len, out_ptr| unsafe { raw::kb_read(ptr, len, out_ptr, out_cap) },
        id,
        out_cap,
    )
}

/// Query narrative context.
pub fn narrative_query(query: &str, out_cap: u32) -> Result<Vec<u8>, HostError> {
    call_host(
        |ptr, len, out_ptr| unsafe { raw::narrative_query(ptr, len, out_ptr, out_cap) },
        query,
        out_cap,
    )
}

fn call_host<F>(f: F, request: &str, out_cap: u32) -> Result<Vec<u8>, HostError>
where
    F: FnOnce(u32, u32, u32) -> i64,
{
    let req_bytes = request.as_bytes();
    let req_ptr = crate::shim::alloc(req_bytes.len() as u32);
    // SAFETY: `req_ptr` is a leaked allocation of exactly `req_bytes.len()`
    // bytes in the module's own linear memory (from `shim::alloc`).
    unsafe {
        std::ptr::copy_nonoverlapping(req_bytes.as_ptr(), req_ptr as *mut u8, req_bytes.len());
    }
    let out_ptr = crate::shim::alloc(out_cap);
    let ret = f(req_ptr, req_bytes.len() as u32, out_ptr);
    map_host_ret(ret)?;
    // SAFETY: the host wrote exactly `ret` bytes at `out_ptr` (an address
    // previously returned by our own `alloc`).
    let bytes = unsafe { std::slice::from_raw_parts(out_ptr as *const u8, ret as usize) }.to_vec();
    Ok(bytes)
}

/// Map a host-function return value to the typed error (AR-4 sentinel
/// mapping): `>= 0` → success (bytes written), `-1` → not found, `-2` →
/// output too small, any other negative → unknown.
fn map_host_ret(ret: i64) -> Result<(), HostError> {
    match ret {
        n if n >= 0 => Ok(()),
        RET_NOT_FOUND => Err(HostError::NotFound),
        RET_OVERFLOW => Err(HostError::OutputTooSmall),
        other => Err(HostError::Unknown(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_host_ret_maps_sentinels() {
        assert_eq!(map_host_ret(0), Ok(()));
        assert_eq!(map_host_ret(42), Ok(()));
        assert_eq!(map_host_ret(-1), Err(HostError::NotFound));
        assert_eq!(map_host_ret(-2), Err(HostError::OutputTooSmall));
        assert_eq!(map_host_ret(-3), Err(HostError::Unknown(-3)));
        assert_eq!(map_host_ret(-100), Err(HostError::Unknown(-100)));
    }
}
