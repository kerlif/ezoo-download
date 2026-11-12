//! Positional writes so many connections can write into one file at once.

use std::fs::File;
use std::io;

/// Write `buf` at `offset` in `file` without moving the file cursor.
#[allow(unused_mut)]
pub fn write_at(file: &File, buf: &[u8], mut offset: u64) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        file.write_all_at(buf, offset)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        let mut rest = buf;
        while !rest.is_empty() {
            let n = file.seek_write(rest, offset)?;
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "failed to write data",
                ));
            }
            rest = &rest[n..];
            offset += n as u64;
        }
        Ok(())
    }
}

/// On NTFS, writing far into a freshly sized file forces Windows to zero-fill
/// everything before it, which stalls big downloads. Marking the file sparse
/// avoids that. Best effort: failures are ignored.
pub fn make_sparse(file: &File) {
    #[cfg(windows)]
    unsafe {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::Ioctl::FSCTL_SET_SPARSE;
        use windows_sys::Win32::System::IO::DeviceIoControl;
        let mut returned: u32 = 0;
        DeviceIoControl(
            file.as_raw_handle() as _,
            FSCTL_SET_SPARSE,
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
        );
    }
    #[cfg(not(windows))]
    let _ = file;
}

/// Refuse a path that is a symbolic link (or, on Unix, any path whose
/// metadata cannot be read without following one). The download state
/// files are ours alone: a link in their place would send the bytes
/// somewhere else — a file the user did not choose, or a file that
/// another program watches. Missing paths are fine: they are created.
pub fn refuse_linked(path: &std::path::Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(md) if md.file_type().is_symlink() => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "refusing to write through a symbolic link: {}",
                path.display()
            ),
        )),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Refuse an already-open file with more than one hard link: another
/// name for the same bytes would let another process read half-written
/// data, or keep the file alive after we delete our name. Unix only;
/// Windows has no cheap equivalent and the check is skipped there.
pub fn check_not_hardlinked(file: &File) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let nlink = file.metadata()?.nlink();
        if nlink > 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "refusing to write a file with more than one hard link",
            ));
        }
    }
    let _ = file;
    Ok(())
}
