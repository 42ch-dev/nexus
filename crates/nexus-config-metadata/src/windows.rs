//! Windows access policy: the document's owner, group, DACL (with its protected
//! bit) and hard-link count.
//!
//! `windows-sys` declares these functions inside an `extern "system"` block, so
//! each call is unsafe; the SAFETY comment at each site covers the handle and
//! buffer contract. The security descriptor is requested and applied with the
//! documented `GetFileSecurityW` / `SetSecurityInfo` pair rather than parsed by
//! hand; the owner/group/DACL component pointers come from the documented
//! `GetSecurityDescriptor*` accessors.

use std::ffi::c_void;
use std::fs::File;
use std::io;
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::io::{AsRawHandle, FromRawHandle as _};
use std::path::Path;
use std::ptr;

use windows_sys::Win32::Foundation::{GENERIC_WRITE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Security::Authorization::{SetSecurityInfo, SE_FILE_OBJECT};
use windows_sys::Win32::Security::{
    AclSizeInformation, EqualSid, GetAce, GetAclInformation, GetFileSecurityW,
    GetKernelObjectSecurity, GetSecurityDescriptorControl, GetSecurityDescriptorDacl,
    GetSecurityDescriptorGroup, GetSecurityDescriptorOwner, GetSecurityDescriptorSacl,
    IsWellKnownSid, WinBuiltinAdministratorsSid, WinLocalSystemSid, ACCESS_ALLOWED_ACE, ACE_HEADER,
    ACL, ACL_SIZE_INFORMATION, ATTRIBUTE_SECURITY_INFORMATION, DACL_SECURITY_INFORMATION,
    GROUP_SECURITY_INFORMATION, LABEL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
    PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SCOPE_SECURITY_INFORMATION,
    SE_DACL_PROTECTED, UNPROTECTED_DACL_SECURITY_INFORMATION,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, CREATE_NEW,
    FILE_ATTRIBUTE_NORMAL, READ_CONTROL, WRITE_DAC, WRITE_OWNER,
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
            &raw mut needed,
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
            &raw mut needed,
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
            &raw mut sacl_present,
            &raw mut sacl,
            &raw mut defaulted,
        )
    };
    if read == 0 {
        return Err(io::Error::last_os_error());
    }
    if sacl_present != 0 && !sacl.is_null() {
        let mut information = ACL_SIZE_INFORMATION::default();
        let information_size = u32::try_from(std::mem::size_of::<ACL_SIZE_INFORMATION>())
            .expect("ACL_SIZE_INFORMATION fits in a Win32 buffer");
        // SAFETY: the non-null SACL was returned from the live descriptor;
        // the aligned output buffer holds the requested information class.
        let queried = unsafe {
            GetAclInformation(
                sacl,
                (&raw mut information).cast(),
                information_size,
                AclSizeInformation,
            )
        };
        if queried == 0 {
            return Err(io::Error::last_os_error());
        }
        if information.AceCount > 0 {
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

    // SAFETY: `descriptor_pointer` points at the captured self-relative
    // descriptor owned by `policy`, which stays alive for this call, and
    // `owner`/`defaulted` are valid local out-parameters.
    let owner_read = unsafe {
        GetSecurityDescriptorOwner(descriptor_pointer, &raw mut owner, &raw mut defaulted)
    };
    // SAFETY: the same captured descriptor stays alive for this call, and
    // `group`/`defaulted` are valid local out-parameters.
    let group_read = unsafe {
        GetSecurityDescriptorGroup(descriptor_pointer, &raw mut group, &raw mut defaulted)
    };
    // SAFETY: the same captured descriptor stays alive for this call, and
    // `dacl_present`/`dacl`/`defaulted` are valid local out-parameters.
    let dacl_read = unsafe {
        GetSecurityDescriptorDacl(
            descriptor_pointer,
            &raw mut dacl_present,
            &raw mut dacl,
            &raw mut defaulted,
        )
    };
    // SAFETY: the same captured descriptor stays alive for this call, and
    // `control`/`revision` are valid local out-parameters.
    let control_read = unsafe {
        GetSecurityDescriptorControl(descriptor_pointer, &raw mut control, &raw mut revision)
    };
    // D39: all four accessor statuses are aggregated, so any zero refuses the
    // transition before the DACL/protection derivation below.
    let read = owner_read & group_read & dacl_read & control_read;
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
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle(), &raw mut information) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(u64::from(information.nNumberOfLinks))
}

/// Create the staged document file with no sharing, so no other principal can
/// open it between creation and the policy transition.
///
/// The handle also carries `WRITE_OWNER`, which the owner/group components of
/// the later `SetSecurityInfo` transition require; without it that documented
/// call fails on the handle this file was just created with.
pub fn open_staging_file(path: &Path) -> io::Result<File> {
    let path = wide_path(path);
    // SAFETY: the path is NUL-terminated, the security attributes are absent (the
    // file inherits the private staging directory's DACL) and no template handle
    // is needed.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            GENERIC_WRITE | WRITE_DAC | WRITE_OWNER | READ_CONTROL,
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

/// The two documented ACE classes (`winnt.h`) the credential privacy gate can
/// interpret; windows-sys 0.61 does not declare these `#define` values.
const ACCESS_ALLOWED_ACE_TYPE: u8 = 0x0;
const ACCESS_DENIED_ACE_TYPE: u8 = 0x1;

/// Decide whether the open file's DACL grants any access right to a principal
/// beyond the file's owner, LOCAL SYSTEM and the Administrators group.
///
/// `Ok(())` means every allow grant belongs to one of those three principals.
/// The memory credential loader calls this with its already-open handle before
/// accepting the file's bytes, so a credential sharing *any* ordinary right
/// with another principal — read, but also nonread rights such as `WRITE_DAC`,
/// which would let that principal rewrite the DACL and grant itself read
/// later — or an uninterpretable policy is refused: an allow grant to any
/// other SID, or a missing/null DACL or an owner that cannot be compared, is
/// [`io::ErrorKind::PermissionDenied`]; a DACL carrying an ACE class this
/// checker cannot interpret (object, callback, audit, alarm and label ACEs are
/// not grants this loader can reason about) is [`io::ErrorKind::Unsupported`].
/// Denied ACEs only narrow access and are not grants.
///
/// The query follows the documented `GetKernelObjectSecurity` two-call
/// contract against the caller's handle, so no path is reopened between the
/// open and the decision and nothing mutates the policy.
pub fn grants_only_to_trusted_principals(file: &File) -> io::Result<()> {
    let mut needed = 0_u32;
    // SAFETY: the measuring form takes a null descriptor with a zero length and
    // a valid out-parameter; it is expected to fail with the required size.
    unsafe {
        GetKernelObjectSecurity(
            file.as_raw_handle(),
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            0,
            &raw mut needed,
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
    // words) and the call writes at most that many, reporting them back
    // through `needed`.
    let written = unsafe {
        GetKernelObjectSecurity(
            file.as_raw_handle(),
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            descriptor.as_mut_ptr().cast(),
            length,
            &raw mut needed,
        )
    };
    if written == 0 {
        return Err(io::Error::last_os_error());
    }

    let descriptor_pointer: PSECURITY_DESCRIPTOR = descriptor.as_mut_ptr().cast();
    let mut owner: PSID = ptr::null_mut();
    let mut defaulted = 0_i32;
    // SAFETY: `descriptor_pointer` addresses the captured self-relative
    // descriptor and `owner`/`defaulted` are valid local out-parameters.
    let owner_read = unsafe {
        GetSecurityDescriptorOwner(descriptor_pointer, &raw mut owner, &raw mut defaulted)
    };
    let mut dacl: *mut ACL = ptr::null_mut();
    let mut dacl_present = 0_i32;
    // SAFETY: the same captured descriptor stays alive for this call, and
    // `dacl_present`/`dacl`/`defaulted` are valid local out-parameters.
    let dacl_read = unsafe {
        GetSecurityDescriptorDacl(
            descriptor_pointer,
            &raw mut dacl_present,
            &raw mut dacl,
            &raw mut defaulted,
        )
    };
    if owner_read & dacl_read == 0 {
        return Err(io::Error::last_os_error());
    }
    if owner.is_null() || dacl_present == 0 || dacl.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "credential file has no verifiable owner or DACL",
        ));
    }

    let mut information = ACL_SIZE_INFORMATION::default();
    let information_size = u32::try_from(std::mem::size_of::<ACL_SIZE_INFORMATION>())
        .expect("ACL_SIZE_INFORMATION fits in a Win32 buffer");
    // SAFETY: the non-null DACL was returned from the live descriptor; the
    // aligned output buffer holds the requested information class.
    let queried = unsafe {
        GetAclInformation(
            dacl,
            (&raw mut information).cast(),
            information_size,
            AclSizeInformation,
        )
    };
    if queried == 0 {
        return Err(io::Error::last_os_error());
    }

    for index in 0..information.AceCount {
        let mut ace: *mut c_void = ptr::null_mut();
        // SAFETY: `index` is within the documented `AceCount` of the live DACL
        // and the out-pointer is a valid local.
        let got = unsafe { GetAce(dacl, index, &raw mut ace) };
        if got == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `GetAce` succeeded for an index below the documented
        // `AceCount` of the DACL captured above, so `ace` is the address of a
        // live ACE record inside that DACL, which stays alive for this whole
        // function. Per the documented layout, `ACE_HEADER` is the first
        // member of every ACE structure and each ACE begins on a DWORD
        // boundary, so this four-byte, four-aligned header read is in extent.
        let header = unsafe { &*ace.cast::<ACE_HEADER>() };
        match header.AceType {
            // A denied ACE only narrows access; it grants nobody anything.
            ACCESS_DENIED_ACE_TYPE => {}
            ACCESS_ALLOWED_ACE_TYPE => {
                // A record whose documented `AceSize` does not cover the full
                // `ACCESS_ALLOWED_ACE` field layout cannot be classified:
                // refuse it instead of reinterpreting a truncated record.
                if (header.AceSize as usize) < std::mem::size_of::<ACCESS_ALLOWED_ACE>() {
                    return Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "credential file carries a truncated allowed-access entry",
                    ));
                }
                // SAFETY: an allowed ACE whose documented `AceSize` field
                // covers the whole `ACCESS_ALLOWED_ACE` field layout (header,
                // mask and the first SID dword) is read here, so the twelve
                // bytes of that layout are within the record's extent; the
                // record is DWORD-aligned per the documented ACE layout and
                // lives inside the DACL captured above. `SidStart` addresses
                // the trustee SID that the documented layout stores
                // contiguously after the mask.
                let allowed = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
                let sid = ptr::from_ref(&allowed.SidStart).cast_mut().cast::<c_void>();
                if !sid_is_trusted(sid, owner) {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "credential file grants access beyond the owner, \
                         SYSTEM and Administrators",
                    ));
                }
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "credential file carries an access-control entry class this \
                     checker cannot interpret",
                ));
            }
        }
    }
    Ok(())
}

/// Whether the SID is the file's owner, LOCAL SYSTEM or the Administrators
/// group — the three principals a credential file may grant access to.
fn sid_is_trusted(sid: PSID, owner: PSID) -> bool {
    // SAFETY: `EqualSid` documents its two operands as SID structures that are
    // assumed to be valid and undefined for invalid ones; both pointers
    // address OS-produced SIDs (the captured owner and the walked ACE's
    // contiguous SID) inside the live descriptor, and the call is synchronous.
    unsafe { EqualSid(sid, owner) != 0 }
        // SAFETY: the same live OS-produced ACE SID compared against the
        // documented well-known SYSTEM SID class; the descriptor stays alive
        // and the call is synchronous.
        || unsafe { IsWellKnownSid(sid, WinLocalSystemSid) != 0 }
        // SAFETY: the same live OS-produced ACE SID compared against the
        // documented Administrators SID class; the descriptor stays alive and
        // the call is synchronous.
        || unsafe { IsWellKnownSid(sid, WinBuiltinAdministratorsSid) != 0 }
}

/// NUL-terminated UTF-16 form of a path for the wide Win32 APIs.
fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}
