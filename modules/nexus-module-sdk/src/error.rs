//! Module error type + sentinel mapping (AR-5; contract §6.3).

use crate::host::HostError;

/// note: errors a module can return from `compute`, mapped to the ABI error
/// sentinels (ABI §6.3; basic-combat semantics lib.rs L73–91).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleError {
    InputMalformed,
    SerializeFailed,
    OutputTooSmall,
    Host(HostError),
}

impl ModuleError {
    /// note: maps to the ABI `compute` return sentinel — `-1` for every variant
    /// except `OutputTooSmall` → `-2`; matches the host's mapping
    /// (`-1` → `ComputeError::ModuleComputeFailed(-1)`,
    /// `-2` → `ComputeError::OutputBufferTooSmall`).
    pub const fn to_compute_return(&self) -> i64 {
        match self {
            ModuleError::OutputTooSmall => -2,
            _ => -1,
        }
    }
}

/// note: writes `bytes` to `[out_ptr, out_ptr+bytes.len())`, respecting `out_cap`.
///
/// The capacity check happens **before** the copy (`bytes.len() > out_cap` →
/// `Err`), then `ptr::copy_nonoverlapping`.
///
/// # Safety contract
///
/// `out_ptr` must point into the module's own linear memory, reserved by the
/// host via `alloc`; the input and output buffers are separate allocations,
/// so the ranges never overlap.
// why: the `Result<usize, ()>` signature is pinned by AR-5 — the error is a
// boolean "buffer too small" signal, not a rich error; the caller maps it to the
// `-2` sentinel.
#[allow(clippy::result_unit_err)]
pub fn write_output(out_ptr: u32, out_cap: u32, bytes: &[u8]) -> Result<usize, ()> {
    if bytes.len() > out_cap as usize {
        return Err(());
    }
    // safety: `out_ptr` points into our own linear memory, reserved by the
    // host via `alloc`. The ranges are non-overlapping (input and output
    // buffers are separate allocations).
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_ptr as *mut u8, bytes.len());
    }
    Ok(bytes.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_compute_return_maps_sentinels() {
        assert_eq!(ModuleError::InputMalformed.to_compute_return(), -1);
        assert_eq!(ModuleError::SerializeFailed.to_compute_return(), -1);
        assert_eq!(
            ModuleError::Host(HostError::NotFound).to_compute_return(),
            -1
        );
        assert_eq!(ModuleError::OutputTooSmall.to_compute_return(), -2);
    }

    #[test]
    fn write_output_rejects_oversized_payload_before_copy() {
        let mut buf = [0u8; 4];
        write_output(buf.as_mut_ptr() as u32, 4, b"too long").expect_err("must fail");
        assert_eq!(&buf, &[0u8; 4]);
    }

    // why: the copy path (`ptr::copy_nonoverlapping` into the module's linear
    // memory) is only meaningful on wasm32, where pointers ARE u32 linear
    // addresses — a host pointer truncated to u32 is invalid memory. The copy
    // itself is a std primitive; the interesting logic (capacity check before
    // copy) is covered above, and the full write path is exercised by the
    // real-host integration tests on wasm32 modules.
}
