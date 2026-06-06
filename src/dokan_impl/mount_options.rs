#[cfg(test)]
use std::ffi::OsStr;
use std::io;

#[cfg(test)]
use crate::fuser_facade::types::Config;
use crate::fuser_facade::types::MountOption;
#[cfg(test)]
use crate::fuser_facade::types::SessionACL;

#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ParsedMountOption {
    Mount(MountOption),
    Acl(SessionACL),
}

#[cfg(test)]
pub(crate) fn parse_single_mount_option(s: &str) -> ParsedMountOption {
    match s {
        "ro" => ParsedMountOption::Mount(MountOption::RO),
        "rw" => ParsedMountOption::Mount(MountOption::RW),
        "allow_other" => ParsedMountOption::Acl(SessionACL::All),
        "allow_root" => ParsedMountOption::Acl(SessionACL::RootAndOwner),
        "auto_unmount" => ParsedMountOption::Mount(MountOption::AutoUnmount),
        "default_permissions" => ParsedMountOption::Mount(MountOption::DefaultPermissions),
        "dev" => ParsedMountOption::Mount(MountOption::Dev),
        "nodev" => ParsedMountOption::Mount(MountOption::NoDev),
        "suid" => ParsedMountOption::Mount(MountOption::Suid),
        "nosuid" => ParsedMountOption::Mount(MountOption::NoSuid),
        "exec" => ParsedMountOption::Mount(MountOption::Exec),
        "noexec" => ParsedMountOption::Mount(MountOption::NoExec),
        "atime" => ParsedMountOption::Mount(MountOption::Atime),
        "noatime" => ParsedMountOption::Mount(MountOption::NoAtime),
        "dirsync" => ParsedMountOption::Mount(MountOption::DirSync),
        "sync" => ParsedMountOption::Mount(MountOption::Sync),
        "async" => ParsedMountOption::Mount(MountOption::Async),
        _ => {
            if let Some(rest) = s.strip_prefix("fsname=") {
                return ParsedMountOption::Mount(MountOption::FSName(rest.to_string()));
            }
            if let Some(rest) = s.strip_prefix("subtype=") {
                return ParsedMountOption::Mount(MountOption::Subtype(rest.to_string()));
            }
            ParsedMountOption::Mount(MountOption::CUSTOM(s.to_string()))
        }
    }
}

#[cfg(test)]
pub(crate) fn parse_mount_options_from_args(options: &[&OsStr]) -> io::Result<Config> {
    let err = |x: String| io::Error::new(io::ErrorKind::InvalidInput, x);
    let args: Option<Vec<_>> = options.iter().map(|x| x.to_str()).collect();
    let args = args.ok_or_else(|| err("Error parsing args: Invalid UTF-8".to_owned()))?;
    let mut it = args.iter();
    let mut out = Config::default();
    loop {
        let opt = match it.next() {
            None => break,
            Some(&"-o") => *it.next().ok_or_else(|| {
                err("Error parsing args: Expected option, reached end of args".to_owned())
            })?,
            Some(x) if x.starts_with("-o") => &x[2..],
            Some(x) => x,
        };
        apply_csv_mount_options(&mut out, opt)?;
    }
    validate_mount_options(&out.mount_options)?;
    Ok(out)
}

#[cfg(test)]
fn apply_csv_mount_options(config: &mut Config, csv: &str) -> io::Result<()> {
    for part in csv.split(',') {
        let option = part.trim();
        if option.is_empty() {
            continue;
        }
        apply_parsed_mount_option(config, parse_single_mount_option(option))?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn apply_parsed_mount_option(
    config: &mut Config, option: ParsedMountOption,
) -> io::Result<()> {
    match option {
        ParsedMountOption::Mount(option) => config.mount_options.push(option),
        ParsedMountOption::Acl(acl) => {
            if config.acl != SessionACL::Owner && config.acl != acl {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "allow_other and allow_root are mutually exclusive",
                ));
            }
            config.acl = acl;
        }
    }
    Ok(())
}

pub(crate) fn validate_mount_options(options: &[MountOption]) -> io::Result<()> {
    reject_conflicting_mount_options(options, MountOption::RO, MountOption::RW, "ro", "rw")?;
    reject_conflicting_mount_options(
        options,
        MountOption::Dev,
        MountOption::NoDev,
        "dev",
        "nodev",
    )?;
    reject_conflicting_mount_options(
        options,
        MountOption::Suid,
        MountOption::NoSuid,
        "suid",
        "nosuid",
    )?;
    reject_conflicting_mount_options(
        options,
        MountOption::Exec,
        MountOption::NoExec,
        "exec",
        "noexec",
    )?;
    reject_conflicting_mount_options(
        options,
        MountOption::Atime,
        MountOption::NoAtime,
        "atime",
        "noatime",
    )?;
    reject_conflicting_mount_options(
        options,
        MountOption::Sync,
        MountOption::Async,
        "sync",
        "async",
    )?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn is_auto_probe_enabled(options: &[MountOption]) -> bool {
    options
        .iter()
        .any(|option| matches!(option, MountOption::CUSTOM(value) if value == "auto_probe"))
}

fn reject_conflicting_mount_options(
    options: &[MountOption], left: MountOption, right: MountOption, left_name: &str,
    right_name: &str,
) -> io::Result<()> {
    if options.contains(&left) && options.contains(&right) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("mount options {left_name} and {right_name} are mutually exclusive"),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Dokan mount-option mapping
// ---------------------------------------------------------------------------

pub(crate) fn to_dokan_mount_options(
    options: &[MountOption],
) -> io::Result<dokan::MountOptions<'static>> {
    validate_mount_options(options)?;
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
            opt @ (MountOption::AutoUnmount
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
                apply_custom_dokan_mount_option(&mut out, v);
            }
        }
    }
    Ok(out)
}

fn apply_custom_dokan_mount_option(out: &mut dokan::MountOptions<'static>, value: &str) {
    if value.eq_ignore_ascii_case("single_thread") {
        out.single_thread = true;
    }
    if value.eq_ignore_ascii_case("debug") {
        out.flags.insert(dokan::MountFlags::DEBUG);
    }
    // auto_probe and unknown custom options are accepted for fuser-facing compatibility.
}

pub(crate) fn is_dokan_inexpressible_mount_option(opt: &MountOption) -> bool {
    matches!(
        opt,
        MountOption::AutoUnmount
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
pub(crate) fn parse_mount_options(options: &[&OsStr]) -> io::Result<Config> {
    let mut out = Config::default();
    let mut i = 0usize;
    while i < options.len() {
        let s = options[i].to_string_lossy();
        if s == "-o" {
            if i + 1 < options.len() {
                let csv = options[i + 1].to_string_lossy();
                apply_csv_mount_options(&mut out, &csv)?;
                i += 2;
                continue;
            }
            out.mount_options
                .push(MountOption::CUSTOM("-o".to_string()));
            i += 1;
            continue;
        }
        if let Some(rest) = s.strip_prefix("-o").filter(|rest| !rest.is_empty()) {
            apply_csv_mount_options(&mut out, rest)?;
            i += 1;
            continue;
        }
        apply_parsed_mount_option(&mut out, parse_single_mount_option(&s))?;
        i += 1;
    }
    validate_mount_options(&out.mount_options)?;
    Ok(out)
}
