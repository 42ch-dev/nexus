//! Windows access policy: the document's owner, group and DACL.
//!
//! `windows-sys` declares these functions inside an `extern "system"` block, so
//! each call is unsafe; the SAFETY comment at each site covers the handle and
//! buffer contract. The security descriptor is requested and applied with the
//! documented `GetFileSecurityW`/`SetFileSecurityW` pair rather than parsed by
//! hand.

use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::os::windows::ffi::OsStrExt as _;
use std::path::Path;

use windows_sys::Win32::Security::{
    GetFileSecurityW, SetFileSecurityW, DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION,
    OWNER_SECURITY_INFORMATION,
};

/// The owner, group and DACL are the document's access policy; the SACL is
/// auditing policy and needs a privilege this writer does not hold.
const INFORMATION: u32 =
    OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;

/// The security descriptor a publication must carry to the replacement.
pub struct WindowsPolicy {
    descriptor: Vec<u8>,
}

pub fn capture(target: &Path) -> io::Result<WindowsPolicy> {
    let path = wide_path(target);
    let mut needed = 0_u32;

    // SAFETY: the measuring form takes a null descriptor with a zero length and
    // a valid out-parameter; it is expected to fail with the required size.
    unsafe {
        GetFileSecurityW(
            path.as_ptr(),
            INFORMATION,
            std::ptr::null_mut(),
            0,
            &mut needed,
        );
    }
    if needed == 0 {
        return Err(io::Error::last_os_error());
    }

    let length = usize::try_from(needed)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "security descriptor length"))?;
    let mut descriptor = vec![0_u8; length];
    // SAFETY: the buffer holds exactly `needed` bytes and the function writes at
    // most that many, reporting them back through `needed`.
    let written = unsafe {
        GetFileSecurityW(
            path.as_ptr(),
            INFORMATION,
            descriptor.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    };
    if written == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(WindowsPolicy { descriptor })
}

pub fn apply(policy: &WindowsPolicy, _file: &File, path: &Path) -> io::Result<()> {
    let path = wide_path(path);
    // SAFETY: `descriptor` is a self-relative security descriptor produced by
    // GetFileSecurityW for exactly these information classes.
    let result = unsafe {
        SetFileSecurityW(
            path.as_ptr(),
            INFORMATION,
            policy.descriptor.as_ptr().cast_mut().cast(),
        )
    };
    if result == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn access_policy_blob(target: &Path) -> io::Result<Vec<u8>> {
    Ok(capture(target)?.descriptor)
}

/// NUL-terminated UTF-16 form of a path for the wide Win32 APIs.
fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}
