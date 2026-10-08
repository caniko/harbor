//! Bounded prescribed snow/aperture geometry. No deposition or flow solver.
use crate::{
    Result,
    contracts::{digest, invalid, token},
    science::Quantity,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NormalAxis {
    X,
    Y,
    Z,
}
impl NormalAxis {
    fn index(&self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanarOpening {
    pub name: String,
    pub normal_axis: NormalAxis,
    pub plane: Quantity,
    /// Min/max along the two other world axes, in increasing axis order.
    pub rectangle: [[Quantity; 2]; 2],
    pub provenance: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SnowPrism {
    pub name: String,
    /// World x/y/z min/max, with explicit units.
    pub bounds: [[Quantity; 2]; 3],
    pub provenance: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SnowOpeningRequest {
    pub schema_version: u32,
    pub synthetic: bool,
    pub model: String,
    pub geometry_tolerance: Quantity,
    pub provenance: String,
    pub openings: Vec<PlanarOpening>,
    pub snow: Vec<SnowPrism>,
}
#[derive(Debug, Serialize, JsonSchema)]
pub struct OpeningCoverage {
    pub name: String,
    pub plane_m: f64,
    pub rectangle_m: [[f64; 2]; 2],
    pub intersecting_prisms: Vec<String>,
    pub original_area_m2: f64,
    pub covered_area_m2: f64,
    pub remaining_area_m2: f64,
    pub covered_fraction: f64,
    /// Geometric area status; no permeability or functional claim.
    pub coverage: String,
}
#[derive(Debug, Serialize, JsonSchema)]
pub struct PreparedSnowOpenings {
    pub schema_version: u32,
    pub preparation_id: String,
    pub input: SnowOpeningRequest,
    pub openings: Vec<OpeningCoverage>,
    pub geometry_tolerance_m: f64,
    pub method: String,
    pub executed: bool,
    pub physical_validation: String,
}

fn provenance(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 4096
}
fn coordinate(q: &Quantity, tolerance: f64) -> Result<f64> {
    let value = q.si("length")?;
    if value.abs() > 1e6 || 16. * (value.abs().next_up() - value.abs()) > tolerance {
        return Err(invalid(
            "prescribed opening coordinates unresolved at declared tolerance",
        ));
    }
    Ok(value)
}
fn interval(q: &[Quantity; 2], tolerance: f64) -> Result<[f64; 2]> {
    let v = [coordinate(&q[0], tolerance)?, coordinate(&q[1], tolerance)?];
    if !(100. * tolerance..=1000.).contains(&(v[1] - v[0])) {
        return Err(invalid(
            "positive bounded prism/opening span must resolve declared tolerance",
        ));
    }
    Ok(v)
}

/// Partition the aperture by all clipped rectangle edges. Each open partition
/// contributes once, so overlapping snow cannot double-count and a tiny
/// remaining slit is accumulated directly instead of full-minus-covered loss.
fn areas(aperture: [[f64; 2]; 2], rectangles: &[[[f64; 2]; 2]]) -> (f64, f64) {
    let mut edges = [
        vec![aperture[0][0], aperture[0][1]],
        vec![aperture[1][0], aperture[1][1]],
    ];
    for rectangle in rectangles {
        for axis in 0..2 {
            edges[axis].extend_from_slice(&rectangle[axis]);
        }
    }
    for axis in &mut edges {
        axis.sort_by(f64::total_cmp);
        axis.dedup();
    }
    let mut sums = [0.; 2];
    let mut corrections = [0.; 2];
    for x in edges[0].windows(2) {
        for y in edges[1].windows(2) {
            // Bound comparisons avoid midpoint rounding across narrow slits.
            let covered = rectangles
                .iter()
                .any(|r| r[0][0] <= x[0] && x[1] <= r[0][1] && r[1][0] <= y[0] && y[1] <= r[1][1]);
            let index = usize::from(covered);
            let area = (x[1] - x[0]) * (y[1] - y[0]);
            let term = area - corrections[index];
            let updated = sums[index] + term;
            corrections[index] = (updated - sums[index]) - term;
            sums[index] = updated;
        }
    }
    (sums[1], sums[0])
}

pub fn prepare(spec: &SnowOpeningRequest) -> Result<PreparedSnowOpenings> {
    let tolerance = spec.geometry_tolerance.si("length")?;
    if spec.schema_version != 1
        || !spec.synthetic
        || spec.model != "prescribed_closed_prisms"
        || !provenance(&spec.provenance)
        || !(1e-12..=1e-3).contains(&tolerance)
        || !(1..=16).contains(&spec.openings.len())
        || spec.snow.len() > 32
    {
        return Err(invalid(
            "bounded explicit synthetic opening/prism geometry and provenance required",
        ));
    }
    let mut names = BTreeSet::new();
    let mut snow = Vec::new();
    for prism in &spec.snow {
        if !token(&prism.name) || !names.insert(&prism.name) || !provenance(&prism.provenance) {
            return Err(invalid(
                "unique named prescribed snow prisms and provenance required",
            ));
        }
        snow.push([
            interval(&prism.bounds[0], tolerance)?,
            interval(&prism.bounds[1], tolerance)?,
            interval(&prism.bounds[2], tolerance)?,
        ]);
    }
    names.clear();
    let mut openings = Vec::new();
    for opening in &spec.openings {
        if !token(&opening.name) || !names.insert(&opening.name) || !provenance(&opening.provenance)
        {
            return Err(invalid(
                "unique named planar openings and provenance required",
            ));
        }
        let plane = coordinate(&opening.plane, tolerance)?;
        let aperture = [
            interval(&opening.rectangle[0], tolerance)?,
            interval(&opening.rectangle[1], tolerance)?,
        ];
        let normal = opening.normal_axis.index();
        let axes: Vec<_> = (0..3).filter(|i| *i != normal).collect();
        let mut rectangles = Vec::new();
        let mut intersecting_prisms = Vec::new();
        for (index, bounds) in snow.iter().enumerate() {
            if bounds[normal][0] <= plane && plane <= bounds[normal][1] {
                let clipped = [
                    [
                        aperture[0][0].max(bounds[axes[0]][0]),
                        aperture[0][1].min(bounds[axes[0]][1]),
                    ],
                    [
                        aperture[1][0].max(bounds[axes[1]][0]),
                        aperture[1][1].min(bounds[axes[1]][1]),
                    ],
                ];
                if clipped.iter().all(|v| v[0] < v[1]) {
                    rectangles.push(clipped);
                    intersecting_prisms.push(spec.snow[index].name.clone());
                }
            }
        }
        let original_area_m2 =
            (aperture[0][1] - aperture[0][0]) * (aperture[1][1] - aperture[1][0]);
        let (covered_area_m2, remaining_area_m2) = areas(aperture, &rectangles);
        if (covered_area_m2 + remaining_area_m2 - original_area_m2).abs() > 1e-12 * original_area_m2
            || covered_area_m2 > original_area_m2
            || remaining_area_m2 > original_area_m2
        {
            return Err(invalid(
                "prescribed opening partition failed area conservation",
            ));
        }
        openings.push(OpeningCoverage {
            name: opening.name.clone(),
            plane_m: plane,
            rectangle_m: aperture,
            intersecting_prisms,
            original_area_m2,
            covered_area_m2,
            remaining_area_m2,
            covered_fraction: covered_area_m2 / original_area_m2,
            coverage: if covered_area_m2 == 0. {
                "clear"
            } else if remaining_area_m2 == 0. {
                "fully_covered"
            } else {
                "partially_covered"
            }
            .into(),
        });
    }
    let report = PreparedSnowOpenings { schema_version: 1, preparation_id: digest(spec)?, input: spec.clone(), openings,
        geometry_tolerance_m: tolerance,
        method: "closed-prism plane intersection; clipped rectangle union by disjoint edge partitions; compensated Float64 area sums; no tolerance expansion or rounding".into(),
        executed: false, physical_validation: "unqualified".into(),
    };
    if serde_json::to_vec(&report)?.len() as u64 > crate::contracts::MAX_MESSAGE - 2048 {
        return Err(invalid(
            "bounded complete snow-opening response exceeds worker message allowance",
        ));
    }
    Ok(report)
}
