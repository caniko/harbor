//! Source-bound whole-region spectral material associations and exact CAD triangles.
use crate::{
    Result,
    cad_source::CadSource,
    cad_triangles::TriangleAssessment,
    contracts::{ArtifactManifest, digest, invalid, token},
    materials::PhysicalInput,
    science::Quantity,
    storage::{Store, safe_path},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpaqueSpectralResponse {
    pub formulation: String,
    pub interpolation: String,
    pub reflectance: Vec<f64>,
    pub absorptivity: Vec<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CadSpectralMaterial {
    pub name: String,
    pub response: PhysicalInput<OpaqueSpectralResponse>,
    pub ageing_action: PhysicalInput<Vec<f64>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RegionMaterial {
    pub region_name: String,
    pub material_name: String,
    pub provenance: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CadSpectralSceneRequest {
    pub schema_version: u32,
    pub source_job: String,
    pub wavelengths: Vec<Quantity>,
    pub geometry_tolerance: Quantity,
    pub materials: Vec<CadSpectralMaterial>,
    pub assignments: Vec<RegionMaterial>,
}

impl CadSpectralSceneRequest {
    pub fn validate(&self) -> Result<Vec<String>> {
        let n = self.wavelengths.len();
        let tolerance = self.geometry_tolerance.si("length")?;
        if self.schema_version != 1
            || uuid::Uuid::parse_str(&self.source_job).is_err()
            || !(2..=16).contains(&n)
            || !(1..=16).contains(&self.materials.len())
            || !(1..=16).contains(&self.assignments.len())
            || !(1e-10..=1e-4).contains(&tolerance)
        {
            return Err(invalid(
                "versioned registered source, bounded complete explicit spectral knots/materials/region assignments required",
            ));
        }
        let mut previous = 0.;
        for q in &self.wavelengths {
            let w = q.si("length")?;
            if !(200e-9..=2500e-9).contains(&w) || w <= previous {
                return Err(invalid(
                    "strictly ordered 200-2500 nm original spectral knots required; no extrapolation",
                ));
            }
            previous = w;
        }
        let mut names = BTreeSet::new();
        let mut missing = Vec::new();
        for material in &self.materials {
            if !token(&material.name) || !names.insert(material.name.as_str()) {
                return Err(invalid("unambiguous named spectral materials required"));
            }
            material.response.validate()?;
            material.ageing_action.validate()?;
            match &material.response {
                PhysicalInput::Known { value, .. } => {
                    if value.formulation != "opaque_lambertian"
                        || value.interpolation != "piecewise_linear"
                        || value.reflectance.len() != n
                        || value.absorptivity.len() != n
                        || value
                            .reflectance
                            .iter()
                            .chain(&value.absorptivity)
                            .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
                        || value
                            .reflectance
                            .iter()
                            .zip(&value.absorptivity)
                            .any(|(r, a)| (r + a - 1.).abs() > 1e-12)
                    {
                        return Err(invalid(
                            "complete explicit opaque Lambertian reflectance/absorptivity at every native knot with unchanged 1e-12 energy closure required",
                        ));
                    }
                }
                PhysicalInput::Missing { .. } => {
                    missing.push(format!("{}.response", material.name))
                }
            }
            match &material.ageing_action {
                PhysicalInput::Known { value, .. }
                    if value.len() != n
                        || value
                            .iter()
                            .any(|v| !v.is_finite() || !(0. ..=1.).contains(v)) =>
                {
                    return Err(invalid(
                        "complete dimensionless bounded ageing action at the original spectral knots required",
                    ));
                }
                PhysicalInput::Missing { .. } => {
                    missing.push(format!("{}.ageing_action", material.name))
                }
                _ => (),
            }
        }
        let mut regions = BTreeSet::new();
        let mut used = BTreeSet::new();
        for assignment in &self.assignments {
            if !token(&assignment.region_name)
                || !regions.insert(assignment.region_name.as_str())
                || !names.contains(assignment.material_name.as_str())
                || assignment.provenance.trim().is_empty()
                || assignment.provenance.len() > 4096
            {
                return Err(invalid(
                    "one explicit known material assignment per complete named region; no face numbers or foreign tags",
                ));
            }
            used.insert(assignment.material_name.as_str());
        }
        if used != names {
            return Err(invalid(
                "unused or ambiguous material definitions are forbidden",
            ));
        }
        Ok(missing)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaterialTriangleRegion {
    pub assignment: RegionMaterial,
    pub source: CadSource,
    pub original_triangles: ArtifactManifest,
    pub geometry: TriangleAssessment,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedCadSpectralScene {
    pub schema_version: u32,
    pub scene_id: String,
    pub request: CadSpectralSceneRequest,
    pub regions: Vec<MaterialTriangleRegion>,
    pub missing_inputs: Vec<String>,
    pub transport_readiness: String,
    pub ageing_readiness: String,
    pub executed: bool,
    pub physical_validation: String,
    pub limitations: Vec<String>,
}

pub fn prepare(
    store: &Store,
    request: CadSpectralSceneRequest,
) -> Result<PreparedCadSpectralScene> {
    let missing_inputs = request.validate()?;
    let registered = crate::cad::regions(store, &request.source_job)?;
    let tolerance = request.geometry_tolerance.si("length")?;
    if tolerance > registered.snapshot.geometry_tolerance.si("length")? {
        return Err(invalid(
            "spectral geometry gate cannot weaken the original importer tolerance",
        ));
    }
    let expected: BTreeSet<_> = registered
        .snapshot
        .regions
        .iter()
        .map(|r| r.name.as_str())
        .collect();
    let supplied: BTreeSet<_> = request
        .assignments
        .iter()
        .map(|r| r.region_name.as_str())
        .collect();
    if expected != supplied {
        return Err(invalid(
            "spectral material tags must cover every original CAD region exactly once",
        ));
    }
    let mut regions = Vec::new();
    for assignment in &request.assignments {
        let (_, source) = crate::cad_source::source(
            store,
            &request.source_job,
            &assignment.region_name,
            2,
            tolerance,
        )?;
        let path = format!("{}.stl", assignment.region_name);
        let original = store
            .artifact_record(&request.source_job, &path)?
            .ok_or_else(|| invalid("original registered CAD triangle export required"))?;
        if original.schema_version != 1
            || original.path != path
            || original.format != "stl"
            || original.units.is_some()
            || original.association.is_some()
            || original.time_s.is_some()
            || !(84..=(84 + 50 * crate::cad_triangles::MAX_TRIANGLES) as u64)
                .contains(&original.bytes)
        {
            return Err(invalid(
                "unchanged bounded original STL units/association descriptor required",
            ));
        }
        let data = crate::worker::read_bounded(
            &safe_path(&store.job_dir(&request.source_job)?, &path)?,
            (84 + 50 * crate::cad_triangles::MAX_TRIANGLES) as u64,
        )?;
        use sha2::{Digest, Sha256};
        if data.len() as u64 != original.bytes
            || format!("{:x}", Sha256::digest(&data)) != original.sha256
        {
            return Err(invalid(
                "CAD triangle original differs from registered immutable bytes",
            ));
        }
        let triangles = crate::cad_triangles::verify_binary_box(&data, &source.geometry)?;
        let count = registered
            .snapshot
            .regions
            .iter()
            .find(|r| r.name == assignment.region_name)
            .ok_or_else(|| invalid("named region required"))?
            .triangles;
        if triangles.assessment.triangles as u64 != count {
            return Err(invalid(
                "original triangle count differs from importer evidence",
            ));
        }
        regions.push(MaterialTriangleRegion {
            assignment: assignment.clone(),
            source,
            original_triangles: original,
            geometry: triangles.assessment,
        });
    }
    let transport_missing = missing_inputs.iter().any(|m| m.ends_with(".response"));
    let ageing_missing = missing_inputs.iter().any(|m| m.ends_with(".ageing_action"));
    let mut report=PreparedCadSpectralScene{schema_version:1,scene_id:String::new(),request,regions,missing_inputs,
        transport_readiness:if transport_missing{"missing_optical_inputs"}else{"prepared_not_executed"}.into(),
        ageing_readiness:if transport_missing || ageing_missing{"missing_inputs"}else{"prepared_not_executed"}.into(),executed:false,physical_validation:"unqualified".into(),limitations:vec![
            "whole-region opaque Lambertian materials on registered top-level planar boxes; no ordinal face tags, assembly traversal, textured or transmitting surfaces".into(),
            "original binary STL Float32 positions/normals and facet order are preserved; SI coordinates are Float64 conversions, not recovered native BREP precision".into(),
            "six boundary planes, oriented closed manifold, surface areas and volume checked without welding, healing, implicit placement or property repair".into(),
            "no source irradiance, angular reduction, occlusion/reflection solve, temperature, dose or material lifetime is inferred by scene preparation".into()]};
    report.scene_id = digest(&report)?;
    if serde_json::to_vec(&report)?.len() > 256 * 1024 {
        return Err(crate::Error::Resource(
            "bounded spectral-scene response allowance exhausted".into(),
        ));
    }
    Ok(report)
}
