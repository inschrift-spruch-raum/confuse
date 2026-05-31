use std::ffi::OsStr;
use std::io;

use crate::fuser_facade::types::MountOption;

// ---------------------------------------------------------------------------
// Mount-option parsing helpers
// ---------------------------------------------------------------------------

pub(crate) fn parse_single_mount_option(s: &str) -> MountOption {
    match s {
        "ro" => MountOption::RO,
        "rw" => MountOption::RW,
        "allow_other" => MountOption::AllowOther,
        "allow_root" => MountOption::AllowRoot,
        "auto_unmount" => MountOption::AutoUnmount,
        "default_permissions" => MountOption::DefaultPermissions,
        "dev" => MountOption::Dev,
        "nodev" => MountOption::NoDev,
        "suid" => MountOption::Suid,
        "nosuid" => MountOption::NoSuid,
        "exec" => MountOption::Exec,
        "noexec" => MountOption::NoExec,
        "atime" => MountOption::Atime,
        "noatime" => MountOption::NoAtime,
        "dirsync" => MountOption::DirSync,
        "sync" => MountOption::Sync,
        "async" => MountOption::Async,
        _ => {
            if let Some(rest) = s.strip_prefix("fsname=") {
                return MountOption::FSName(rest.to_string());
            }
            if let Some(rest) = s.strip_prefix("subtype=") {
                return MountOption::Subtype(rest.to_string());
            }
            MountOption::CUSTOM(s.to_string())
        }
    }
}

pub(crate) fn parse_mount_options_from_args(options: &[&OsStr]) -> io::Result<Vec<MountOption>> {
    let err = |x: String| io::Error::new(io::ErrorKind::InvalidInput, x);
    let args: Option<Vec<_>> = options.iter().map(|x| x.to_str()).collect();
    let args = args.ok_or_else(|| err("Error parsing args: Invalid UTF-8".to_owned()))?;
    let mut it = args.iter();
    let mut out = vec![];
    loop {
        let opt = match it.next() {
            None => break,
            Some(&"-o") => *it.next().ok_or_else(|| {
                err("Error parsing args: Expected option, reached end of args".to_owned())
            })?,
            Some(x) if x.starts_with("-o") => &x[2..],
            Some(x) => x,
        };
        for x in opt.split(',') {
            let trimmed = x.trim();
            if trimmed.is_empty() {
                continue;
            }
            out.push(parse_single_mount_option(trimmed));
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Dokan mount-option mapping
// ---------------------------------------------------------------------------

pub(crate) fn to_dokan_mount_options(
    options: &[MountOption],
) -> io::Result<dokan::MountOptions<'static>> {
    let mut out = dokan::MountOptions::default();
    for opt in options {
        match opt {
            MountOption::RO => {
                out.flags.insert(dokan::MountFlags::WRITE_PROTECT);
            }
            MountOption::RW => {
                // Dokan defaults to read-write when write-protect flag is absent.
            }
            MountOption::FSName(_) | MountOption::Subtype(_) => {
                // Consumed by derive_volume_names; no direct Dokan flag.
            }
            opt @ (MountOption::AllowOther
            | MountOption::AllowRoot
            | MountOption::AutoUnmount
            | MountOption::DefaultPermissions
            | MountOption::Dev
            | MountOption::NoDev
            | MountOption::Suid
            | MountOption::NoSuid
            | MountOption::Exec
            | MountOption::NoExec
            | MountOption::Atime
            | MountOption::NoAtime
            | MountOption::DirSync
            | MountOption::Sync
            | MountOption::Async) => {
                debug_assert!(is_dokan_inexpressible_mount_option(opt));
                // Accepted for fuser-facing parity; no Dokan equivalent flag.
            }
            MountOption::CUSTOM(v) => {
                if v.eq_ignore_ascii_case("single_thread") {
                    out.single_thread = true;
                    continue;
                }
                if v.eq_ignore_ascii_case("debug") {
                    out.flags.insert(dokan::MountFlags::DEBUG);
                    continue;
                }
                // Accept unknown custom options for fuser-facing compatibility.
            }
        }
    }
    Ok(out)
}

pub(crate) fn is_dokan_inexpressible_mount_option(opt: &MountOption) -> bool {
    matches!(
        opt,
        MountOption::AllowOther
            | MountOption::AllowRoot
            | MountOption::AutoUnmount
            | MountOption::DefaultPermissions
            | MountOption::Dev
            | MountOption::NoDev
            | MountOption::Suid
            | MountOption::NoSuid
            | MountOption::Exec
            | MountOption::NoExec
            | MountOption::Atime
            | MountOption::NoAtime
            | MountOption::DirSync
            | MountOption::Sync
            | MountOption::Async
    )
}

#[cfg(test)]
pub(crate) fn parse_mount_options(options: &[&OsStr]) -> Vec<MountOption> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < options.len() {
        let s = options[i].to_string_lossy();
        if s == "-o" {
            if i + 1 < options.len() {
                let csv = options[i + 1].to_string_lossy();
                for part in csv.split(',') {
                    let p = part.trim();
                    if !p.is_empty() {
                        out.push(parse_single_mount_option(p));
                    }
                }
                i += 2;
                continue;
            }
            out.push(MountOption::CUSTOM("-o".to_string()));
            i += 1;
            continue;
        }
        if let Some(rest) = s.strip_prefix("-o").filter(|rest| !rest.is_empty()) {
            for part in rest.split(',') {
                let p = part.trim();
                if !p.is_empty() {
                    out.push(parse_single_mount_option(p));
                }
            }
            i += 1;
            continue;
        }
        out.push(parse_single_mount_option(&s));
        i += 1;
    }
    out
}
