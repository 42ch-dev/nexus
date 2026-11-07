//! Windows access policy: the document's owner, group, DACL (with its protected
//! bit) and hard-link count.
//!
//! `windows-sys` declares these functions inside an `extern "system"` block, so
//! each call is unsafe; the SAFETY comment at each site covers the handle and
//! buffer contract. The security descriptor is requested and applied with the
//! documented `GetFileSecurityW` / `SetSecurityInfo` pair rather than parsed by
//! hand; the owner/group/DACL component pointers come from the documented
//! `GetSecurityDescriptor*` accessors.

use std::fs::File;
use std::io;
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::io::{AsRawHandle, FromRawHandle as _};
use std::path::Path;
use std::ptr;

use windows_sys::Win32::Foundation::{GENERIC_WRITE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Security::Authorization::{SetSecurityInfo, SE_FILE_OBJECT};
use windows_sys::Win32::Security::{
    GetFileSecurityW, GetSecurityDescriptorControl, GetSecurityDescriptorDacl,
    GetSecurityDescriptorGroup, GetSecurityDescriptorOwner, GetSecurityDescriptorSacl, ACL,
    ATTRIBUTE_SECURITY_INFORMATION, DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION,
    LABEL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, PSID, SCOPE_SECURITY_INFORMATION, SE_DACL_PROTECTED,
    UNPROTECTED_DACL_SECURITY_INFORMATION,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, CREATE_NEW,
    FILE_ATTRIBUTE_NORMAL, READ_CONTROL, WRITE_DAC,
};

/// Owner, group and DACL is the access policy this writer reproduces; the
/// mandatory label, resource attributes and scoped policy are detected and
/// refused instead of being dropped.
const CAPTURE_INFORMATION: u32 = OWNER_SECURITY_INFORMATION
    | GROUP_SECURITY_INFORMATION
    | DACL_SECURITY_INFORMATION
    | LABEL_SECURITY_INFORMATION
    | ATTRIBUTE_SECURITY_INFORMATION
    | SCOPE_SECURITY_INFORMATION;

/// The security descriptor a publication must carry to the replacement.
pub struct WindowsPolicy {
    /// A self-relative descriptor, word-aligned for the Win32 security API.
    descriptor: Vec<u64>,
}

pub fn capture(target: &Path) -> io::Result<WindowsPolicy> {
    let path = wide_path(target);
    let mut needed = 0_u32;
    // SAFETY: the measuring form takes a null descriptor with a zero length and
    // a valid out-parameter; it is expected to fail with the required size.
    unsafe {
        GetFileSecurityW(
            path.as_ptr(),
            CAPTURE_INFORMATION,
            ptr::null_mut(),
            0,
            &mut needed,
        );
    }
    if needed == 0 {
        return Err(io::Error::last_os_error());
    }

    let words = usize::try_from(needed.div_ceil(8))
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "security descriptor length"))?;
    let mut descriptor = vec![0_u64; words];
    let length = needed;
    // SAFETY: the buffer holds at least `length` bytes (rounded up to whole
    // words) and the function writes at most that many, reporting them back
    // through `needed`.
    let written = unsafe {
        GetFileSecurityW(
            path.as_ptr(),
            CAPTURE_INFORMATION,
            descriptor.as_mut_ptr().cast(),
            length,
            &mut needed,
        )
    };
    if written == 0 {
        return Err(io::Error::last_os_error());
    }

    let descriptor_pointer: PSECURITY_DESCRIPTOR = descriptor.as_mut_ptr().cast();
    let mut sacl_present = 0_i32;
    let mut sacl: *mut ACL = ptr::null_mut();
    let mut defaulted = 0_i32;
    // SAFETY: `descriptor_pointer` addresses the captured self-relative
    // descriptor and the out-parameters are valid locals.
    let read = unsafe {
        GetSecurityDescriptorSacl(
            descriptor_pointer,
            &mut sacl_present,
            &mut sacl,
            &mut defaulted,
        )
    };
    if read == 0 {
        return Err(io::Error::last_os_error());
    }
    if sacl_present != 0 && !sacl.is_null() {
        // SAFETY: `sacl` points inside the captured descriptor.
        if unsafe { (*sacl).AceCount } > 0 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "refusing to replace a document whose mandatory label, resource \
                 attributes or scoped policy this writer cannot reproduce",
            ));
        }
    }

    Ok(WindowsPolicy { descriptor })
}

pub fn apply(policy: &WindowsPolicy, file: &File) -> io::Result<()> {
    let descriptor_pointer: PSECURITY_DESCRIPTOR = policy.descriptor.as_ptr().cast_mut().cast();
    let mut owner: PSID = ptr::null_mut();
    let mut group: PSID = ptr::null_mut();
    let mut dacl: *mut ACL = ptr::null_mut();
    let mut dacl_present = 0_i32;
    let mut defaulted = 0_i32;
    let mut control: u16 = 0;
    let mut revision = 0_u32;

    // SAFETY: every pointer comes from the captured self-relative descriptor,
    // which stays alive for this call, and the out-parameters are valid locals.
    let read = unsafe {
        GetSecurityDescriptorOwner(descriptor_pointer, &mut owner, &mut defaulted)
            & GetSecurityDescriptorGroup(descriptor_pointer, &mut group, &mut defaulted)
            & GetSecurityDescriptorDacl(
                descriptor_pointer,
                &mut dacl_present,
                &mut dacl,
                &mut defaulted,
            )
            & GetSecurityDescriptorControl(descriptor_pointer, &mut control, &mut revision)
    };
    if read == 0 {
        return Err(io::Error::last_os_error());
    }

    let dacl = if dacl_present != 0 {
        dacl.cast_const()
    } else {
        ptr::null()
    };
    // A protected DACL must stay protected; an inherited one must stay
    // inheritable, or the replacement silently changes the effective policy.
    let protection = if control & SE_DACL_PROTECTED != 0 {
        PROTECTED_DACL_SECURITY_INFORMATION
    } else {
        UNPROTECTED_DACL_SECURITY_INFORMATION
    };
    let information = OWNER_SECURITY_INFORMATION
        | GROUP_SECURITY_INFORMATION
        | DACL_SECURITY_INFORMATION
        | protection;

    // SAFETY: the handle is owned by `file` and the sids/DACL point into the
    // captured descriptor; a null SACL is passed because SACL-class policies are
    // refused at capture time.
    let status = unsafe {
        SetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            information,
            owner,
            group,
            dacl,
            ptr::null(),
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status.cast_signed()));
    }
    Ok(())
}

pub fn link_count(target: &Path) -> io::Result<u64> {
    let file = File::open(target)?;
    // SAFETY: the structure is plain old data and is fully initialised by the
    // documented call before it is read.
    let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: the handle is owned by `file` and the out-structure is a valid
    // local for the call.
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(u64::from(information.nNumberOfLinks))
}

/// Create the staged document file with no sharing, so no other principal can
/// open it between creation and the policy transition.
pub fn open_staging_file(path: &Path) -> io::Result<File> {
    let path = wide_path(path);
    // SAFETY: the path is NUL-terminated, the security attributes are absent (the
    // file inherits the private staging directory's DACL) and no template handle
    // is needed.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            GENERIC_WRITE | WRITE_DAC | READ_CONTROL,
            0,
            ptr::null(),
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `handle` is a valid owned handle returned by CreateFileW.
    Ok(unsafe { File::from_raw_handle(handle) })
}

pub fn access_policy_blob(target: &Path) -> io::Result<Vec<u8>> {
    let policy = capture(target)?;
    Ok(policy
        .descriptor
        .iter()
        .flat_map(|word| word.to_ne_bytes())
        .collect())
}

/// NUL-terminated UTF-16 form of a path for the wide Win32 APIs.
fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}
