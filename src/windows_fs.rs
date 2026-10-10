//! Windows filesystem operations with checked ownership and non-following handles.
use anyhow::{Context, Result, bail};
use std::{
    ffi::OsStr,
    fs::{self, File, Metadata},
    io,
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::{AsRawHandle, FromRawHandle},
    },
    path::Path,
    ptr,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, INVALID_HANDLE_VALUE, LocalFree},
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(Some(0)).collect()
}
fn checked(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
struct Local(*mut core::ffi::c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
struct Token(windows_sys::Win32::Foundation::HANDLE);
impl Drop for Token {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn current_user() -> Result<Vec<usize>> {
    let mut token = ptr::null_mut();
    unsafe {
        checked(OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_QUERY,
            &mut token,
        ))?;
    }
    let token = Token(token);
    let mut size = 0;
    unsafe {
        GetTokenInformation(token.0, TokenUser, ptr::null_mut(), 0, &mut size);
    }
    if size == 0 {
        bail!("current user token unavailable");
    }
    let mut bytes = vec![0usize; (size as usize).div_ceil(size_of::<usize>())];
    unsafe {
        checked(GetTokenInformation(
            token.0,
            TokenUser,
            bytes.as_mut_ptr().cast(),
            size,
            &mut size,
        ))?;
    }
    Ok(bytes)
}
fn user_sid(user: &[usize]) -> PSID {
    unsafe { (*(user.as_ptr().cast::<TOKEN_USER>())).User.Sid }
}
fn private_descriptor() -> Result<Local> {
    let user = current_user()?;
    let mut text = ptr::null_mut();
    unsafe {
        checked(ConvertSidToStringSidW(user_sid(&user), &mut text))?;
    }
    let text_guard = Local(text.cast());
    let mut length = 0;
    unsafe {
        while *text.add(length) != 0 {
            length += 1;
        }
    }
    let sid = String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) })?;
    let descriptor_text = wide(OsStr::new(&format!("O:{sid}D:P(A;OICI;FA;;;{sid})")));
    let mut descriptor = ptr::null_mut();
    unsafe {
        checked(ConvertStringSecurityDescriptorToSecurityDescriptorW(
            descriptor_text.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            ptr::null_mut(),
        ))?;
    }
    drop(text_guard);
    Ok(Local(descriptor))
}
fn attributes(descriptor: &Local) -> SECURITY_ATTRIBUTES {
    SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    }
}
pub(crate) fn create_private_directory(path: &Path) -> Result<()> {
    let descriptor = private_descriptor()?;
    unsafe {
        checked(CreateDirectoryW(
            wide(path.as_os_str()).as_ptr(),
            &attributes(&descriptor),
        ))?;
    }
    Ok(())
}
pub(crate) fn create_private_file(path: &Path) -> Result<File> {
    let descriptor = private_descriptor()?;
    let raw = unsafe {
        CreateFileW(
            wide(path.as_os_str()).as_ptr(),
            FILE_GENERIC_WRITE | READ_CONTROL,
            0,
            &attributes(&descriptor),
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            ptr::null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error().into());
    }
    Ok(unsafe { File::from_raw_handle(raw) })
}
pub(crate) fn is_reparse(metadata: &Metadata) -> bool {
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}
pub(crate) fn mode(metadata: &Metadata) -> u32 {
    if metadata.permissions().readonly() {
        0o444
    } else {
        0o666
    }
}
const DIRECTORY_READONLY_ATTRIBUTE: u32 = 1 << 16;
pub(crate) fn mode_for_path(path: &Path, metadata: &Metadata) -> Result<u32> {
    if !metadata.is_dir() {
        return Ok(mode(metadata));
    }
    let file = open_owned(path, false)?;
    let mut descriptor = ptr::null_mut();
    let result = unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            &mut descriptor,
        )
    };
    if result != 0 {
        return Err(io::Error::from_raw_os_error(result as i32).into());
    }
    let _descriptor = Local(descriptor);
    let mut raw = ptr::null_mut();
    unsafe {
        checked(OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_QUERY | TOKEN_DUPLICATE,
            &mut raw,
        ))?;
    }
    let token = Token(raw);
    let mut raw = ptr::null_mut();
    unsafe {
        checked(DuplicateToken(token.0, SecurityImpersonation, &mut raw))?;
    }
    let impersonation = Token(raw);
    let mapping = GENERIC_MAPPING {
        GenericRead: FILE_GENERIC_READ,
        GenericWrite: FILE_GENERIC_WRITE,
        GenericExecute: FILE_GENERIC_EXECUTE,
        GenericAll: FILE_ALL_ACCESS,
    };
    let mut mode = 0o444;
    for (access, bit) in [
        (FILE_ADD_FILE, 0o200),
        (FILE_ADD_SUBDIRECTORY, 0o020),
        (FILE_DELETE_CHILD, 0o002),
    ] {
        let mut privileges = [0usize; 256];
        let mut size = size_of_val(&privileges) as u32;
        let mut granted = 0;
        let mut allowed = 0;
        unsafe {
            checked(AccessCheck(
                descriptor,
                impersonation.0,
                access,
                &mapping,
                privileges.as_mut_ptr().cast(),
                &mut size,
                &mut granted,
                &mut allowed,
            ))?;
        }
        if allowed != 0 {
            mode |= bit;
        }
    }
    if metadata.permissions().readonly() {
        mode |= DIRECTORY_READONLY_ATTRIBUTE;
    }
    Ok(mode)
}
pub(crate) fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    let mut permissions = metadata.permissions();
    if metadata.is_dir() {
        permissions.set_readonly(mode & DIRECTORY_READONLY_ATTRIBUTE != 0);
        fs::set_permissions(path, permissions)?;
        set_directory_access(path, mode).map_err(|error| io::Error::other(error.to_string()))?;
    } else {
        permissions.set_readonly(mode & 0o222 == 0);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}
pub(crate) fn same_snapshot(left: &Metadata, right: &Metadata) -> bool {
    left.file_size() == right.file_size()
        && left.last_write_time() == right.last_write_time()
        && left.creation_time() == right.creation_time()
        && left.file_attributes() == right.file_attributes()
}
pub(crate) fn identity(file: &File) -> Result<(u64, [u8; 16])> {
    let mut info = FILE_ID_INFO::default();
    unsafe {
        checked(GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileIdInfo,
            (&mut info as *mut FILE_ID_INFO).cast(),
            size_of::<FILE_ID_INFO>() as u32,
        ))?;
    }
    Ok((info.VolumeSerialNumber, info.FileId.Identifier))
}
// Denying delete sharing holds the opened path component in place until close.
pub(crate) fn open_owned(path: &Path, delete: bool) -> Result<File> {
    let raw = unsafe {
        CreateFileW(
            wide(path.as_os_str()).as_ptr(),
            FILE_READ_ATTRIBUTES
                | FILE_LIST_DIRECTORY
                | READ_CONTROL
                | if delete {
                    DELETE | FILE_WRITE_ATTRIBUTES
                } else {
                    0
                },
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            ptr::null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error().into());
    }
    Ok(unsafe { File::from_raw_handle(raw) })
}
pub(crate) fn open_regular(path: &Path) -> Result<File> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .share_mode(FILE_SHARE_READ)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || is_reparse(&metadata) {
        bail!("assigned file must be regular and cannot be a reparse point");
    }
    Ok(file)
}
pub(crate) fn validate_private(file: &File) -> Result<()> {
    let user = current_user()?;
    let mut owner = ptr::null_mut();
    let mut acl = ptr::null_mut();
    let mut descriptor = ptr::null_mut();
    let result = unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            &mut acl,
            ptr::null_mut(),
            &mut descriptor,
        )
    };
    if result != 0 {
        return Err(io::Error::from_raw_os_error(result as i32).into());
    }
    let _descriptor = Local(descriptor);
    if owner.is_null()
        || acl.is_null()
        || unsafe { EqualSid(owner, user_sid(&user)) } == 0
        || unsafe { (*acl).AceCount } != 1
    {
        bail!("state must be private and owned by the current user");
    }
    let mut ace = ptr::null_mut();
    unsafe {
        checked(GetAce(acl, 0, &mut ace))?;
    }
    let ace = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
    if ace.Header.AceType != 0
        || ace.Mask & FILE_ALL_ACCESS != FILE_ALL_ACCESS
        || unsafe {
            EqualSid(
                (&ace.SidStart as *const u32).cast_mut().cast(),
                user_sid(&user),
            )
        } == 0
    {
        bail!("state access must be restricted to the current user");
    }
    Ok(())
}
pub(crate) fn open_lock(path: &Path) -> Result<File> {
    if !path.exists() {
        match create_private_file(path) {
            Ok(file) => drop(file),
            Err(error)
                if error
                    .downcast_ref::<io::Error>()
                    .is_some_and(|e| e.kind() == io::ErrorKind::AlreadyExists) => {}
            Err(error) => return Err(error),
        }
    }
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .context("mutation state is locked or unavailable")?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || is_reparse(&metadata) {
        bail!("state lock must be a private regular file");
    }
    validate_private(&file)?;
    Ok(file)
}
pub(crate) fn replace(temporary: &Path, destination: &Path) -> Result<()> {
    unsafe {
        checked(MoveFileExW(
            wide(temporary.as_os_str()).as_ptr(),
            wide(destination.as_os_str()).as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        ))?;
    }
    Ok(())
}
pub(crate) fn remove_handle(file: &File) -> Result<()> {
    let info = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_FLAG_DELETE
            | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS
            | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE,
    };
    unsafe {
        checked(SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfoEx,
            (&info as *const FILE_DISPOSITION_INFO_EX).cast(),
            size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
        ))?;
    }
    Ok(())
}

fn set_directory_access(path: &Path, mode: u32) -> Result<()> {
    let user = current_user()?;
    let mut text = ptr::null_mut();
    unsafe {
        checked(ConvertSidToStringSidW(user_sid(&user), &mut text))?;
    }
    let _text = Local(text.cast());
    let mut length = 0;
    unsafe {
        while *text.add(length) != 0 {
            length += 1;
        }
    }
    let sid = String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) })?;
    let mut mask = FILE_ALL_ACCESS;
    for (access, bit) in [
        (FILE_ADD_FILE, 0o200),
        (FILE_ADD_SUBDIRECTORY, 0o020),
        (FILE_DELETE_CHILD, 0o002),
    ] {
        if mode & bit == 0 {
            mask &= !access;
        }
    }
    // Files retain inherited private access. This restriction applies to the directory itself.
    let text = wide(OsStr::new(&format!(
        "D:P(A;;0x{mask:x};;;{sid})(A;OICIIO;FA;;;{sid})"
    )));
    let mut descriptor = ptr::null_mut();
    unsafe {
        checked(ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            ptr::null_mut(),
        ))?;
    }
    let _descriptor = Local(descriptor);
    let mut present = 0;
    let mut defaulted = 0;
    let mut acl = ptr::null_mut();
    unsafe {
        checked(GetSecurityDescriptorDacl(
            descriptor,
            &mut present,
            &mut acl,
            &mut defaulted,
        ))?;
    }
    let result = unsafe {
        SetNamedSecurityInfoW(
            wide(path.as_os_str()).as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            acl,
            ptr::null_mut(),
        )
    };
    if result != 0 {
        return Err(io::Error::from_raw_os_error(result as i32).into());
    }
    Ok(())
}

pub(crate) fn hold_ancestors(path: &Path) -> Result<Vec<File>> {
    let mut prefix = std::path::PathBuf::new();
    let mut handles = Vec::new();
    for component in path.components() {
        prefix.push(component.as_os_str());
        if matches!(component, std::path::Component::Prefix(_)) {
            continue;
        }
        if matches!(component, std::path::Component::ParentDir) || handles.len() >= 256 {
            bail!("unsafe or excessive directory traversal");
        }
        let file = open_owned(&prefix, false)?;
        let metadata = file.metadata()?;
        if !metadata.is_dir() || is_reparse(&metadata) {
            bail!("owned directory ancestor is a reparse point or non-directory");
        }
        handles.push(file);
    }
    Ok(handles)
}

pub(crate) fn create_junction(link: &Path, destination: &Path) -> Result<()> {
    let destination = std::path::absolute(destination)?;
    let text = destination
        .to_str()
        .context("junction target must be UTF-8")?;
    let plain = text.strip_prefix(r"\\?\").unwrap_or(text);
    let native = if let Some(unc) = plain.strip_prefix(r"\\") {
        format!(r"\??\UNC\{unc}")
    } else if let Some(unc) = plain.strip_prefix(r"UNC\") {
        format!(r"\??\UNC\{unc}")
    } else {
        format!(r"\??\{plain}")
    };
    let substitute: Vec<u16> = OsStr::new(&native).encode_wide().collect();
    let print: Vec<u16> = destination.as_os_str().encode_wide().collect();
    let mut bytes = Vec::new();
    let data_length = 8usize
        .checked_add((substitute.len() + print.len() + 2) * 2)
        .context("junction data overflow")?;
    let data_length =
        u16::try_from(data_length).context("junction target exceeds Windows reparse limit")?;
    bytes.extend_from_slice(&0xA0000003u32.to_le_bytes());
    bytes.extend_from_slice(&data_length.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&u16::try_from(substitute.len() * 2)?.to_le_bytes());
    bytes.extend_from_slice(&u16::try_from((substitute.len() + 1) * 2)?.to_le_bytes());
    bytes.extend_from_slice(&u16::try_from(print.len() * 2)?.to_le_bytes());
    for unit in substitute
        .into_iter()
        .chain(Some(0))
        .chain(print)
        .chain(Some(0))
    {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    fs::create_dir(link)?;
    let outcome = (|| -> Result<()> {
        let mut options = fs::OpenOptions::new();
        options
            .write(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .share_mode(0);
        let handle = options.open(link)?;
        let mut returned = 0;
        unsafe {
            checked(windows_sys::Win32::System::IO::DeviceIoControl(
                handle.as_raw_handle(),
                0x000900A4,
                bytes.as_ptr().cast(),
                bytes.len() as u32,
                ptr::null_mut(),
                0,
                &mut returned,
                ptr::null_mut(),
            ))?;
        }
        Ok(())
    })();
    if outcome.is_err() {
        let _ = fs::remove_dir(link);
    }
    outcome
}

pub(crate) fn check_cleanup_capability(directory: &Path) -> Result<()> {
    probe_cleanup_capability(directory, remove_handle)
}
fn remove_probe_handle(file: &File) -> Result<()> {
    let mut attributes = file.metadata()?.file_attributes() & !FILE_ATTRIBUTE_READONLY;
    if attributes == 0 {
        attributes = FILE_ATTRIBUTE_NORMAL;
    }
    let info = FILE_BASIC_INFO {
        CreationTime: 0,
        LastAccessTime: 0,
        LastWriteTime: 0,
        ChangeTime: 0,
        FileAttributes: attributes,
    };
    unsafe {
        checked(SetFileInformationByHandle(
            file.as_raw_handle(),
            FileBasicInfo,
            (&info as *const FILE_BASIC_INFO).cast(),
            size_of::<FILE_BASIC_INFO>() as u32,
        ))?;
    }
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    unsafe {
        checked(SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfo,
            (&disposition as *const FILE_DISPOSITION_INFO).cast(),
            size_of::<FILE_DISPOSITION_INFO>() as u32,
        ))?;
    }
    Ok(())
}
fn probe_cleanup_capability(directory: &Path, remove: impl Fn(&File) -> Result<()>) -> Result<()> {
    let scratch = directory.join(".archguard-cleanup-probe");
    create_private_directory(&scratch)?;
    let directory_handle = open_owned(&scratch, true)?;
    let path = scratch.join("readonly");
    drop(create_private_file(&path)?);
    set_mode(&path, 0o444)?;
    let file = open_owned(&path, true)?;
    let result = identity(&file)
        .and_then(|_| identity(&directory_handle))
        .and_then(|_| remove(&file));
    if let Err(error) = result {
        // The compatibility probe contains only these owned handles. Older APIs can remove them safely.
        remove_probe_handle(&file)?;
        drop(file);
        remove_probe_handle(&directory_handle)?;
        drop(directory_handle);
        return Err(error.context("mutation execution requires Windows 10 1809 / Server 2019 or newer and filesystem support for stable IDs and readonly handle deletion"));
    }
    drop(file);
    if path.exists() {
        bail!("Windows cleanup probe did not remove its readonly file");
    }
    if let Err(error) = remove(&directory_handle) {
        remove_probe_handle(&directory_handle)?;
        drop(directory_handle);
        return Err(error.context("Windows filesystem cannot remove owned directories by handle"));
    }
    drop(directory_handle);
    if scratch.exists() {
        bail!("Windows cleanup probe did not remove its owned directory");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::windows_fs::*;
    #[test]
    fn private_acl_rejects_shared_directory_and_accepts_owned_directory() {
        let root = tempfile::tempdir().unwrap();
        let private = root.path().join("private state");
        create_private_directory(&private).unwrap();
        validate_private(&open_owned(&private, false).unwrap()).unwrap();
        let shared = root.path().join("shared");
        fs::create_dir(&shared).unwrap();
        assert!(validate_private(&open_owned(&shared, false).unwrap()).is_err());
        let lock = private.join("lock");
        let held = open_lock(&lock).unwrap();
        assert!(open_lock(&lock).is_err());
        drop(held);
        open_lock(&lock).unwrap();
    }
    #[test]
    fn regular_handle_identity_is_stable_and_excludes_reparse_points() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("input.ts");
        let other = root.path().join("other.ts");
        fs::write(&path, b"same bytes").unwrap();
        fs::write(&other, b"same bytes").unwrap();
        let file = open_regular(&path).unwrap();
        assert_eq!(
            identity(&file).unwrap(),
            identity(&open_regular(&path).unwrap()).unwrap()
        );
        assert_ne!(
            identity(&file).unwrap(),
            identity(&open_regular(&other).unwrap()).unwrap()
        );
        assert!(fs::rename(&path, root.path().join("moved.ts")).is_err());
        assert!(fs::write(&path, b"changed").is_err());
        drop(file);
        fs::write(&path, b"changed").unwrap();
        let target = root.path().join("directory");
        fs::create_dir(&target).unwrap();
        let junction = root.path().join("junction");
        create_junction(&junction, &target).unwrap();
        assert!(is_reparse(&fs::symlink_metadata(&junction).unwrap()));
        assert!(open_regular(&junction).is_err());
        fs::remove_dir(junction).unwrap();
    }
    #[test]
    fn unsupported_cleanup_probe_fails_before_work_and_corrected_probe_cleans_up() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("owned");
        create_private_directory(&directory).unwrap();
        assert!(
            probe_cleanup_capability(&directory, |_| bail!("unsupported deletion control"))
                .is_err()
        );
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
        check_cleanup_capability(&directory).unwrap();
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
    }
    #[test]
    fn readonly_directory_blocks_creation_and_allows_corrected_access() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("directory");
        create_private_directory(&directory).unwrap();
        set_mode(&directory, 0o444).unwrap();
        assert!(fs::write(directory.join("denied"), b"data").is_err());
        set_mode(&directory, 0o666).unwrap();
        fs::write(directory.join("allowed"), b"data").unwrap();
    }
}
