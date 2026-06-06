use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;

use widestring::U16CString;

pub(crate) fn mountpoint_to_u16(mountpoint: &OsStr) -> io::Result<U16CString> {
    U16CString::new(mountpoint.encode_wide().collect::<Vec<u16>>())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid mountpoint"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    #[test]
    fn mountpoint_to_u16_valid_path_succeeds() {
        let result = mountpoint_to_u16(OsStr::new("."));
        assert!(result.is_ok());
    }

    #[test]
    fn mountpoint_to_u16_non_empty_path_roundtrips() {
        let path = OsStr::new("C:\\mount");
        let result = mountpoint_to_u16(path);
        assert!(result.is_ok());
        let u16str = result.expect("ok");
        let encoded: Vec<u16> = path.encode_wide().collect();
        assert_eq!(u16str.as_slice(), encoded.as_slice());
    }
}
