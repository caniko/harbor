//! Independent original-midpoint/native-packet reconstruction, without runtime promotion.
use crate::{
    Result,
    atmosphere::AtmosphericReferenceSpec,
    atmosphere_transfer::{AtmosphericTransferRequest, propagation, reference},
    contracts::invalid,
    radiation::product_integral,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AtmosphericComponent {
    Direct,
    Diffuse,
}

/// Optical-product reductions in original spectral units; no execution attestation.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AtmosphericPacketChannels {
    pub incident_w_m2: f64,
    pub absorbed_w_m2: f64,
    pub ageing_w_m2: f64,
}

struct Emitter {
    direction: [f64; 3],
    irradiance: Vec<f64>,
    pmf: f64,
}

fn scalar(text: &str) -> Result<f64> {
    text.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| invalid("finite original native atmospheric spectral scalar required"))
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn add(total: &mut f64, correction: &mut f64, value: f64) {
    let adjusted = value - *correction;
    let next = *total + adjusted;
    *correction = (next - *total) - adjusted;
    *total = next;
}

/// Reconstruct complete four-lane CSV packets against unchanged source originals.
///
/// This checks original source/receiver applicability, ordered sample/knot
/// coverage, physical rectangle, emitter identity/direction, source-only PMFs,
/// original Float32 weights and the unchanged analytical gate. The caller must
/// separately bind registered artifacts, approved seeds, receipt, runtime,
/// source execution and operation sandbox before making any execution claim.
pub fn reconstruct_native_packets(
    source: &AtmosphericReferenceSpec,
    request: &AtmosphericTransferRequest,
    original: &str,
    component: AtmosphericComponent,
    packets: &str,
) -> Result<AtmosphericPacketChannels> {
    if original.len() > 32 * 1024 * 1024 {
        return Err(invalid("bounded unchanged atmospheric source required"));
    }
    let surface = reference(source, request, original)?;
    let prepared = source.prepare()?;
    let receiver = &request.receiver;
    let normalized = receiver.prepare()?.normalized;
    let n = source.wavelengths.len();
    let packet_count = n.div_ceil(4);
    let bound = u64::from(receiver.samples) * packet_count as u64 * 1024 + 1024;
    if packets.len() as u64 > bound {
        return Err(invalid("bounded complete native spectral packets required"));
    }
    let fields = original
        .lines()
        .map(|line| {
            line.split_whitespace()
                .map(scalar)
                .collect::<Result<Vec<_>>>()
        })
        .collect::<Result<Vec<_>>>()?;
    let mut emitters = BTreeMap::new();
    match component {
        AtmosphericComponent::Direct => {
            let values = fields
                .iter()
                .map(|row| row[1] / -prepared.propagation_direction[2])
                .collect::<Vec<_>>();
            if values.iter().any(|v| *v != 0.) {
                emitters.insert(
                    "solar-direct".into(),
                    Emitter {
                        direction: prepared.propagation_direction,
                        irradiance: values,
                        pmf: 1.,
                    },
                );
            }
        }
        AtmosphericComponent::Diffuse => {
            for (i, mu) in prepared.umu.iter().enumerate() {
                for (j, phi) in prepared.phi_deg.iter().enumerate() {
                    let index = 4 + i * prepared.phi_deg.len() + j;
                    let values = fields
                        .iter()
                        .map(|row| row[index] * prepared.angular_cell_solid_angle_sr)
                        .collect::<Vec<_>>();
                    if values.iter().any(|v| *v != 0.) {
                        emitters.insert(
                            format!("angular-{i:03}-{j:03}"),
                            Emitter {
                                direction: propagation(*mu, *phi),
                                irradiance: values,
                                pmf: 0.,
                            },
                        );
                    }
                }
            }
        }
    }
    let maximum = emitters
        .values()
        .flat_map(|e| &e.irradiance)
        .copied()
        .fold(0_f64, f64::max);
    let mut total_weight = 0.;
    let mut weight_correction = 0.;
    for emitter in emitters.values_mut() {
        let (mut weight, mut correction) = (0., 0.);
        for value in &emitter.irradiance {
            add(&mut weight, &mut correction, value / maximum);
        }
        emitter.pmf = weight / n as f64;
        add(&mut total_weight, &mut weight_correction, emitter.pmf);
    }
    for emitter in emitters.values_mut() {
        emitter.pmf /= total_weight;
    }
    let normal = receiver.sensor_normal;
    let up = if normal[1].abs() < 0.9 {
        [0., 1., 0.]
    } else {
        [1., 0., 0.]
    };
    let mut right = cross(up, normal);
    let length = dot(right, right).sqrt();
    right.iter_mut().for_each(|v| *v /= length);
    let vertical = cross(normal, right);
    let [width, height] = normalized.sensor_size_m;
    let geometric_roundoff = 5e-6 * width.max(height);
    let mut rows = packets.lines();
    let header = "sample,knot_offset,x_m,y_m,z_m,towards_source_x,towards_source_y,towards_source_z,native_cosine,native_emitter_id,native_pdf,native_weight_w_m2_nm_0,native_weight_w_m2_nm_1,native_weight_w_m2_nm_2,native_weight_w_m2_nm_3";
    if rows.next() != Some(header) {
        return Err(invalid(
            "exact native atmospheric spectral packet columns required",
        ));
    }
    let mut totals = vec![0.; n];
    let mut corrections = vec![0.; n];
    for sample in 0..receiver.samples {
        let mut sampling_identity = None;
        for offset in (0..n).step_by(4) {
            let row = rows
                .next()
                .ok_or_else(|| invalid("complete native atmospheric spectral packets truncated"))?;
            let row = row.split(',').collect::<Vec<_>>();
            if row.len() != 15
                || row[0].parse::<u32>().ok() != Some(sample)
                || row[1].parse::<usize>().ok() != Some(offset)
            {
                return Err(invalid(
                    "complete ordered native atmospheric sample/knot identities required",
                ));
            }
            let point = [scalar(row[2])?, scalar(row[3])?, scalar(row[4])?];
            let direction = [scalar(row[5])?, scalar(row[6])?, scalar(row[7])?];
            let cosine = scalar(row[8])?;
            let pdf = scalar(row[10])?;
            let weights = [
                scalar(row[11])?,
                scalar(row[12])?,
                scalar(row[13])?,
                scalar(row[14])?,
            ];
            if !(0. ..=1.000005).contains(&cosine)
                || !(0. ..=1.000005).contains(&pdf)
                || weights.iter().any(|v| *v < 0.)
                || dot(point, normal).abs() > geometric_roundoff
                || dot(point, right).abs() > width / 2. + geometric_roundoff
                || dot(point, vertical).abs() > height / 2. + geometric_roundoff
            {
                return Err(invalid(
                    "finite original Float32 packets inside the approved physical rectangle required",
                ));
            }
            let identity = (point, direction, cosine, row[9], pdf);
            if sampling_identity
                .as_ref()
                .is_some_and(|previous| *previous != identity)
            {
                return Err(invalid(
                    "every knot packet must preserve the same original surface/emitter draw",
                ));
            }
            sampling_identity = Some(identity);
            if let Some(emitter) = emitters.get(row[9]) {
                let expected_cosine = (-dot(normal, emitter.direction)).max(0.);
                if direction
                    .into_iter()
                    .zip(emitter.direction)
                    .any(|(a, b)| (a + b).abs() > 5e-6)
                    || (cosine - expected_cosine).abs() > 5e-6
                {
                    return Err(invalid(
                        "native direction/cosine differs from the complete original angular emitter",
                    ));
                }
                if pdf == 0. {
                    if cosine != 0. || weights.iter().any(|v| *v != 0.) {
                        return Err(invalid(
                            "unobstructed front-facing native rays cannot discard radiation",
                        ));
                    }
                } else {
                    if (pdf / emitter.pmf - 1.).abs() > 5e-6 {
                        return Err(invalid(
                            "native emitter PDF differs from unchanged source-derived PMF",
                        ));
                    }
                    for (lane, weight) in weights.iter().enumerate() {
                        let source = emitter.irradiance[(offset + lane).min(n - 1)];
                        if (weight * pdf - source).abs() > 5e-6 * source
                            || (source == 0. && *weight != 0.)
                        {
                            return Err(invalid(
                                "native spectral packet weights changed original knots or padding",
                            ));
                        }
                    }
                }
            } else if !emitters.is_empty()
                || row[9] != "none"
                || direction != [0.; 3]
                || pdf != 0.
                || cosine != 0.
                || weights != [0.; 4]
            {
                return Err(invalid(
                    "only a completely empty original source may have an empty native emitter draw",
                ));
            }
            for (lane, weight) in weights.iter().enumerate().take((n - offset).min(4)) {
                add(
                    &mut totals[offset + lane],
                    &mut corrections[offset + lane],
                    weight * cosine,
                );
            }
        }
    }
    if rows.next().is_some() {
        return Err(invalid(
            "extra native atmospheric spectral packets rejected",
        ));
    }
    totals
        .iter_mut()
        .for_each(|v| *v /= f64::from(receiver.samples));
    let reference = match component {
        AtmosphericComponent::Direct => surface.direct_w_m2_nm,
        AtmosphericComponent::Diffuse => surface.diffuse_w_m2_nm,
    };
    let weights = [
        vec![1.; n],
        receiver.absorptivity.clone(),
        receiver.ageing_action.clone(),
    ];
    let mut channels = [0.; 3];
    for (index, weight) in weights.iter().enumerate() {
        let actual = product_integral(&prepared.wavelengths_nm, &totals, weight);
        let expected = product_integral(&prepared.wavelengths_nm, &reference, weight);
        if !actual.is_finite()
            || actual < 0.
            || (expected == 0. && actual != 0.)
            || (expected > 0. && (actual / expected - 1.).abs() > receiver.relative_tolerance)
        {
            return Err(invalid(
                "reconstructed native atmospheric optical channel exceeds unchanged analytical gate",
            ));
        }
        channels[index] = actual;
    }
    Ok(AtmosphericPacketChannels {
        incident_w_m2: channels[0],
        absorbed_w_m2: channels[1],
        ageing_w_m2: channels[2],
    })
}
