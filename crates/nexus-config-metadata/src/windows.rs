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
    IsWellKnownSid, IsValidSid, WinBuiltinAdministratorsSid, WinLocalSystemSid,
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_SIZE_INFORMATION, ATTRIBUTE_SECURITY_INFORMATION,
    DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION, LABEL_SECURITY_INFORMATION,
    OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
    SCOPE_SECURITY_INFORMATION, SE_DACL_PROTECTED, SID, UNPROTECTED_DACL_SECURITY_INFORMATION,
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

/// Read the advertised subauthority count of a SID — the single bounded
/// fixed-SID safety-metadata access (D45): exactly one byte at the
/// documented count offset, read only to size the SID for its enclosure
/// check. No identifier, subauthority or other content byte is read.
///
/// # SAFETY (caller obligations)
///
/// `sid` addresses a live SID structure whose complete fixed prefix — the
/// whole `std::mem::size_of::<SID>()` representation (revision, count,
/// identifier authority, one subauthority slot) — has been proven readable
/// inside the enclosing extent by the caller's bounds checks, and the
/// enclosing object outlives this call. Only the one count byte at the
/// documented offset is dereferenced.
fn advertised_subauthority_count(sid: PSID) -> u32 {
    // SAFETY: the caller proved the complete fixed-prefix extent above; this
    // reads exactly the one count byte at its documented offset (no alignment
    // requirement, no content or subauthority bytes), within the live
    // enclosing object.
    unsafe { u32::from(std::ptr::read((sid as *const u8).add(1))) }
}

/// The complete byte length of a SID advertising `count` subauthorities:
/// the fixed prefix of the `SID` representation (everything but its one
/// subauthority slot) plus four bytes per advertised subauthority. `None`
/// when the advertised count cannot yield a bounded length.
fn sid_required_length(count: u32) -> Option<u32> {
    let fixed_prefix = (std::mem::size_of::<SID>() - std::mem::size_of::<u32>()) as u32;
    fixed_prefix.checked_add(count.checked_mul(4)?)
}

/// Whether a record of `record_extent` bytes encloses its fixed fields
/// (header and access mask) plus the complete SID advertised by `count`.
/// An unbounded advertised length never encloses.
fn record_encloses_sid(record_extent: u32, count: u32) -> bool {
    let fixed_fields = (std::mem::size_of::<ACE_HEADER>() + std::mem::size_of::<u32>()) as u32;
    sid_required_length(count)
        .is_some_and(|sid_length| fixed_fields.checked_add(sid_length) <= Some(record_extent))
}

/// Decide whether the open file's DACL grants any access right to a principal
/// beyond the file's owner, LOCAL SYSTEM and the Administrators group.
///
/// `Ok(())` means every allow grant belongs to one of those three principals.
/// Before any comparison, both comparison operands — the descriptor's owner
/// SID and each walked ACE's trustee SID — pass a complete enclosure chain:
/// fixed-prefix readability, the bounded count metadata read (D45), the
/// full count-dependent SID extent against its enclosing returned bytes
/// (descriptor or record), and the native `IsValidSid`; an unenclosed,
/// truncated, invalid or uninterpretable record refuses the credential.
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
    // The owner SID lives inside the returned descriptor: its readable extent
    // ends at the bytes the second query actually wrote (`needed`), not at
    // the allocation's spare capacity or padding.
    let descriptor_end = (descriptor.as_ptr() as usize).checked_add(needed as usize);
    let owner_start = owner as usize;
    // Fixed-prefix readability before the metadata read: the whole minimal
    // `SID` representation (revision, count, identifier authority, one
    // subauthority slot) must lie inside the returned bytes.
    if !descriptor_end.is_some_and(|end| {
        owner_start
            .checked_add(std::mem::size_of::<SID>())
            .is_some_and(|fixed_end| fixed_end <= end)
    }) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "credential file has an invalid owner security identifier",
        ));
    }
    // Bounded safety metadata (D45), then the complete count-dependent
    // enclosure, before any unbounded SID validator or comparison.
    let owner_count = advertised_subauthority_count(owner);
    if !descriptor_end.is_some_and(|end| {
        sid_required_length(owner_count)
            .is_some_and(|required| {
                owner_start
                    .checked_add(required as usize)
                    .is_some_and(|sid_end| sid_end <= end)
            })
    }) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "credential file has an invalid owner security identifier",
        ));
    }
    // Semantic validity before any comparison: the owner SID must be a valid
    // SID structure, or the credential policy is unverifiable and refuses.
    // SAFETY: `IsValidSid` validates the SID structure at `owner` and
    // documents a non-null pointer as its precondition; `owner` passed the
    // non-null check above, its complete advertised representation was proved
    // enclosed in the returned descriptor bytes, and the descriptor stays
    // alive for this call.
    if unsafe { IsValidSid(owner) } == 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "credential file has an invalid owner security identifier",
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
                // The fixed-prefix guard: a record whose documented
                // `AceSize` does not cover the header, the mask and the
                // complete minimal `SID` structure (revision, subauthority
                // count, identifier authority and one subauthority slot)
                // cannot be classified and refuses. The record's full
                // count-dependent enclosure is checked after the bounded
                // count read below.
                let needed_extent = std::mem::size_of::<ACE_HEADER>()
                    + std::mem::size_of::<u32>()
                    + std::mem::size_of::<SID>();
                if (header.AceSize as usize) < needed_extent {
                    return Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "credential file carries a truncated allowed-access entry",
                    ));
                }
                // SAFETY: an allowed ACE whose documented `AceSize` field
                // covers the header, the mask and the complete minimal `SID`
                // representation is read here, so those bytes are within the
                // record's extent; the record is DWORD-aligned per the
                // documented ACE layout and lives inside the DACL captured
                // above. `SidStart` addresses the trustee SID that the
                // documented layout stores contiguously after the mask, so
                // the fixed SID structure is readable before validation.
                let allowed = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
                let sid = ptr::from_ref(&allowed.SidStart).cast_mut().cast::<c_void>();
                // Bounded safety metadata (D45): the count byte's fixed
                // prefix was proved readable by the extent guard above, and
                // only then is the complete count-dependent enclosure
                // checked, before any unbounded SID validator or comparison.
                let trustee_count = advertised_subauthority_count(sid);
                if !record_encloses_sid(header.AceSize as u32, trustee_count) {
                    return Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "credential file carries a truncated allowed-access entry",
                    ));
                }
                // Semantic validity before any comparison: the trustee SID
                // must be a valid SID structure, or the credential policy is
                // unverifiable and refuses.
                // SAFETY: `IsValidSid` validates the SID structure at `sid`
                // and documents a non-null pointer as its precondition; `sid`
                // addresses the ACE's contiguous trustee SID, whose complete
                // advertised representation the enclosure check above proved
                // inside the record's documented `AceSize` extent, and the
                // descriptor stays alive for this call.
                if unsafe { IsValidSid(sid) } == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "credential file carries an access-control entry with \
                         an invalid security identifier",
                    ));
                }
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
///
/// Both operands must already have passed the native `IsValidSid` check at
/// the call sites above; `EqualSid` documents its operands as SID structures
/// assumed to be valid.
fn sid_is_trusted(sid: PSID, owner: PSID) -> bool {
    // SAFETY: both pointers address SID structures the `IsValidSid` calls
    // above validated, inside the live captured descriptor; `EqualSid`
    // documents its operands as valid SID structures and is synchronous.
    unsafe { EqualSid(sid, owner) != 0 }
        // SAFETY: the IsValidSid-validated ACE SID compared against the
        // documented well-known SYSTEM SID class; the descriptor stays alive
        // and the call is synchronous.
        || unsafe { IsWellKnownSid(sid, WinLocalSystemSid) != 0 }
        // SAFETY: the same IsValidSid-validated ACE SID compared against the
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

#[cfg(test)]
mod sid_bounds_tests {
    use super::{record_encloses_sid, sid_required_length};

    /// A record whose fixed storage fits but whose advertised variable tail
    /// does not must be refused: one-subauthority storage does not enclose a
    /// SID advertising a second subauthority.
    #[test]
    fn sid_bounds_fixed_fit_does_not_enclose_longer_advertised_tail() {
        let fixed_fit = (std::mem::size_of::<ACE_HEADER>()
            + std::mem::size_of::<u32>()
            + std::mem::size_of::<SID>()) as u32;
        assert!(record_encloses_sid(fixed_fit, 1), "exactly the fixed fit encloses one subauthority");
        assert!(
            !record_encloses_sid(fixed_fit, 2),
            "the same extent must refuse a SID advertising a second subauthority"
        );
    }

    /// The exact enclosing boundary is accepted: header + mask + the SID's
    /// complete advertised length, and not a byte less.
    #[test]
    fn sid_bounds_exact_enclosing_boundary_is_accepted_and_shorter_refused() {
        let two_subauthorities = sid_required_length(2).expect("a bounded advertised length");
        let exact = (std::mem::size_of::<ACE_HEADER>() + std::mem::size_of::<u32>()) as u32
            + two_subauthorities;
        assert!(record_encloses_sid(exact, 2), "the exact enclosing extent encloses");
        assert!(
            !record_encloses_sid(exact - 1, 2),
            "one byte short of the advertised tail must refuse"
        );
    }

    /// The advertised length is the fixed prefix plus four bytes per
    /// subauthority, bounded even at the maximum representable count.
    #[test]
    fn sid_bounds_required_length_is_bounded_per_subauthority() {
        let fixed_prefix = (std::mem::size_of::<SID>() - std::mem::size_of::<u32>()) as u32;
        assert_eq!(sid_required_length(0), Some(fixed_prefix));
        assert_eq!(sid_required_length(1), Some(fixed_prefix + 4));
        assert_eq!(sid_required_length(255), Some(fixed_prefix + 255 * 4));
    }

    /// An unbounded advertised length (overflow path) never encloses,
    /// regardless of the record extent.
    #[test]
    fn sid_bounds_unbounded_advertised_length_never_encloses() {
        assert!(!record_encloses_sid(u32::MAX, u32::MAX));
    }
}
