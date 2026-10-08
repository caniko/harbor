//! Static synthetic FEM references bound to exact authorized world-space CAD.
use crate::{
    Result,
    cad_source::{self, CadMeshDescriptor, CadSource},
    contracts::*,
    fem::FemReferenceSpec,
    storage::Store,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportedFemSpec {
    pub schema_version: u32,
    pub reference: FemReferenceSpec,
    pub material_provenance: String,
    pub boundary_provenance: String,
}

impl ImportedFemSpec {
    pub fn validate(&self, source: &CadSource) -> Result<()> {
        source.validate()?;
        self.reference.validate()?;
        let geometry = &source.geometry;
        if self.schema_version != 1
            || !geometry.synthetic
            || geometry.resolution != self.reference.resolution
            || geometry.geometry_tolerance_m != self.reference.geometry_tolerance_m
            || [&self.material_provenance, &self.boundary_provenance]
                .into_iter()
                .any(|v| v.trim().is_empty() || v.len() > 4096)
            || geometry
                .lengths_m()
                .iter()
                .zip(self.reference.size_m)
                .any(|(a, b)| {
                    (a - b).abs() > geometry.geometry_tolerance_m
                        || (a - b).abs() > geometry.volume_relative_tolerance * b
                })
        {
            return Err(invalid(
                "controlled synthetic imported FEM must retain exact source dimensions/refinement/tolerance and explicit material/boundary provenance",
            ));
        }
        Ok(())
    }
    pub fn formulation(&self) -> String {
        format!("imported_{}", self.reference.formulation())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportedFemRequest {
    pub source_job: String,
    pub region_name: String,
    pub spec: ImportedFemSpec,
}

#[derive(Serialize)]
pub struct NativeDescriptor<'a> {
    schema_version: u32,
    geometry: &'a CadMeshDescriptor,
    reference: &'a FemReferenceSpec,
    material_provenance: &'a str,
    boundary_provenance: &'a str,
}

pub fn descriptor(plan: &ExecutionPlan) -> Result<NativeDescriptor<'_>> {
    let source = plan
        .cad_source
        .as_ref()
        .ok_or_else(|| invalid("approved imported CAD source required"))?;
    let spec = plan
        .imported_fem
        .as_ref()
        .ok_or_else(|| invalid("approved imported FEM recipe required"))?;
    spec.validate(source)?;
    Ok(NativeDescriptor {
        schema_version: 1,
        geometry: &source.geometry,
        reference: &spec.reference,
        material_provenance: &spec.material_provenance,
        boundary_provenance: &spec.boundary_provenance,
    })
}

impl ExecutionPlan {
    pub fn fem_imported(source: CadSource, spec: ImportedFemSpec, policy: String) -> Result<Self> {
        spec.validate(&source)?;
        let mut plan = Self::cad_mesh(source, policy)?;
        plan.schema_version = 8;
        plan.imported_fem = Some(spec);
        plan.stages[0].id = "fem-imported".into();
        plan.stages[0].operation = StageOperation::FemImported;
        plan.stages[1].dependencies = vec!["fem-imported".into()];
        plan.observation.metrics.clear();
        crate::estimates::minimum(&plan)?.apply(&mut plan);
        plan.validate()?;
        Ok(plan)
    }
}

pub fn plan(store: &Store, request: ImportedFemRequest, policy: String) -> Result<ExecutionPlan> {
    let (_, source) = cad_source::source(
        store,
        &request.source_job,
        &request.region_name,
        request.spec.reference.resolution,
        request.spec.reference.geometry_tolerance_m,
    )?;
    ExecutionPlan::fem_imported(source, request.spec, policy)
}

pub fn verify_receipt(
    plan: &ExecutionPlan,
    root: &std::path::Path,
    value: &serde_json::Value,
) -> Result<crate::qualification::NumericalEvidence> {
    let descriptor = descriptor(plan)?;
    let geometry = descriptor.geometry;
    let spec = plan
        .imported_fem
        .as_ref()
        .ok_or_else(|| invalid("imported FEM recipe required"))?;
    if value["brep_sha256"] != geometry.brep_sha256
        || value["region_name"] != geometry.region_name
        || value["geometry_provenance"] != geometry.geometry_provenance
        || value["source_unit"] != "mm"
        || value["coordinate_unit"] != "m"
        || value["scale_to_m"] != 0.001
        || value["placement_translation_unit"] != "mm"
        || value["source_transform"] != serde_json::to_value(geometry.source_transform)?
        || value["world_bounds_m"] != serde_json::to_value(geometry.bounds_m)?
        || value["world_origin_m"]
            != serde_json::json!([
                geometry.bounds_m[0],
                geometry.bounds_m[2],
                geometry.bounds_m[4]
            ])
        || value["material_provenance"] != spec.material_provenance
        || value["boundary_provenance"] != spec.boundary_provenance
        || value["gap_healing"] != false
    {
        return Err(invalid(
            "imported FEM source/provenance/world placement changed",
        ));
    }
    for (path, key, max) in [
        ("mesh.json", "mesh_sha256", 32 * 1024 * 1024),
        ("reference.dat", "native_field_sha256", 32 * 1024 * 1024),
    ] {
        let record = crate::storage::native_manifest(
            root,
            path,
            max,
            "check bound imported FEM native output",
        )?;
        if value[key] != record.sha256 {
            return Err(invalid(
                "imported FEM native output bytes differ from receipt",
            ));
        }
    }
    let mesh = crate::worker::read_bounded(
        &crate::storage::safe_path(root, "mesh.json")?,
        32 * 1024 * 1024,
    )?;
    crate::cad_mesh::verify_mesh(geometry, serde_json::from_slice(&mesh)?)?;
    let mut evidence = crate::fem::verify_recipe_receipt(
        &spec.reference,
        value,
        &digest(&descriptor)?,
        spec.reference.formulation(),
        crate::execution::FEM_IMPORTED_SANDBOX_POLICY,
        &["source_brep_readonly", "named_source_only"],
    )?;
    evidence.scope="synthetic imported static FEM with approved CAD world origin; no contact or physical validation".into();
    Ok(evidence)
}
