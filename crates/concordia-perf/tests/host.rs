//! The host probe reads the GPU core count, memory and OS from the OS's tools (M4 phase 4.1a).

use concordia_perf::HostProbe;
use concordia_perf::machine::{SystemProbe, parse_gpu_cores, parse_memory_gb};

#[test]
fn gpu_cores_come_from_the_accelerator_entry() {
    let ioreg = r#"+-o AGXAcceleratorG15X  <class AGXAcceleratorG15X>
    {
      "model" = "Apple M3 Pro"
      "gpu-core-count" = 18
      "AGXParameterBufferMaxSize" = 1006632960
    }"#;
    assert_eq!(parse_gpu_cores(ioreg), Some(18));
    assert_eq!(parse_gpu_cores("no accelerator here"), None);
    assert_eq!(parse_gpu_cores(r#""gpu-core-count" = lots"#), None);
}

#[test]
fn memory_is_whole_gigabytes() {
    assert_eq!(parse_memory_gb("19327352832\n"), Some(18));
    assert_eq!(parse_memory_gb("17179869184"), Some(16));
    assert_eq!(parse_memory_gb("garbage"), None);
}

#[test]
fn this_host_is_described_from_its_caps_and_tools() {
    let Ok(caps) = concordia_rhi_metal::probe() else {
        assert!(!concordia_testkit::gpu_required(), "no Metal device");
        return;
    };
    let host = SystemProbe.host(&caps);
    assert_eq!(host.device_name, caps.device_name);
    assert_eq!(host.families, caps.feature_sets);
    assert_eq!(host.os, "macOS");
    assert!(
        host.os_version
            .split('.')
            .next()
            .unwrap()
            .parse::<u32>()
            .is_ok(),
        "{host:?}"
    );
    assert!(host.gpu_cores.is_some_and(|c| c > 0), "{host:?}");
    assert!(host.memory_gb.is_some_and(|m| m > 0), "{host:?}");
}
