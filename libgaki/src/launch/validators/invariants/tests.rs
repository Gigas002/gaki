use std::path::PathBuf;

use super::*;
use crate::infra::runners::CommandSpec;
use crate::launch::dll_provider_resolver::{DllProvider, DllResolution};

/// A context whose command carries `env`.
fn with_env(env: &[(&str, &str)]) -> PipelineContext {
    let mut ctx = PipelineContext::new(1);
    let mut spec = CommandSpec::default();
    for (key, value) in env {
        spec.env.insert(key.to_string(), value.to_string());
    }
    ctx.command_spec = Some(spec);
    ctx
}

/// The warning codes validation adds, in order.
fn codes(mut ctx: PipelineContext) -> Vec<String> {
    LaunchInvariantValidator.validate(&mut ctx);
    ctx.warnings.into_iter().map(|w| w.code).collect()
}

fn resolution(name: &str, path: &str) -> DllResolution {
    DllResolution {
        name: name.into(),
        chosen_provider: DllProvider::Runner,
        chosen_path: Some(PathBuf::from(path)),
        fallback_reason: None,
        candidates: Vec::new(),
    }
}

#[test]
fn a_clean_launch_has_no_warnings() {
    let mut ctx = with_env(&[("WINEDLLOVERRIDES", "d3d11=n,b")]);
    ctx.graphics_stack.effective_backend = "DXVK".into();
    ctx.graphics_stack.runtime_evidence.dxvk.evidence_found = true;
    assert!(codes(ctx).is_empty());
    assert!(codes(PipelineContext::new(1)).is_empty());
}

#[test]
fn gpu_forcing_without_a_chosen_gpu() {
    let ctx = with_env(&[("DRI_PRIME", "1"), ("__GLX_VENDOR_LIBRARY_NAME", "nvidia")]);
    assert_eq!(
        codes(ctx),
        ["INVARIANT_A_VIOLATION", "INVARIANT_A_VIOLATION"]
    );

    let mut ctx = with_env(&[("DRI_PRIME", "1")]);
    ctx.graphics_stack.effective_gpu = Some("AMD".into());
    assert!(codes(ctx).is_empty());
}

#[test]
fn dxvk_overrides_without_dxvk() {
    let ctx = with_env(&[
        ("WINEDLLOVERRIDES", "D3D11 =n;dxgi=b;d3d9=n,b"),
        ("WINEDLLPATH", "/runner/lib/dxvk"),
    ]);
    // Native d3d11 and d3d9 (case and spaces ignored); builtin dxgi is fine.
    assert_eq!(
        codes(ctx),
        [
            "INVARIANT_B_VIOLATION",
            "INVARIANT_B_VIOLATION",
            "INVARIANT_B_VIOLATION"
        ]
    );
}

#[test]
fn d3d12_overrides_without_a_provider() {
    let mut ctx = with_env(&[
        ("WINEDLLOVERRIDES", "d3d12=n"),
        ("WINEDLLPATH", "/x/vkd3d/lib"),
    ]);
    ctx.graphics_stack.effective_backend = "DXVK".into();
    ctx.graphics_stack.runtime_evidence.dxvk.evidence_found = true;
    ctx.graphics_stack.effective_d3d12_provider = "None".into();
    assert_eq!(
        codes(ctx),
        ["INVARIANT_C_VIOLATION", "INVARIANT_C_VIOLATION"]
    );
}

#[test]
fn d3d12_provider_must_match_its_path() {
    let mut ctx = PipelineContext::new(1);
    ctx.graphics_stack.effective_d3d12_provider = "vkd3d-proton".into();
    ctx.graphics_stack
        .runtime_evidence
        .vkd3d_proton
        .evidence_found = true;
    ctx.dll_resolutions = vec![resolution("d3d12", "/r/vkd3d/x64/d3d12.dll")];
    assert_eq!(codes(ctx), ["INVARIANT_C_MISMATCH"]);

    let mut ctx = PipelineContext::new(1);
    ctx.graphics_stack.effective_d3d12_provider = "vkd3d".into();
    ctx.dll_resolutions = vec![resolution("d3d12core", "/r/vkd3d-proton/d3d12core.dll")];
    assert_eq!(codes(ctx), ["INVARIANT_C_MISMATCH"]);

    let mut ctx = PipelineContext::new(1);
    ctx.graphics_stack.effective_d3d12_provider = "vkd3d-proton".into();
    ctx.graphics_stack
        .runtime_evidence
        .vkd3d_proton
        .evidence_found = true;
    ctx.dll_resolutions = vec![resolution("d3d12", "/r/vkd3d-proton/d3d12.dll")];
    assert!(codes(ctx).is_empty());
}

#[test]
fn requested_settings_must_survive() {
    let mut ctx = PipelineContext::new(1);
    ctx.graphics_stack.requested_backend = "DXVK".into();
    ctx.graphics_stack.effective_backend = "WineD3D (Baseline)".into();
    ctx.graphics_stack
        .fallback_reasons
        .insert("graphics_backend".into(), "no Vulkan".into());
    LaunchInvariantValidator.validate(&mut ctx);
    assert_eq!(ctx.warnings.len(), 1);
    assert_eq!(ctx.warnings[0].code, "STRICT_POLICY_VIOLATION");
    assert!(ctx.warnings[0].message.contains("no Vulkan"));

    let mut ctx = PipelineContext::new(1);
    ctx.graphics_stack.requested_d3d12_provider = "vkd3d-proton".into();
    ctx.graphics_stack.effective_d3d12_provider = "None".into();
    assert_eq!(codes(ctx), ["INVARIANT_D_D3D12_MISMATCH"]);

    let mut ctx = PipelineContext::new(1);
    ctx.graphics_stack.requested_gpu = Some("AMD Radeon".into());
    ctx.graphics_stack.effective_gpu = Some("Intel UHD".into());
    assert_eq!(codes(ctx), ["INVARIANT_D_GPU_MISMATCH"]);

    // Effective names are synthesized, so a partial match is enough.
    let mut ctx = PipelineContext::new(1);
    ctx.graphics_stack.requested_gpu = Some("NVIDIA RTX".into());
    ctx.graphics_stack.effective_gpu = Some("NVIDIA GeForce".into());
    assert!(codes(ctx).is_empty());
}

#[test]
fn missing_runtime_evidence() {
    let mut ctx = PipelineContext::new(1);
    ctx.graphics_stack.effective_backend = "DXVK".into();
    LaunchInvariantValidator.validate(&mut ctx);
    assert_eq!(ctx.warnings[0].code, "DIAGNOSTICS_MISSING_DXVK_EVIDENCE");
    assert!(ctx.warnings[0].message.contains("(Wine log missing)"));

    let mut ctx = PipelineContext::new(1);
    ctx.graphics_stack.requested_backend = "DXVK".into();
    ctx.graphics_stack.effective_backend = "DXVK".into();
    ctx.graphics_stack
        .runtime_evidence
        .scan_metadata
        .file_exists = true;
    LaunchInvariantValidator.validate(&mut ctx);
    assert_eq!(ctx.warnings[0].code, "STRICT_DXVK_EVIDENCE_MISSING");
    assert!(ctx.warnings[0].message.contains("(Wine log empty)"));

    let mut ctx = PipelineContext::new(1);
    ctx.graphics_stack.effective_d3d12_provider = "vkd3d-proton".into();
    assert_eq!(codes(ctx), ["DIAGNOSTICS_MISSING_VKD3D_PROTON_EVIDENCE"]);
}

#[test]
fn launch_outcomes() {
    let mut ctx = PipelineContext::new(1);
    ctx.verification.status = "failed_after_spawn".into();
    ctx.verification.exit_code = Some(1);
    assert_eq!(codes(ctx), ["LAUNCH_EARLY_EXIT"]);

    // DXVK with an early exit explains the missing evidence by the exit.
    let mut ctx = PipelineContext::new(1);
    ctx.graphics_stack.effective_backend = "DXVK".into();
    ctx.verification.status = "failed_after_spawn".into();
    LaunchInvariantValidator.validate(&mut ctx);
    assert!(ctx.warnings[0].message.contains("exited immediately"));
    assert_eq!(ctx.warnings[1].code, "LAUNCH_EARLY_EXIT");

    let mut ctx = PipelineContext::new(1);
    ctx.verification.status = "uncertain".into();
    assert_eq!(codes(ctx), ["LAUNCH_UNCERTAIN"]);
}

#[test]
fn wined3d_seen_when_dxvk_was_asked_for() {
    let mut ctx = PipelineContext::new(1);
    ctx.graphics_stack.requested_backend = "DXVK".into();
    ctx.graphics_stack.effective_backend = "DXVK".into();
    ctx.graphics_stack.runtime_evidence.dxvk.evidence_found = true;
    ctx.graphics_stack.runtime_evidence.wined3d.evidence_found = true;
    assert_eq!(codes(ctx), ["STRICT_DXVK_POLICY_VIOLATION"]);
}

#[test]
fn dll_load_failures_name_the_dll() {
    let mut ctx = PipelineContext::new(1);
    ctx.graphics_stack.requested_backend = "DXVK".into();
    ctx.graphics_stack.effective_backend = "DXVK".into();
    ctx.graphics_stack.runtime_evidence.dxvk.evidence_found = true;
    ctx.dll_resolutions = vec![resolution("d3d11", "/r/dxvk/d3d11.dll")];
    ctx.graphics_stack.graphics_stack_evidence = vec![
        "DLL Load Failure: L\"d3d11.dll\" not found".into(),
        "DLL Dependency Missing: Library vulkan-1.dll (needed)".into(),
        // Noise: never reported.
        "DLL Load Failure: L\"steam_api64.dll\"".into(),
        "DXVK: v2.3".into(),
    ];
    LaunchInvariantValidator.validate(&mut ctx);
    let found: Vec<_> = ctx.warnings.iter().map(|w| w.code.as_str()).collect();
    assert_eq!(
        found,
        ["STRICT_DXVK_LOAD_FAILURE", "STRICT_DXVK_LOAD_FAILURE"]
    );
    assert!(ctx.warnings[0].message.contains("Provider: Runner"));
    assert!(ctx.warnings[0].message.contains("/r/dxvk/d3d11.dll"));
    assert!(ctx.warnings[1].message.contains("DLL: vulkan-1.dll"));
    assert!(ctx.warnings[1].message.contains("Provider: Unknown"));

    // Without a strict request it's the general policy code.
    let mut ctx = PipelineContext::new(1);
    ctx.graphics_stack.requested_backend = "VKD3D".into();
    ctx.graphics_stack.graphics_stack_evidence = vec!["DLL Load Failure: something".into()];
    assert_eq!(codes(ctx), ["STRICT_POLICY_LOAD_FAILURE"]);
}
