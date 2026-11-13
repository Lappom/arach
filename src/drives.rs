//! The disks games live on: capacity, free space and volume name.

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drive {
    /// `"D:"`
    pub letter: String,
    pub label: String,
    pub total: u64,
    pub free: u64,
}

/// Every fixed drive with a letter.
pub fn list() -> Vec<Drive> {
    (b'C'..=b'Z').filter_map(|c| info(&format!("{}:", c as char))).collect()
}

#[cfg(windows)]
fn info(letter: &str) -> Option<Drive> {
    use windows_sys::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetVolumeInformationW};
    const DRIVE_FIXED: u32 = 3;
    let root: Vec<u16> = format!("{letter}\\").encode_utf16().chain(Some(0)).collect();
    let (mut free, mut total, mut _avail) = (0u64, 0u64, 0u64);
    let mut name = [0u16; 128];
    // SAFETY: `root` is NUL-terminated and outlives the calls; the out pointers are valid.
    unsafe {
        if GetDriveTypeW(root.as_ptr()) != DRIVE_FIXED { return None; }
        if GetDiskFreeSpaceExW(root.as_ptr(), &mut _avail, &mut total, &mut free) == 0 { return None; }
        GetVolumeInformationW(
            root.as_ptr(),
            name.as_mut_ptr(),
            name.len() as u32,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        );
    }
    let len = name.iter().position(|&c| c == 0).unwrap_or(0);
    Some(Drive { letter: letter.into(), label: String::from_utf16_lossy(&name[..len]), total, free })
}

#[cfg(not(windows))]
fn info(_: &str) -> Option<Drive> {
    None
}
