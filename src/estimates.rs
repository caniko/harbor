//! Conservative minima for the currently supported fixed adapters, not measured peaks.
use crate::{Result, contracts::*};

const MIB: u64 = 1024 * 1024;

pub struct StageMinimum {
    pub ram_bytes: u64,
    pub vram_bytes: u64,
}
pub struct MinimumResources {
    pub stages: Vec<StageMinimum>,
    pub artifact_bytes: u64,
}

fn add(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b)
        .ok_or_else(|| invalid("resource estimate overflow"))
}
fn multiply(a: u64, b: u64) -> Result<u64> {
    a.checked_mul(b)
        .ok_or_else(|| invalid("resource estimate overflow"))
}
fn lattice_cells(case: &CaseSpec) -> Result<u64> {
    let n = u64::from(case.resolution);
    let nx = (case.length.si("length")? / case.channel_height.si("length")?
        * f64::from(case.resolution))
    .ceil();
    if !nx.is_finite() || nx < 1. || nx >= u64::MAX as f64 {
        return Err(invalid("allocated periodic lattice extent overflow"));
    }
    // The driver allocates a padded cuboid, including non-fluid and halo cells.
    // These extents dominate its nx+2, ny+4, nz+2 VTI fixture.
    multiply(multiply(add(nx as u64, 4)?, add(n, 6)?)?, add(n, 4)?)
}

pub fn minimum(plan: &ExecutionPlan) -> Result<MinimumResources> {
    let pixels = multiply(
        u64::from(plan.case.presentation.width),
        u64::from(plan.case.presentation.height),
    )?;
    let snapshots = plan.observation.retained_times_s.len() as u64;
    let native = plan.stages.iter().any(|s| {
        !matches!(
            s.operation,
            StageOperation::ChannelReference | StageOperation::Bundle
        )
    });
    let mut output = if native { 16 * MIB } else { MIB };
    let mut stages = Vec::new();
    for stage in &plan.stages {
        let mut vram = 0;
        let ram = match stage.operation {
            StageOperation::ChannelReference => {
                output = output.max(multiply(u64::from(plan.case.resolution), 128)?);
                add(16 * MIB, multiply(u64::from(plan.case.resolution), 160)?)?
            }
            StageOperation::CadFixture | StageOperation::CadInspect => 1024 * MIB,
            StageOperation::Openlb => {
                let cells = lattice_cells(&plan.case)?;
                // Two D3Q19 Float64 distribution sets, fields, geometry, halo
                // storage and staging; use the established 2048-byte allowance.
                let ram = add(64 * MIB, multiply(cells, 2048)?)?;
                output = add(32 * MIB, multiply(multiply(cells, 96)?, snapshots)?)?;
                if stage.gpu != GpuRequirement::CpuOnly {
                    vram = ram;
                }
                ram
            }
            StageOperation::Render => {
                let cells = lattice_cells(&plan.case)?;
                vram = add(add(128 * MIB, multiply(pixels, 32)?)?, multiply(cells, 64)?)?;
                // Bound PNG worst-case staging and reader/display copies.
                output = add(
                    output,
                    multiply(add(multiply(pixels, 8)?, MIB)?, snapshots)?,
                )?;
                (1024 * MIB).max(add(multiply(cells, 256)?, multiply(pixels, 32)?)?)
            }
            StageOperation::Video => {
                vram = add(128 * MIB, multiply(pixels, 16)?)?;
                output = add(
                    output,
                    add(128 * MIB, multiply(multiply(pixels, 4)?, snapshots)?)?,
                )?;
                (512 * MIB).max(multiply(pixels, 16)?)
            }
            StageOperation::Bundle => 16 * MIB,
        };
        stages.push(StageMinimum {
            ram_bytes: ram,
            vram_bytes: vram,
        });
    }
    if output > 1_000_000_000_000
        || stages
            .iter()
            .any(|s| s.ram_bytes > i64::MAX as u64 || s.vram_bytes > i64::MAX as u64)
    {
        return Err(invalid(
            "resource estimate outside bounded adapter capacity",
        ));
    }
    Ok(MinimumResources {
        stages,
        artifact_bytes: output,
    })
}

impl MinimumResources {
    pub fn validate(&self, plan: &ExecutionPlan) -> Result<()> {
        for (stage, required) in plan.stages.iter().zip(&self.stages) {
            if stage.ram_bytes < required.ram_bytes || stage.vram_bytes < required.vram_bytes {
                return Err(invalid(format!(
                    "stage {} underestimates operation RAM/VRAM: at least {}/{} bytes required; parameters preserved",
                    stage.id, required.ram_bytes, required.vram_bytes
                )));
            }
        }
        if plan.observation.max_artifact_bytes < self.artifact_bytes {
            return Err(invalid(format!(
                "scientific/frame output estimate requires at least {} bytes for every approved retained time; parameters preserved",
                self.artifact_bytes
            )));
        }
        Ok(())
    }
    pub fn apply(&self, plan: &mut ExecutionPlan) {
        for (stage, required) in plan.stages.iter_mut().zip(&self.stages) {
            stage.ram_bytes = stage.ram_bytes.max(required.ram_bytes);
            stage.vram_bytes = stage.vram_bytes.max(required.vram_bytes);
        }
        plan.observation.max_artifact_bytes =
            plan.observation.max_artifact_bytes.max(self.artifact_bytes);
    }
}
