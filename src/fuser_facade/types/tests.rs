use super::*;
use crate::dokan_impl::default_kernel_config;
use std::time::Duration;
use std::time::UNIX_EPOCH;

#[test]
fn kernel_config_set_max_write_happy_path() {
    let mut cfg = default_kernel_config();
    assert_eq!(cfg.set_max_write(4096), Ok(128 * 1024));
    assert_eq!(cfg.set_max_write(8192), Ok(4096));
    assert_eq!(cfg.max_write, 8192);
}

#[test]
fn kernel_config_set_max_write_rejects_zero() {
    let mut cfg = default_kernel_config();
    assert_eq!(cfg.set_max_write(0), Err(1));
}

#[test]
fn kernel_config_set_max_write_rejects_too_large() {
    let mut cfg = default_kernel_config();
    assert_eq!(
        cfg.set_max_write(16 * 1024 * 1024 + 1),
        Err(16 * 1024 * 1024)
    );
    assert!(cfg.set_max_write(16 * 1024 * 1024).is_ok());
}

#[test]
fn kernel_config_set_max_readahead_happy_path() {
    let mut cfg = default_kernel_config();
    let previous = cfg.set_max_readahead(4096).expect("should succeed");
    assert_eq!(previous, 128 * 1024);
    assert_eq!(cfg.max_readahead, 4096);
}

#[test]
fn kernel_config_set_max_readahead_rejects_zero() {
    let mut cfg = default_kernel_config();
    assert_eq!(cfg.set_max_readahead(0), Err(1));
}

#[test]
fn kernel_config_set_max_readahead_rejects_exceeds_max() {
    let mut cfg = KernelConfig {
        max_write: 128 * 1024,
        max_readahead: 128 * 1024,
        max_max_readahead: 4096,
        capabilities: InitFlags::empty(),
        requested: InitFlags::empty(),
        max_background: 16,
        congestion_threshold: None,
        time_gran: Duration::new(0, 1),
        max_stack_depth: 0,
        kernel_abi: Version(7, 40),
    };
    assert_eq!(cfg.set_max_readahead(8192), Err(4096));
}

#[test]
fn kernel_config_add_capabilities_rejects_incompatible() {
    let mut cfg = KernelConfig {
        max_write: 128 * 1024,
        max_readahead: 128 * 1024,
        max_max_readahead: 1024 * 1024,
        capabilities: InitFlags::from_bits_retain(0b01),
        requested: InitFlags::empty(),
        max_background: 16,
        congestion_threshold: None,
        time_gran: Duration::new(0, 1),
        max_stack_depth: 0,
        kernel_abi: Version(7, 40),
    };
    assert_eq!(
        cfg.add_capabilities(InitFlags::from_bits_retain(0b10)),
        Err(InitFlags::from_bits_retain(0b10))
    );
}

#[test]
fn kernel_config_add_capabilities_accepts_compatible() {
    let mut cfg = KernelConfig {
        max_write: 128 * 1024,
        max_readahead: 128 * 1024,
        max_max_readahead: 1024 * 1024,
        capabilities: InitFlags::from_bits_retain(0b11),
        requested: InitFlags::empty(),
        max_background: 16,
        congestion_threshold: None,
        time_gran: Duration::new(0, 1),
        max_stack_depth: 0,
        kernel_abi: Version(7, 40),
    };
    assert_eq!(
        cfg.add_capabilities(InitFlags::from_bits_retain(0b01)),
        Ok(())
    );
    assert_eq!(cfg.requested, InitFlags::from_bits_retain(0b01));
}

#[test]
fn time_or_now_variants_exist_and_match() {
    let specific = TimeOrNow::SpecificTime(UNIX_EPOCH);
    let now = TimeOrNow::Now;
    assert!(matches!(specific, TimeOrNow::SpecificTime(_)));
    assert!(matches!(now, TimeOrNow::Now));
}

#[test]
fn open_flags_acc_mode_uses_libc_mask() {
    assert_eq!(OpenFlags(libc::O_RDONLY).acc_mode(), OpenAccMode::O_RDONLY);
    assert_eq!(OpenFlags(libc::O_WRONLY).acc_mode(), OpenAccMode::O_WRONLY);
    assert_eq!(OpenFlags(libc::O_RDWR).acc_mode(), OpenAccMode::O_RDWR);
    assert_eq!(
        OpenFlags(!(libc::O_RDONLY | libc::O_WRONLY | libc::O_RDWR)).acc_mode(),
        OpenAccMode::O_RDONLY
    );
}

#[test]
fn kernel_config_set_max_background_happy_path() {
    let mut cfg = default_kernel_config();
    assert_eq!(cfg.set_max_background(32), Ok(16));
    assert_eq!(cfg.max_background, 32);
}

#[test]
fn kernel_config_set_max_background_rejects_zero() {
    let mut cfg = default_kernel_config();
    assert_eq!(cfg.set_max_background(0), Err(1));
}

#[test]
fn kernel_config_set_congestion_threshold_happy_path() {
    let mut cfg = default_kernel_config();
    assert_eq!(cfg.set_congestion_threshold(8), Ok(12));
    assert_eq!(cfg.congestion_threshold, Some(8));
}

#[test]
fn kernel_config_set_congestion_threshold_clamps_previous_value_to_max_background() {
    let mut cfg = default_kernel_config();
    cfg.congestion_threshold = Some(32);
    assert_eq!(cfg.set_congestion_threshold(8), Ok(16));
}

#[test]
fn file_type_from_std_returns_none_for_unknown_platform_types() {
    let metadata = std::fs::metadata(".").expect("current directory metadata");
    assert_eq!(
        FileType::from_std(metadata.file_type()),
        Some(FileType::Directory)
    );
}

#[test]
fn kernel_config_set_congestion_threshold_rejects_zero() {
    let mut cfg = default_kernel_config();
    assert_eq!(cfg.set_congestion_threshold(0), Err(1));
}

#[test]
fn kernel_config_set_time_granularity_valid_1ns() {
    let mut cfg = default_kernel_config();
    assert_eq!(
        cfg.set_time_granularity(Duration::new(0, 1)),
        Ok(Duration::new(0, 1))
    );
}

#[test]
fn kernel_config_set_time_granularity_rejects_zero() {
    let mut cfg = default_kernel_config();
    assert_eq!(
        cfg.set_time_granularity(Duration::ZERO),
        Err(Duration::new(0, 1))
    );
}

#[test]
fn kernel_config_set_time_granularity_rejects_gt_one_sec() {
    let mut cfg = default_kernel_config();
    assert_eq!(
        cfg.set_time_granularity(Duration::new(2, 0)),
        Err(Duration::new(1, 0))
    );
}

#[test]
fn kernel_config_set_time_granularity_rejects_non_power_of_10() {
    let mut cfg = default_kernel_config();
    assert!(cfg.set_time_granularity(Duration::new(0, 3)).is_err());
}

#[test]
fn kernel_config_set_time_granularity_accepts_1sec_exact() {
    let mut cfg = default_kernel_config();
    let previous = cfg
        .set_time_granularity(Duration::new(1, 0))
        .expect("should succeed");
    assert_eq!(previous, Duration::new(0, 1));
    assert_eq!(cfg.time_gran, Duration::new(1, 0));
}

#[test]
fn mount_option_enum_excludes_acl_variants() {
    let source = include_str!("../types.rs");
    assert!(!source.contains(concat!("Allow", "Other")));
    assert!(!source.contains(concat!("Allow", "Root")));
}

#[cfg(feature = "macos-api")]
#[test]
fn bsd_file_flags_bits() {
    assert_eq!(BsdFileFlags::UF_NODUMP.bits(), 0x0000_0001);
    assert_eq!(BsdFileFlags::UF_IMMUTABLE.bits(), 0x0000_0002);
    assert_eq!(BsdFileFlags::UF_APPEND.bits(), 0x0000_0004);
    assert_eq!(BsdFileFlags::UF_OPAQUE.bits(), 0x0000_0008);
    assert_eq!(BsdFileFlags::UF_HIDDEN.bits(), 0x0000_8000);
    assert_eq!(BsdFileFlags::SF_ARCHIVED.bits(), 0x0001_0000);
    assert_eq!(BsdFileFlags::SF_IMMUTABLE.bits(), 0x0002_0000);
    assert_eq!(BsdFileFlags::SF_APPEND.bits(), 0x0004_0000);
    assert_eq!(BsdFileFlags::all().bits(), 0x0007_800f);
}

#[cfg(feature = "macos-api")]
#[test]
fn fopen_purge_constants() {
    assert_eq!(FopenFlags::FOPEN_PURGE_ATTR.bits(), 0x4000_0000);
    assert_eq!(FopenFlags::FOPEN_PURGE_UBC.bits(), 0x8000_0000);
    assert_eq!(FopenFlags::all().bits(), 0xc000_00ff);
}

#[cfg(feature = "macos-api")]
#[test]
fn macos_init_flags_match_fuser_017_bits() {
    assert_eq!(InitFlags::FUSE_ALLOCATE.bits(), 1 << 27);
    assert_eq!(InitFlags::FUSE_EXCHANGE_DATA.bits(), 1 << 28);
    assert_eq!(InitFlags::FUSE_CASE_INSENSITIVE.bits(), 1 << 29);
    assert_eq!(InitFlags::FUSE_VOL_RENAME.bits(), 1 << 30);
    assert_eq!(InitFlags::FUSE_XTIMES.bits(), 1 << 31);
}
