//! Exact binary STL triangles for the supported closed planar-box CAD scope.
//! Original Float32 vertices are converted once to Float64 metres, never welded,
//! translated again, healed or clipped. This is geometry verification, not transport.
use crate::{Result, cad_source::CadMeshDescriptor, contracts::invalid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_TRIANGLES: usize = 100_000;

#[derive(Clone, Debug)]
pub struct Triangle {
    pub vertices_m: [[f64; 3]; 3],
    pub normal: [f64; 3],
    pub area_m2: f64,
    /// x-/x+/y-/y+/z-/z+; this is a geometric predicate, not a native face ID.
    pub boundary: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TriangleAssessment {
    pub triangles: usize,
    pub face_triangles: [usize; 6],
    pub face_area_m2: [f64; 6],
    pub area_m2: f64,
    pub volume_m3: f64,
    pub maximum_bounds_error_m: f64,
    pub maximum_face_area_relative_error: f64,
    pub volume_relative_error: f64,
    pub coordinate_unit: String,
    pub original_coordinate_unit: String,
    pub coordinate_precision: String,
    pub topology: String,
}

#[derive(Clone, Debug)]
pub struct BoxTriangles {
    /// Exact original facet order; material assignments attach to whole regions.
    pub triangles: Vec<Triangle>,
    pub assessment: TriangleAssessment,
}

fn subtract(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
fn sum(values: impl IntoIterator<Item = f64>) -> f64 {
    let (mut total, mut correction) = (0., 0.);
    for value in values {
        let next = total + value;
        correction += if total.abs() >= value.abs() {
            (total - next) + value
        } else {
            (value - next) + total
        };
        total = next;
    }
    total + correction
}

pub fn verify_binary_box(data: &[u8], geometry: &CadMeshDescriptor) -> Result<BoxTriangles> {
    geometry.validate()?;
    if data.len() < 84 {
        return Err(invalid("complete original binary STL required"));
    }
    let count = u32::from_le_bytes(
        data[80..84]
            .try_into()
            .map_err(|_| invalid("binary STL count"))?,
    ) as usize;
    if !(12..=MAX_TRIANGLES).contains(&count) || data.len() != 84 + 50 * count {
        return Err(invalid(
            "bounded exact binary STL facet count, no truncated or trailing records required",
        ));
    }
    let mut triangles = Vec::with_capacity(count);
    let mut unique = BTreeSet::new();
    let mut edges = BTreeMap::<([u32; 3], [u32; 3]), (usize, i32)>::new();
    let mut bounds = [
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    let mut volumes = Vec::with_capacity(count);
    let origin = [
        geometry.bounds_m[0],
        geometry.bounds_m[2],
        geometry.bounds_m[4],
    ];
    for record in data[84..].as_chunks::<50>().0 {
        if record[48..] != [0, 0] {
            return Err(invalid(
                "STL colour/attribute material encoding is ambiguous; explicit region materials required",
            ));
        }
        let numbers: Vec<f32> = record[..48]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|s| f32::from_le_bytes([s[0], s[1], s[2], s[3]]))
            .collect();
        if numbers.iter().any(|n| !n.is_finite()) {
            return Err(invalid("finite original STL normals and vertices required"));
        }
        let normal = std::array::from_fn(|i| f64::from(numbers[i]));
        let vertices_m: [[f64; 3]; 3] = std::array::from_fn(|v| {
            std::array::from_fn(|i| f64::from(numbers[3 + v * 3 + i]) * geometry.scale_to_m)
        });
        let keys: [[u32; 3]; 3] = std::array::from_fn(|v| {
            std::array::from_fn(|i| {
                let n = numbers[3 + v * 3 + i];
                if n == 0. { 0 } else { n.to_bits() }
            })
        });
        let mut sorted = keys;
        sorted.sort();
        if !unique.insert(sorted) {
            return Err(invalid("duplicate original STL facets rejected"));
        }
        for vertex in vertices_m {
            for i in 0..3 {
                bounds[2 * i] = bounds[2 * i].min(vertex[i]);
                bounds[2 * i + 1] = bounds[2 * i + 1].max(vertex[i]);
            }
        }
        let geometric = cross(
            subtract(vertices_m[1], vertices_m[0]),
            subtract(vertices_m[2], vertices_m[0]),
        );
        let norm = dot(geometric, geometric).sqrt();
        if !norm.is_finite()
            || norm <= 0.
            || (dot(normal, normal) - 1.).abs() > 1e-6
            || (0..3).any(|i| (geometric[i] / norm - normal[i]).abs() > 1e-6)
        {
            return Err(invalid(
                "nondegenerate outward facet winding and consistent original STL normal required",
            ));
        }
        let faces: Vec<usize> = (0..6)
            .filter(|face| {
                vertices_m.iter().all(|v| {
                    (v[face / 2] - geometry.bounds_m[*face]).abs() <= geometry.geometry_tolerance_m
                })
            })
            .collect();
        if faces.len() != 1 {
            return Err(invalid(
                "every supported box triangle must lie on one unambiguous geometric boundary plane",
            ));
        }
        let boundary = faces[0];
        let expected: f64 = if boundary.is_multiple_of(2) { -1. } else { 1. };
        if (normal[boundary / 2] - expected).abs() > 1e-6
            || (0..3).any(|i| i != boundary / 2 && normal[i].abs() > 1e-6)
        {
            return Err(invalid(
                "original STL must preserve outward named-box surface orientation",
            ));
        }
        for (a, b) in [(0, 1), (1, 2), (2, 0)] {
            let (key, sign) = if keys[a] < keys[b] {
                ((keys[a], keys[b]), 1)
            } else {
                ((keys[b], keys[a]), -1)
            };
            let entry = edges.entry(key).or_default();
            entry.0 += 1;
            entry.1 += sign;
            if entry.0 > 2 {
                return Err(invalid("nonmanifold STL edge rejected without welding"));
            }
        }
        volumes.push(
            dot(
                subtract(vertices_m[0], origin),
                cross(
                    subtract(vertices_m[1], origin),
                    subtract(vertices_m[2], origin),
                ),
            ) / 6.,
        );
        triangles.push(Triangle {
            vertices_m,
            normal,
            area_m2: norm / 2.,
            boundary,
        });
    }
    if edges
        .values()
        .any(|(count, orientation)| *count != 2 || *orientation != 0)
    {
        return Err(invalid(
            "complete closed oriented binary STL topology required; no gap healing",
        ));
    }
    let maximum_bounds_error_m = bounds
        .into_iter()
        .zip(geometry.bounds_m)
        .map(|(a, b)| (a - b).abs())
        .fold(0., f64::max);
    let face_triangles =
        std::array::from_fn(|face| triangles.iter().filter(|t| t.boundary == face).count());
    let face_area_m2 = std::array::from_fn(|face| {
        sum(triangles
            .iter()
            .filter(|t| t.boundary == face)
            .map(|t| t.area_m2))
    });
    let dimensions = geometry.lengths_m();
    let maximum_face_area_relative_error = (0..6)
        .map(|face| {
            let target: f64 = (0..3)
                .filter(|i| *i != face / 2)
                .map(|i| dimensions[i])
                .product();
            (face_area_m2[face] / target - 1.).abs()
        })
        .fold(0., f64::max);
    let volume_m3 = sum(volumes);
    let volume_relative_error = (volume_m3 / geometry.volume_m3 - 1.).abs();
    if maximum_bounds_error_m > geometry.geometry_tolerance_m
        || face_triangles.contains(&0)
        || maximum_face_area_relative_error > 1e-10
        || !volume_m3.is_finite()
        || volume_m3 <= 0.
        || volume_relative_error > geometry.volume_relative_tolerance
    {
        return Err(invalid(
            "complete native box bounds, six surface areas and unchanged 1e-10 volume/area correspondence required; original STL precision is not repaired",
        ));
    }
    let area_m2 = sum(face_area_m2);
    let assessment = TriangleAssessment {
        triangles: count,
        face_triangles,
        face_area_m2,
        area_m2,
        volume_m3,
        maximum_bounds_error_m,
        maximum_face_area_relative_error,
        volume_relative_error,
        coordinate_unit: "m".into(),
        original_coordinate_unit: "mm".into(),
        coordinate_precision: "original_binary_STL_Float32_to_SI_Float64".into(),
        topology: "closed_oriented_manifold_without_welding_or_healing".into(),
    };
    Ok(BoxTriangles {
        triangles,
        assessment,
    })
}
