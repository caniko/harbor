use crate::{Error, Result, science::Quantity};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const PROTOCOL_VERSION: u32 = 1;
pub const FLEETIX_REV: &str = "2230d9ee804a66d94424a91919182e4fcca13ab2";
pub const MAX_MESSAGE: u64 = 65536;

pub fn digest<T: Serialize>(value: &T) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
pub fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}
pub fn token(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub source: String,
    pub sha256: Option<String>,
    pub synthetic: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Material {
    pub name: String,
    pub provenance: Provenance,
    pub density: Quantity,
    pub humidity_fraction: Option<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PhysicsApplicability {
    pub formulation: String,
    pub dimensionality: u8,
    pub precision: String,
    pub validated_reynolds_max: Option<f64>,
    pub exclusions: Vec<String>,
    pub numerical_tolerance: f64,
    pub physical_validation: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Claim {
    Velocity,
    Cooling,
    ResolvedIngress,
    SnowDeposition,
    IceDamage,
    MaterialLifetime,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Presentation {
    pub camera: [f64; 3],
    pub width: u32,
    pub height: u32,
    pub field: String,
    pub range: [f64; 2],
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CaseSpec {
    pub schema_version: u32,
    pub name: String,
    pub geometry: Provenance,
    pub regions: Vec<String>,
    pub material: Material,
    pub applicability: PhysicsApplicability,
    pub length: Quantity,
    pub channel_height: Quantity,
    pub kinematic_viscosity: Quantity,
    pub acceleration: Quantity,
    pub geometry_tolerance: Quantity,
    pub resolution: u32,
    pub max_time_s: f64,
    pub claims: Vec<Claim>,
    pub presentation: Presentation,
}
impl CaseSpec {
    pub fn reference() -> Self {
        Self {
            schema_version: 1,
            name: "synthetic-channel".into(),
            geometry: Provenance {
                source: "synthetic parallel plates".into(),
                sha256: None,
                synthetic: true,
            },
            regions: vec!["fluid".into(), "wall".into()],
            material: Material {
                name: "synthetic Newtonian fluid".into(),
                provenance: Provenance {
                    source: "analytical fixture".into(),
                    sha256: None,
                    synthetic: true,
                },
                density: Quantity {
                    value: 1.,
                    unit: "kg/m3".into(),
                },
                humidity_fraction: None,
            },
            applicability: PhysicsApplicability {
                formulation: "steady_incompressible_channel".into(),
                dimensionality: 3,
                precision: "float64".into(),
                validated_reynolds_max: None,
                exclusions: vec!["thermal coupling".into(), "wetting and ingress".into()],
                numerical_tolerance: 1e-12,
                physical_validation: "unqualified".into(),
            },
            length: Quantity {
                value: 1.,
                unit: "m".into(),
            },
            channel_height: Quantity {
                value: 0.01,
                unit: "m".into(),
            },
            kinematic_viscosity: Quantity {
                value: 1e-5,
                unit: "m2/s".into(),
            },
            acceleration: Quantity {
                value: 0.1,
                unit: "m/s2".into(),
            },
            geometry_tolerance: Quantity {
                value: 1e-5,
                unit: "m".into(),
            },
            resolution: 33,
            max_time_s: 1.,
            claims: vec![Claim::Velocity],
            presentation: Presentation {
                camera: [1., 1., 1.],
                width: 640,
                height: 480,
                field: "velocity".into(),
                range: [0., 0.125],
            },
        }
    }
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1 || !token(&self.name) {
            return Err(invalid("case version/name"));
        }
        for (q, dimension) in [
            (&self.length, "length"),
            (&self.channel_height, "length"),
            (&self.kinematic_viscosity, "kinematic_viscosity"),
            (&self.material.density, "density"),
        ] {
            if q.si(dimension)? <= 0. {
                return Err(invalid(format!("positive {dimension} required")));
            }
        }
        self.acceleration.si("acceleration")?;
        if self.geometry_tolerance.si("length")? <= 0. {
            return Err(invalid("positive explicit CAD chord tolerance required"));
        }
        if !(3..=1_000_000).contains(&self.resolution)
            || !self.max_time_s.is_finite()
            || self.max_time_s <= 0.
        {
            return Err(invalid("resolution/time outside explicit limits"));
        }
        if self.regions.is_empty()
            || self.regions.iter().any(|r| !token(r))
            || self.regions.iter().collect::<BTreeSet<_>>().len() != self.regions.len()
        {
            return Err(invalid("regions must be unique, named and unambiguous"));
        }
        if let Some(h) = self.material.humidity_fraction
            && (!h.is_finite() || !(0.0..=1.).contains(&h))
        {
            return Err(invalid("humidity fraction"));
        }
        if self.applicability.precision != "float64"
            || self.applicability.dimensionality != 3
            || !self.applicability.numerical_tolerance.is_finite()
            || self.applicability.numerical_tolerance <= 0.
        {
            return Err(invalid(
                "unsupported dimensionality, precision or tolerance",
            ));
        }
        if self.applicability.physical_validation != "unqualified" {
            return Err(invalid(
                "physical validation must come from evidence, not a case assertion",
            ));
        }
        for claim in &self.claims {
            match claim {
                Claim::Velocity => {}
                Claim::Cooling => {
                    return Err(invalid(
                        "velocity alone cannot define cooling; heat-flux or correlation inputs required",
                    ));
                }
                Claim::ResolvedIngress => {
                    return Err(invalid("no validated resolved gap/wetting model"));
                }
                Claim::SnowDeposition => return Err(invalid("prescribed snow is not deposition")),
                Claim::IceDamage => return Err(invalid("latent heat is not ice damage")),
                Claim::MaterialLifetime => {
                    return Err(invalid("dose is not calibrated material lifetime"));
                }
            }
        }
        if self.presentation.width == 0
            || self.presentation.height == 0
            || self.presentation.width > 4096
            || self.presentation.height > 4096
            || self
                .presentation
                .camera
                .iter()
                .chain(self.presentation.range.iter())
                .any(|v| !v.is_finite())
            || self.presentation.range[0] >= self.presentation.range[1]
        {
            return Err(invalid("presentation budget or fixed comparison range"));
        }
        Ok(())
    }
    pub fn science_id(&self) -> Result<String> {
        let mut value = serde_json::to_value(self)?;
        value
            .as_object_mut()
            .ok_or_else(|| invalid("case serialization"))?
            .remove("presentation");
        digest(&value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Compute,
    Render,
    Media,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GpuSelection {
    pub role: Role,
    pub backend: String,
    pub pci: String,
    pub backend_uuid: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct B1Selections {
    pub compute: GpuSelection,
    pub render: GpuSelection,
    pub media: GpuSelection,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GpuRequirement {
    Required,
    Preferred,
    CpuOnly,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StageOperation {
    ChannelReference,
    CadFixture,
    CadInspect,
    Openlb,
    NumericalFilter,
    Render,
    Video,
    Bundle,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Stage {
    pub id: String,
    pub dependencies: Vec<String>,
    pub operation: StageOperation,
    pub gpu: GpuRequirement,
    pub selection: Option<GpuSelection>,
    pub ram_bytes: u64,
    pub vram_bytes: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObservationPlan {
    pub metrics: Vec<String>,
    pub probes: Vec<[f64; 3]>,
    pub retained_times_s: Vec<f64>,
    pub checkpoint_times_s: Vec<f64>,
    pub preview_times_s: Vec<f64>,
    pub max_artifact_bytes: u64,
    pub scientific_congestion: String,
    pub preview_may_drop: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransferSpec {
    pub source_region: String,
    pub destination_region: String,
    pub source_quantity: String,
    pub destination_quantity: String,
    pub unit: String,
    pub orientation: [f64; 3],
    pub interpolation: String,
    pub maximum_relative_conservation_error: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RetainedSource {
    pub job_id: String,
    pub plan_digest: String,
    pub execution_binding_digest: String,
    pub authorization_digest: String,
    pub snapshot_sha256: String,
    pub artifact_id: String,
    pub science_id: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresentationRequest {
    pub source_job: String,
    pub times_s: Vec<f64>,
    pub presentation: Presentation,
    pub render: GpuSelection,
    pub media: Option<GpuSelection>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FrameSource {
    pub job_id: String,
    pub plan_digest: String,
    pub execution_binding_digest: String,
    pub authorization_digest: String,
    pub sequence_sha256: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VideoRequest {
    pub source_job: String,
    pub media: GpuSelection,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GradientField {
    Velocity,
    Pressure,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FilterSpec {
    pub time_s: f64,
    pub field: GradientField,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FilterRequest {
    pub source_job: String,
    pub filter: FilterSpec,
    pub compute: GpuSelection,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPlan {
    pub schema_version: u32,
    pub case: CaseSpec,
    pub stages: Vec<Stage>,
    pub transfers: Vec<TransferSpec>,
    pub observation: ObservationPlan,
    pub fleetix_revision: String,
    pub fleetix_contract_digest: String,
    pub policy: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<RetainedSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frames: Option<FrameSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<FilterSpec>,
}

// Decode versions explicitly: v1's field set remains strict, and v2 requires
// an immutable source. The remote record shares the exact existing field types.
#[derive(Deserialize, JsonSchema)]
#[serde(remote = "ExecutionPlan", deny_unknown_fields)]
struct ExecutionPlanRecord {
    schema_version: u32,
    case: CaseSpec,
    stages: Vec<Stage>,
    transfers: Vec<TransferSpec>,
    observation: ObservationPlan,
    fleetix_revision: String,
    fleetix_contract_digest: String,
    policy: String,
    #[serde(default, deserialize_with = "retained_source")]
    source: Option<RetainedSource>,
    #[serde(default, deserialize_with = "frame_source")]
    frames: Option<FrameSource>,
    #[serde(default, deserialize_with = "filter_spec")]
    filter: Option<FilterSpec>,
}

fn filter_spec<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> std::result::Result<Option<FilterSpec>, D::Error> {
    FilterSpec::deserialize(decoder).map(Some)
}

fn frame_source<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> std::result::Result<Option<FrameSource>, D::Error> {
    FrameSource::deserialize(decoder).map(Some)
}

fn retained_source<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> std::result::Result<Option<RetainedSource>, D::Error> {
    RetainedSource::deserialize(decoder).map(Some)
}

impl<'de> Deserialize<'de> for ExecutionPlan {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        use serde::de::Error;
        let plan = ExecutionPlanRecord::deserialize(decoder)?;
        match (
            plan.schema_version,
            &plan.source,
            &plan.frames,
            &plan.filter,
        ) {
            (1, None, None, None)
            | (2, Some(_), None, None)
            | (3, Some(_), Some(_), None)
            | (4, Some(_), None, Some(_)) => {}
            _ => {
                return Err(D::Error::custom(
                    "explicit v1, retained-source v2, frame-bound v3 or numerical-filter v4 plan required",
                ));
            }
        }
        Ok(plan)
    }
}

impl JsonSchema for ExecutionPlan {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ExecutionPlan".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let mut schema = ExecutionPlanRecord::json_schema(generator);
        if let Some(properties) = schema.get_mut("properties") {
            properties["schema_version"]["enum"] = serde_json::json!([1, 2, 3, 4]);
        }
        schema.insert("allOf".into(), serde_json::json!([
            {"if":{"properties":{"schema_version":{"const":1}}},"then":{"not":{"anyOf":[{"required":["source"]},{"required":["frames"]},{"required":["filter"]}]}}},
            {"if":{"properties":{"schema_version":{"const":2}}},"then":{"required":["source"],"properties":{"source":{"type":"object"}},"not":{"anyOf":[{"required":["frames"]},{"required":["filter"]}]}}},
            {"if":{"properties":{"schema_version":{"const":3}}},"then":{"required":["source","frames"],"properties":{"source":{"type":"object"},"frames":{"type":"object"}},"not":{"required":["filter"]}}},
            {"if":{"properties":{"schema_version":{"const":4}}},"then":{"required":["source","filter"],"properties":{"source":{"type":"object"},"filter":{"type":"object"}},"not":{"required":["frames"]}}}
        ]));
        schema
    }
}
impl ExecutionPlan {
    pub fn numerical_filter(
        original: &Self,
        source: RetainedSource,
        request: FilterRequest,
        policy: String,
    ) -> Result<Self> {
        if source.job_id != request.source_job
            || source.plan_digest != original.id()?
            || source.science_id != original.case.science_id()?
            || !original
                .observation
                .retained_times_s
                .contains(&request.filter.time_s)
            || request.compute.role != Role::Compute
            || request.compute.backend != "hip"
            || request.compute.backend_uuid.is_none()
        {
            return Err(invalid(
                "numerical filter requires exact retained source and HIP compute selection",
            ));
        }
        let mut plan = Self {
            schema_version: 4,
            case: original.case.clone(),
            source: Some(source),
            frames: None,
            filter: Some(request.filter.clone()),
            stages: vec![
                Stage {
                    id: "filter".into(),
                    dependencies: vec![],
                    operation: StageOperation::NumericalFilter,
                    gpu: GpuRequirement::Required,
                    selection: Some(request.compute),
                    ram_bytes: 1,
                    vram_bytes: 1,
                },
                Stage {
                    id: "bundle".into(),
                    dependencies: vec!["filter".into()],
                    operation: StageOperation::Bundle,
                    gpu: GpuRequirement::CpuOnly,
                    selection: None,
                    ram_bytes: 16 * 1024 * 1024,
                    vram_bytes: 0,
                },
            ],
            transfers: vec![],
            observation: ObservationPlan {
                metrics: vec![],
                probes: vec![],
                retained_times_s: vec![request.filter.time_s],
                checkpoint_times_s: vec![],
                preview_times_s: vec![],
                max_artifact_bytes: 0,
                scientific_congestion: "fail".into(),
                preview_may_drop: false,
            },
            fleetix_revision: FLEETIX_REV.into(),
            fleetix_contract_digest: fleetix_digest(),
            policy,
        };
        crate::estimates::minimum(&plan)?.apply(&mut plan);
        plan.validate()?;
        Ok(plan)
    }
    pub fn video(
        render: &Self,
        source: RetainedSource,
        frames: FrameSource,
        request: VideoRequest,
        policy: String,
    ) -> Result<Self> {
        if frames.job_id != request.source_job
            || frames.plan_digest != render.id()?
            || render.case.science_id()? != source.science_id
            || !render
                .stages
                .iter()
                .any(|s| matches!(s.operation, StageOperation::Render))
            || request.media.role != Role::Media
        {
            return Err(invalid(
                "video requires approved rendered-frame science and an independent media role",
            ));
        }
        let mut plan = Self {
            schema_version: 3,
            case: render.case.clone(),
            source: Some(source),
            frames: Some(frames),
            filter: None,
            stages: vec![
                Stage {
                    id: "video".into(),
                    dependencies: vec![],
                    operation: StageOperation::Video,
                    gpu: GpuRequirement::Required,
                    selection: Some(request.media),
                    ram_bytes: 1,
                    vram_bytes: 0,
                },
                Stage {
                    id: "bundle".into(),
                    dependencies: vec!["video".into()],
                    operation: StageOperation::Bundle,
                    gpu: GpuRequirement::CpuOnly,
                    selection: None,
                    ram_bytes: 16 * 1024 * 1024,
                    vram_bytes: 0,
                },
            ],
            transfers: vec![],
            observation: ObservationPlan {
                metrics: vec![],
                probes: vec![],
                retained_times_s: render.observation.retained_times_s.clone(),
                checkpoint_times_s: vec![],
                preview_times_s: vec![],
                max_artifact_bytes: 0,
                scientific_congestion: "fail".into(),
                preview_may_drop: false,
            },
            fleetix_revision: FLEETIX_REV.into(),
            fleetix_contract_digest: fleetix_digest(),
            policy,
        };
        crate::estimates::minimum(&plan)?.apply(&mut plan);
        plan.validate()?;
        Ok(plan)
    }
    pub fn presentation(
        original: &Self,
        source: RetainedSource,
        request: PresentationRequest,
        policy: String,
    ) -> Result<Self> {
        if source.job_id != request.source_job
            || source.plan_digest != original.id()?
            || source.science_id != original.case.science_id()?
            || request
                .times_s
                .iter()
                .any(|t| !original.observation.retained_times_s.contains(t))
            || request.render.role != Role::Render
            || request
                .media
                .as_ref()
                .is_some_and(|s| s.role != Role::Media)
        {
            return Err(invalid(
                "presentation source, retained times or operation roles differ",
            ));
        }
        let mut case = original.case.clone();
        case.presentation = request.presentation;
        let mut stages = vec![Stage {
            id: "render".into(),
            dependencies: vec![],
            operation: StageOperation::Render,
            gpu: GpuRequirement::Required,
            selection: Some(request.render),
            ram_bytes: 1,
            vram_bytes: 0,
        }];
        if let Some(media) = request.media {
            stages.push(Stage {
                id: "video".into(),
                dependencies: vec!["render".into()],
                operation: StageOperation::Video,
                gpu: GpuRequirement::Required,
                selection: Some(media),
                ram_bytes: 1,
                vram_bytes: 0,
            });
        }
        stages.push(Stage {
            id: "bundle".into(),
            dependencies: vec![if stages.len() == 1 { "render" } else { "video" }.into()],
            operation: StageOperation::Bundle,
            gpu: GpuRequirement::CpuOnly,
            selection: None,
            ram_bytes: 16 * 1024 * 1024,
            vram_bytes: 0,
        });
        let mut plan = Self {
            schema_version: 2,
            case,
            stages,
            transfers: vec![],
            observation: ObservationPlan {
                metrics: vec![],
                probes: vec![],
                retained_times_s: request.times_s,
                checkpoint_times_s: vec![],
                preview_times_s: vec![],
                max_artifact_bytes: 0,
                scientific_congestion: "fail".into(),
                preview_may_drop: false,
            },
            fleetix_revision: FLEETIX_REV.into(),
            fleetix_contract_digest: fleetix_digest(),
            policy,
            source: Some(source),
            frames: None,
            filter: None,
        };
        crate::estimates::minimum(&plan)?.apply(&mut plan);
        plan.validate()?;
        Ok(plan)
    }
    pub fn reference(case: CaseSpec) -> Result<Self> {
        let ram_bytes = 16 * 1024 * 1024 + u64::from(case.resolution) * 160;
        let max_artifact_bytes = 1048576u64.max(u64::from(case.resolution) * 128);
        let plan = Self {
            schema_version: 1,
            case,
            stages: vec![Stage {
                id: "reference".into(),
                dependencies: vec![],
                operation: StageOperation::ChannelReference,
                gpu: GpuRequirement::CpuOnly,
                selection: None,
                ram_bytes,
                vram_bytes: 0,
            }],
            transfers: vec![],
            observation: ObservationPlan {
                metrics: vec!["mean_velocity".into()],
                probes: vec![],
                retained_times_s: vec![0.],
                checkpoint_times_s: vec![],
                preview_times_s: vec![],
                max_artifact_bytes,
                scientific_congestion: "fail".into(),
                preview_may_drop: false,
            },
            fleetix_revision: FLEETIX_REV.into(),
            fleetix_contract_digest: fleetix_digest(),
            policy: "ci".into(),
            source: None,
            frames: None,
            filter: None,
        };
        plan.validate()?;
        Ok(plan)
    }
    /// Inspect approved source CAD in the native importer; no solver is implied.
    pub fn cad_inspection(case: CaseSpec, policy: String, max_artifact_bytes: u64) -> Result<Self> {
        if policy == "ci" {
            return Err(invalid("native CAD inspection requires systemd policy"));
        }
        let plan = Self {
            schema_version: 1,
            case,
            stages: vec![
                Stage {
                    id: "cad".into(),
                    dependencies: vec![],
                    operation: StageOperation::CadInspect,
                    gpu: GpuRequirement::CpuOnly,
                    selection: None,
                    ram_bytes: 1024 * 1024 * 1024,
                    vram_bytes: 0,
                },
                Stage {
                    id: "bundle".into(),
                    dependencies: vec!["cad".into()],
                    operation: StageOperation::Bundle,
                    gpu: GpuRequirement::CpuOnly,
                    selection: None,
                    ram_bytes: 16 * 1024 * 1024,
                    vram_bytes: 0,
                },
            ],
            transfers: vec![],
            observation: ObservationPlan {
                metrics: vec!["geometry_regions".into()],
                probes: vec![],
                retained_times_s: vec![],
                checkpoint_times_s: vec![],
                preview_times_s: vec![],
                max_artifact_bytes,
                scientific_congestion: "fail".into(),
                preview_may_drop: false,
            },
            fleetix_revision: FLEETIX_REV.into(),
            fleetix_contract_digest: fleetix_digest(),
            policy,
            source: None,
            frames: None,
            filter: None,
        };
        plan.validate()?;
        Ok(plan)
    }
    pub fn openlb_reference(case: CaseSpec, policy: String) -> Result<Self> {
        case.validate()?;
        if !case.geometry.synthetic
            || case.applicability.formulation != "periodic_forced_channel"
            || policy == "ci"
        {
            return Err(invalid(
                "native CPU reference requires synthetic periodic channel and systemd policy",
            ));
        }
        let stages = vec![
            Stage {
                id: "cad".into(),
                dependencies: vec![],
                operation: StageOperation::CadFixture,
                gpu: GpuRequirement::CpuOnly,
                selection: None,
                ram_bytes: 1024 * 1024 * 1024,
                vram_bytes: 0,
            },
            Stage {
                id: "flow".into(),
                dependencies: vec!["cad".into()],
                operation: StageOperation::Openlb,
                gpu: GpuRequirement::CpuOnly,
                selection: None,
                ram_bytes: 0,
                vram_bytes: 0,
            },
            Stage {
                id: "bundle".into(),
                dependencies: vec!["flow".into()],
                operation: StageOperation::Bundle,
                gpu: GpuRequirement::CpuOnly,
                selection: None,
                ram_bytes: 16 * 1024 * 1024,
                vram_bytes: 0,
            },
        ];
        let end = case.max_time_s;
        let mut plan = Self {
            schema_version: 1,
            case,
            stages,
            transfers: vec![],
            observation: ObservationPlan {
                metrics: vec!["channel_relative_l2_error".into()],
                probes: vec![],
                retained_times_s: vec![0., end / 2., end],
                checkpoint_times_s: vec![],
                preview_times_s: vec![],
                max_artifact_bytes: 0,
                scientific_congestion: "fail".into(),
                preview_may_drop: false,
            },
            fleetix_revision: FLEETIX_REV.into(),
            fleetix_contract_digest: fleetix_digest(),
            policy,
            source: None,
            frames: None,
            filter: None,
        };
        crate::estimates::minimum(&plan)?.apply(&mut plan);
        plan.validate()?;
        Ok(plan)
    }
    pub fn id(&self) -> Result<String> {
        digest(self)
    }
    pub fn peak_ram(&self) -> u64 {
        self.stages.iter().map(|s| s.ram_bytes).max().unwrap_or(0)
    }
    pub fn disk_reservation(&self) -> Result<u64> {
        let copies = if self.stages.iter().any(|s| {
            !matches!(
                s.operation,
                StageOperation::ChannelReference | StageOperation::Bundle
            )
        }) {
            2
        } else {
            1
        };
        self.observation
            .max_artifact_bytes
            .checked_mul(copies)
            .ok_or_else(|| invalid("disk reservation overflow"))
    }
    pub fn b1(case: CaseSpec, selections: B1Selections, policy: String) -> Result<Self> {
        if selections.compute.role != Role::Compute
            || !["hip", "cuda"].contains(&selections.compute.backend.as_str())
            || selections
                .compute
                .backend_uuid
                .as_ref()
                .is_none_or(|id| !token(id))
            || selections.render.role != Role::Render
            || selections.render.backend != "egl"
            || selections.media.role != Role::Media
            || selections.media.backend != "vaapi"
        {
            return Err(invalid(
                "B1 requires independent HIP or CUDA UUID/PCI, EGL and VAAPI selections",
            ));
        }
        let mut plan = Self::openlb_reference(case, policy)?;
        plan.stages.pop();
        let flow = &mut plan.stages[1];
        flow.gpu = GpuRequirement::Required;
        flow.selection = Some(selections.compute);
        flow.vram_bytes = flow.ram_bytes;
        let pixels =
            u64::from(plan.case.presentation.width) * u64::from(plan.case.presentation.height);
        plan.stages.extend([
            Stage {
                id: "render".into(),
                dependencies: vec!["flow".into()],
                operation: StageOperation::Render,
                gpu: GpuRequirement::Required,
                selection: Some(selections.render),
                ram_bytes: 1024 * 1024 * 1024,
                vram_bytes: 128 * 1024 * 1024 + pixels * 32,
            },
            Stage {
                id: "video".into(),
                dependencies: vec!["render".into()],
                operation: StageOperation::Video,
                gpu: GpuRequirement::Required,
                selection: Some(selections.media),
                ram_bytes: 512 * 1024 * 1024,
                vram_bytes: 128 * 1024 * 1024 + pixels * 16,
            },
            Stage {
                id: "bundle".into(),
                dependencies: vec!["video".into()],
                operation: StageOperation::Bundle,
                gpu: GpuRequirement::CpuOnly,
                selection: None,
                ram_bytes: 16 * 1024 * 1024,
                vram_bytes: 0,
            },
        ]);
        crate::estimates::minimum(&plan)?.apply(&mut plan);
        plan.validate()?;
        Ok(plan)
    }
    pub fn validate(&self) -> Result<()> {
        self.case.validate()?;
        if !matches!(
            (
                self.schema_version,
                &self.source,
                &self.frames,
                &self.filter
            ),
            (1, None, None, None)
                | (2, Some(_), None, None)
                | (3, Some(_), Some(_), None)
                | (4, Some(_), None, Some(_))
        ) || self.fleetix_revision != FLEETIX_REV
            || self.fleetix_contract_digest != fleetix_digest()
        {
            return Err(invalid("schema/Fleetix source or contract drift"));
        }
        if !["ci", "prototype", "production", "research"].contains(&self.policy.as_str()) {
            return Err(invalid("policy"));
        }
        if let Some(source) = &self.source {
            let hashes = [
                &source.plan_digest,
                &source.execution_binding_digest,
                &source.authorization_digest,
                &source.snapshot_sha256,
                &source.artifact_id,
                &source.science_id,
            ];
            if self.policy == "ci"
                || uuid::Uuid::parse_str(&source.job_id).is_err()
                || hashes.iter().any(|s| {
                    s.len() != 64
                        || !s
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                })
                || source.science_id != self.case.science_id()?
                || source.bytes == 0
                || source.bytes > self.observation.max_artifact_bytes
                || self.case.presentation.field != "velocity"
                || self.observation.retained_times_s.is_empty()
                || !self.observation.metrics.is_empty()
                || !self.observation.probes.is_empty()
                || !self.observation.preview_times_s.is_empty()
                || !self.observation.checkpoint_times_s.is_empty()
                || self.observation.preview_may_drop
            {
                return Err(invalid(
                    "presentation requires exact source science, retained times and bounded supported fields",
                ));
            }
            let operations: Vec<_> = self.stages.iter().map(|s| &s.operation).collect();
            let render = matches!(
                operations.as_slice(),
                [StageOperation::Render, StageOperation::Bundle]
                    | [
                        StageOperation::Render,
                        StageOperation::Video,
                        StageOperation::Bundle
                    ]
            );
            let video = matches!(
                operations.as_slice(),
                [StageOperation::Video, StageOperation::Bundle]
            );
            let numerical = matches!(
                operations.as_slice(),
                [StageOperation::NumericalFilter, StageOperation::Bundle]
            );
            if (self.filter.is_some() && !numerical)
                || (self.filter.is_none() && self.frames.is_none() && !render)
                || (self.frames.is_some() && !video)
            {
                return Err(invalid(
                    "source-bound plans require their exact render, video or numerical-filter DAG",
                ));
            }
            if let Some(filter) = &self.filter
                && (!filter.time_s.is_finite()
                    || self.observation.retained_times_s != vec![filter.time_s])
            {
                return Err(invalid(
                    "numerical filter requires one exact retained physical time",
                ));
            }
            if let Some(frames) = &self.frames {
                let hashes = [
                    &frames.plan_digest,
                    &frames.execution_binding_digest,
                    &frames.authorization_digest,
                    &frames.sequence_sha256,
                ];
                if uuid::Uuid::parse_str(&frames.job_id).is_err()
                    || frames.bytes == 0
                    || frames.bytes > self.observation.max_artifact_bytes
                    || hashes.iter().any(|h| {
                        h.len() != 64
                            || !h
                                .bytes()
                                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    })
                {
                    return Err(invalid(
                        "exact bounded frame-source execution identities required",
                    ));
                }
            }
            for (index, stage) in self.stages.iter().enumerate() {
                let dependencies = if index == 0 {
                    vec![]
                } else {
                    vec![self.stages[index - 1].id.clone()]
                };
                if stage.dependencies != dependencies
                    || stage.selection.as_ref().is_some_and(|s| {
                        if matches!(stage.operation, StageOperation::NumericalFilter) {
                            s.role != Role::Compute
                                || s.backend != "hip"
                                || s.backend_uuid.as_deref().is_none_or(|id| !token(id))
                        } else {
                            s.backend_uuid.is_some()
                                || s.backend
                                    != if s.role == Role::Render {
                                        "egl"
                                    } else {
                                        "vaapi"
                                    }
                        }
                    })
                {
                    return Err(invalid(
                        "presentation requires exact sequential dependencies and EGL/VAAPI roles",
                    ));
                }
            }
        }
        if self.stages.is_empty() || self.stages.len() > 32 {
            return Err(invalid("bounded nonempty DAG required"));
        }
        let mut seen = BTreeSet::new();
        let mut operations = BTreeSet::new();
        for stage in &self.stages {
            if matches!(stage.operation, StageOperation::NumericalFilter) && self.filter.is_none() {
                return Err(invalid(
                    "numerical compute stage requires a source-bound version-4 filter specification",
                ));
            }
            let op_name = serde_json::to_string(&stage.operation)?;
            if !operations.insert(op_name) {
                return Err(invalid(
                    "one stage per fixed-output adapter operation required",
                ));
            }
            if !token(&stage.id)
                || seen.contains(&stage.id)
                || stage.dependencies.iter().any(|d| !seen.contains(d))
            {
                return Err(invalid(
                    "DAG requires unique IDs and topologically ordered dependencies",
                ));
            }
            if stage.ram_bytes == 0 {
                return Err(invalid("explicit RAM estimate required"));
            }
            if matches!(stage.operation, StageOperation::ChannelReference)
                && self.case.applicability.formulation != "steady_incompressible_channel"
            {
                return Err(invalid("analytical adapter formulation mismatch"));
            }
            if matches!(stage.operation, StageOperation::Openlb)
                && (!self.case.geometry.synthetic
                    || self.case.applicability.formulation != "periodic_forced_channel")
            {
                return Err(invalid(
                    "OpenLB adapter supports only the synthetic periodic channel",
                ));
            }
            if matches!(stage.operation, StageOperation::CadInspect)
                && (self.case.geometry.sha256.as_ref().is_none_or(|s| {
                    s.len() != 64
                        || !s
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                }) || self.case.geometry.source.is_empty()
                    || std::path::Path::new(&self.case.geometry.source)
                        .components()
                        .any(|c| !matches!(c, std::path::Component::Normal(_))))
            {
                return Err(invalid(
                    "CAD inspection requires a scoped relative source and lowercase SHA-256 approval",
                ));
            }
            let expected_role = match stage.operation {
                StageOperation::Openlb if stage.gpu != GpuRequirement::CpuOnly => {
                    Some(Role::Compute)
                }
                StageOperation::Render => Some(Role::Render),
                StageOperation::Video => Some(Role::Media),
                StageOperation::NumericalFilter => Some(Role::Compute),
                _ => None,
            };
            if let Some(role) = expected_role {
                if stage.gpu != GpuRequirement::Required
                    || stage.selection.as_ref().map(|s| s.role) != Some(role)
                {
                    return Err(invalid(
                        "native B1 operations require explicit role/device selection; no fallback",
                    ));
                }
            } else if stage.gpu != GpuRequirement::CpuOnly
                || stage.selection.is_some()
                || stage.vram_bytes != 0
            {
                return Err(invalid("CPU operation cannot claim GPU execution"));
            }
            seen.insert(stage.id.clone());
        }
        if let Some(index) = self
            .stages
            .iter()
            .position(|s| matches!(s.operation, StageOperation::Bundle))
            && index != self.stages.len() - 1
        {
            return Err(invalid("bundle must be the final stage"));
        }
        if !self.transfers.is_empty() {
            return Err(Error::Unqualified(
                "coupled transfers require model-specific conservation qualification".into(),
            ));
        }
        if self.observation.max_artifact_bytes < u64::from(self.case.resolution) * 128
            || self.observation.max_artifact_bytes > 1_000_000_000_000
            || self.observation.scientific_congestion != "fail"
        {
            return Err(invalid("scientific output budget/congestion policy"));
        }
        if self.observation.probes.len() > 256
            || self
                .observation
                .probes
                .iter()
                .flatten()
                .any(|v| !v.is_finite())
        {
            return Err(invalid("bounded finite probes required"));
        }
        for times in [
            &self.observation.retained_times_s,
            &self.observation.checkpoint_times_s,
            &self.observation.preview_times_s,
        ] {
            if times.len() > 1024
                || times
                    .iter()
                    .any(|t| !t.is_finite() || *t < 0. || *t > self.case.max_time_s)
                || times.windows(2).any(|p| p[0] >= p[1])
            {
                return Err(invalid("ordered, bounded observation times required"));
            }
        }
        if !self.observation.checkpoint_times_s.is_empty() {
            return Err(Error::Unqualified("no verified checkpoint adapter".into()));
        }
        if self
            .stages
            .iter()
            .any(|s| matches!(s.operation, StageOperation::Openlb))
        {
            self.validate_openlb_lattice()?;
        }
        crate::estimates::minimum(self)?.validate(self)?;
        Ok(())
    }
    fn validate_openlb_lattice(&self) -> Result<()> {
        let c = &self.case;
        let height = c.channel_height.si("length")?;
        let dx = height / f64::from(c.resolution);
        let viscosity = c.kinematic_viscosity.si("kinematic_viscosity")?;
        let velocity =
            c.acceleration.si("acceleration")?.abs() * height * height / (8. * viscosity);
        // Preserve the order in OpenLB's pinned converter constructor, including
        // Float64 tau subtraction, rather than replacing it with an ideal 0.1.
        let dt = ((0.8f64 - 0.5) / 3.) * (dx * dx) / viscosity;
        let mach = 3f64.sqrt() * (velocity / (dx / dt));
        if velocity == 0. || !mach.is_finite() || mach > 0.1 {
            return Err(invalid(
                "fixed BGK reference requires nonzero drive and lattice Mach <= 0.1; parameters preserved",
            ));
        }
        let nx = c.length.si("length")? / dx;
        if !nx.is_finite() || nx < 1. || (nx - nx.round()).abs() > 1e-8 {
            return Err(invalid(
                "integral periodic lattice extent required; geometry cannot be silently rounded",
            ));
        }
        // OpenLB 1.9 unitConverter.h getLatticeTime uses size_t(t/dt+0.5).
        let lattice_time = |time: f64| -> Result<u64> {
            let step = (time / dt + 0.5).floor();
            if !dt.is_finite()
                || dt <= 0.
                || !step.is_finite()
                || step < 0.
                || step >= u64::MAX as f64
            {
                return Err(invalid("lattice time conversion overflow"));
            }
            Ok(step as u64)
        };
        let end = lattice_time(c.max_time_s)?;
        if end == 0 || self.observation.retained_times_s.last() != Some(&c.max_time_s) {
            return Err(invalid(
                "native duration must reach a lattice step and final scientific state must be retained",
            ));
        }
        let mut prior = None;
        for time in &self.observation.retained_times_s {
            let step = lattice_time(*time)?;
            if step > end || prior == Some(step) {
                return Err(invalid(
                    "retained times collapse to one lattice step or exceed duration; reapprove explicitly",
                ));
            }
            prior = Some(step);
        }
        Ok(())
    }
}
pub fn fleetix_digest() -> String {
    format!(
        "{:x}",
        Sha256::digest(fleetix::gpu::CONTRACT_JSON.as_bytes())
    )
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactManifest {
    pub schema_version: u32,
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub format: String,
    pub provenance: String,
    pub units: Option<String>,
    pub time_s: Option<f64>,
    pub association: Option<String>,
}
pub fn default_artifact_limit() -> u32 {
    20
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactPage {
    pub items: Vec<ArtifactManifest>,
    pub total: u64,
    pub next_after: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ValidationReport {
    pub process: String,
    pub convergence: String,
    pub numerical_verification: String,
    pub physical_validation: String,
    pub limitations: Vec<String>,
    pub moisture_risk: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HostExecutionProfile {
    pub schema_version: u32,
    pub policy: String,
    pub allowed_input_root: String,
    pub max_ram_bytes: u64,
    pub max_disk_bytes: u64,
    pub threads: u32,
    pub timeout_seconds: u32,
    pub native_runtime: Option<String>,
    pub service_mode: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Doctor,
    BackendList,
    Validate {
        case: Box<CaseSpec>,
    },
    Plan {
        case: Box<CaseSpec>,
    },
    PlanOpenlbReference {
        case: Box<CaseSpec>,
    },
    PlanCadInspection {
        case: Box<CaseSpec>,
        max_artifact_bytes: u64,
    },
    PlanB1 {
        case: Box<CaseSpec>,
        selections: B1Selections,
    },
    PlanPresentation {
        request: Box<PresentationRequest>,
    },
    ValidateColdRestart {
        case: Box<crate::recipes::ColdRestartSpec>,
    },
    PlanVideo {
        request: Box<VideoRequest>,
    },
    PlanFilter {
        request: Box<FilterRequest>,
    },
    Submit {
        plan: Box<ExecutionPlan>,
        approved_digest: String,
        idempotency_key: String,
    },
    Status {
        job_id: String,
    },
    Logs {
        job_id: String,
        after: u64,
        limit: u32,
    },
    Cancel {
        job_id: String,
    },
    Artifacts {
        job_id: String,
        #[serde(default)]
        after: Option<String>,
        #[serde(default = "default_artifact_limit")]
        limit: u32,
    },
    Describe {
        job_id: String,
    },
    QualificationReport {
        job_id: String,
    },
    CadRegions {
        job_id: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkerRequest {
    pub protocol_version: u32,
    pub request_id: String,
    pub request: Operation,
}
pub fn schemas() -> serde_json::Value {
    serde_json::json!({"CaseSpec": schemars::schema_for!(CaseSpec), "PhysicsApplicability": schemars::schema_for!(PhysicsApplicability),
        "ExecutionPlan": schemars::schema_for!(ExecutionPlan), "TransferSpec": schemars::schema_for!(TransferSpec),
        "HostExecutionProfile": schemars::schema_for!(HostExecutionProfile), "GpuSelection": schemars::schema_for!(GpuSelection),
        "ObservationPlan": schemars::schema_for!(ObservationPlan), "ArtifactManifest": schemars::schema_for!(ArtifactManifest),
        "B1Selections": schemars::schema_for!(B1Selections),
        "ArtifactPage": schemars::schema_for!(ArtifactPage),
        "ExecutionBinding": schemars::schema_for!(crate::execution::ExecutionBinding),
        "HostAuthority": schemars::schema_for!(crate::authority::HostAuthority),
        "ExecutionAuthorization": schemars::schema_for!(crate::authority::ExecutionAuthorization),
        "FieldSnapshot": schemars::schema_for!(crate::fields::FieldSnapshot),
        "RetainedSource": schemars::schema_for!(RetainedSource),
        "PresentationRequest": schemars::schema_for!(PresentationRequest),
        "FrameSource": schemars::schema_for!(FrameSource),
        "VideoRequest": schemars::schema_for!(VideoRequest),
        "FilterRequest": schemars::schema_for!(FilterRequest),
        "JobEvidenceReport": schemars::schema_for!(crate::qualification::JobEvidenceReport),
        "RegionReport": schemars::schema_for!(crate::cad::RegionReport),
        "ConservativeTransfer": schemars::schema_for!(crate::transfers::ConservativeTransfer),
        "TransferReceipt": schemars::schema_for!(crate::transfers::TransferReceipt),
        "ThermalMaterial": schemars::schema_for!(crate::materials::ThermalMaterial),
        "ColdRestartSpec": schemars::schema_for!(crate::recipes::ColdRestartSpec),
        "ValidationReport": schemars::schema_for!(ValidationReport), "WorkerRequest": schemars::schema_for!(WorkerRequest)})
}
