//! Conservative scalar one-way maps. Geometric overlap must be supplied and
//! validated by the mesh adapter; this module never guesses a correspondence.
use crate::{
    Result,
    contracts::{digest, invalid},
    science::Quantity,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransferQuantity {
    Temperature,
    SurfaceHeatFlux,
    Irradiance,
    MassDensity,
}

impl TransferQuantity {
    fn units(self) -> (&'static str, &'static str, &'static str, &'static str) {
        match self {
            Self::Temperature => ("temperature", "K", "heat_capacity", "J/K"),
            Self::SurfaceHeatFlux | Self::Irradiance => ("irradiance", "W/m2", "area", "m2"),
            Self::MassDensity => ("density", "kg/m3", "volume", "m3"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Association {
    Point,
    Cell,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NormalMapping {
    SameDirection,
    OpposingOutwardNormals,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransferEndpoint {
    pub mesh_sha256: String,
    pub region: String,
    pub association: Association,
    pub orientation: [f64; 3],
    /// Thermal capacitances for temperature; areas for flux/irradiance;
    /// volumes for retained mass density. Point measures are explicit lumped
    /// measures, not node counts or an implicit cell-to-point average.
    pub measures: Vec<Quantity>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Overlap {
    pub source: usize,
    pub destination: usize,
    pub measure: Quantity,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConservativeTransfer {
    pub schema_version: u32,
    pub source_artifact_sha256: String,
    pub quantity: TransferQuantity,
    pub source: TransferEndpoint,
    pub destination: TransferEndpoint,
    pub normal_mapping: NormalMapping,
    pub interpolation: String,
    pub overlaps: Vec<Overlap>,
    pub maximum_relative_conservation_error: f64,
}

fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransferReceipt {
    pub schema_version: u32,
    pub transfer_id: String,
    pub original_source_unit: String,
    pub destination_unit: String,
    pub conserved_quantity: String,
    pub integral_unit: String,
    pub source_integral: f64,
    pub destination_integral: f64,
    pub relative_conservation_error: f64,
    pub physical_validation: String,
}

pub struct TransferredField {
    /// Persist these arrays in an artifact; transport carries the receipt only.
    pub values_si: Vec<f64>,
    pub receipt: TransferReceipt,
}

impl ConservativeTransfer {
    pub fn apply(&self, values: &[f64], source_unit: &str) -> Result<TransferredField> {
        self.validate()?;
        if values.len() != self.source.measures.len() {
            return Err(invalid(
                "source field association/extent differs from transfer map",
            ));
        }
        let (dimension, unit, measure_dimension, _) = self.quantity.units();
        let values: Vec<_> = values
            .iter()
            .map(|value| {
                Quantity {
                    value: *value,
                    unit: source_unit.into(),
                }
                .si(dimension)
            })
            .collect::<Result<_>>()?;
        if self.quantity != TransferQuantity::SurfaceHeatFlux && values.iter().any(|v| *v < 0.) {
            return Err(invalid(
                "nonnegative physical temperature, irradiance or mass density required",
            ));
        }
        let mut mapped = vec![0.; self.destination.measures.len()];
        for entry in &self.overlaps {
            mapped[entry.destination] +=
                entry.measure.si(measure_dimension)? * values[entry.source];
        }
        let sign = if self.normal_mapping == NormalMapping::OpposingOutwardNormals {
            -1.
        } else {
            1.
        };
        let mut source_integral = 0.;
        let mut scale = 0.;
        for (value, measure) in values.iter().zip(&self.source.measures) {
            let integrated = value * measure.si(measure_dimension)?;
            source_integral += integrated;
            scale += integrated.abs();
        }
        let mut destination_integral = 0.;
        for (value, measure) in mapped.iter_mut().zip(&self.destination.measures) {
            destination_integral += sign * *value;
            *value = sign * *value / measure.si(measure_dimension)?;
        }
        let error = (destination_integral - sign * source_integral).abs();
        let relative = if scale == 0. { error } else { error / scale };
        if mapped.iter().any(|v| !v.is_finite())
            || !source_integral.is_finite()
            || !destination_integral.is_finite()
            || !scale.is_finite()
            || !relative.is_finite()
            || relative > self.maximum_relative_conservation_error
        {
            return Err(invalid(
                "transfer integral overflow or conservation loss exceeds approved tolerance",
            ));
        }
        let (conserved_quantity, integral_unit) = match self.quantity {
            TransferQuantity::Temperature => ("constant_capacitance_weighted_temperature", "J"),
            TransferQuantity::SurfaceHeatFlux => ("signed_heat_power", "W"),
            TransferQuantity::Irradiance => ("incident_radiant_power", "W"),
            TransferQuantity::MassDensity => ("retained_mass", "kg"),
        };
        Ok(TransferredField {
            values_si: mapped,
            receipt: TransferReceipt {
                schema_version: 1,
                transfer_id: self.id()?,
                original_source_unit: source_unit.into(),
                destination_unit: unit.into(),
                conserved_quantity: conserved_quantity.into(),
                integral_unit: integral_unit.into(),
                source_integral,
                destination_integral,
                relative_conservation_error: relative,
                physical_validation: "unqualified".into(),
            },
        })
    }
    pub fn validate(&self) -> Result<()> {
        let tolerance = self.maximum_relative_conservation_error;
        if self.schema_version != 1
            || !hash(&self.source_artifact_sha256)
            || self.interpolation != "piecewise_constant_overlap"
            || !tolerance.is_finite()
            || !(0. ..1.).contains(&tolerance)
            || self.overlaps.is_empty()
            || self.overlaps.len() > 1_000_000
            || (self.normal_mapping == NormalMapping::OpposingOutwardNormals
                && self.quantity != TransferQuantity::SurfaceHeatFlux)
        {
            return Err(invalid(
                "versioned bounded conservative overlap map required",
            ));
        }
        let (_, _, dimension, _) = self.quantity.units();
        let mut totals = Vec::new();
        for endpoint in [&self.source, &self.destination] {
            let norm: f64 = endpoint.orientation.iter().map(|v| v * v).sum();
            if !hash(&endpoint.mesh_sha256)
                || endpoint.region.is_empty()
                || endpoint.region.len() > 64
                || !endpoint
                    .region
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                || !norm.is_finite()
                || (norm - 1.).abs() > 1e-12
                || endpoint.measures.is_empty()
                || endpoint.measures.len() > 1_000_000
            {
                return Err(invalid(
                    "exact mesh, bounded region, association and unit orientation required",
                ));
            }
            let measures: Vec<_> = endpoint
                .measures
                .iter()
                .map(|m| m.si(dimension))
                .collect::<Result<_>>()?;
            if measures.iter().any(|m| *m <= 0.) {
                return Err(invalid("positive physical transfer measures required"));
            }
            totals.push(measures);
        }
        let dot: f64 = self
            .source
            .orientation
            .iter()
            .zip(&self.destination.orientation)
            .map(|(a, b)| a * b)
            .sum();
        let expected = if self.normal_mapping == NormalMapping::SameDirection {
            1.
        } else {
            -1.
        };
        if (dot - expected).abs() > 1e-12 {
            return Err(invalid(
                "transfer orientation differs from declared normal mapping",
            ));
        }
        let mut sums = [vec![0.; totals[0].len()], vec![0.; totals[1].len()]];
        let mut seen = BTreeSet::new();
        for entry in &self.overlaps {
            let weight = entry.measure.si(dimension)?;
            if weight <= 0.
                || entry.source >= sums[0].len()
                || entry.destination >= sums[1].len()
                || !seen.insert((entry.source, entry.destination))
            {
                return Err(invalid("bounded positive unique overlap entries required"));
            }
            sums[0][entry.source] += weight;
            sums[1][entry.destination] += weight;
        }
        for (sum, declared) in sums.iter().zip(&totals) {
            if sum
                .iter()
                .zip(declared)
                .any(|(s, d)| !s.is_finite() || (*s - *d).abs() / d > tolerance)
            {
                return Err(invalid(
                    "overlaps fail source or destination measure conservation; no silent renormalization",
                ));
            }
        }
        Ok(())
    }
    pub fn id(&self) -> Result<String> {
        self.validate()?;
        digest(self)
    }
}
