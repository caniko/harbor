use harbor_cad::devices::HipIdentity;
use std::fs;

fn topology(root: &std::path::Path) {
    let nodes = root.join("nodes");
    fs::create_dir_all(nodes.join("0")).unwrap();
    fs::create_dir_all(nodes.join("1")).unwrap();
    fs::write(root.join("generation_id"), "1\n").unwrap();
    fs::write(nodes.join("0/gpu_id"), "0\n").unwrap();
    fs::write(
        nodes.join("0/properties"),
        "cpu_cores_count 32\nsimd_count 0\n",
    )
    .unwrap();
    fs::write(nodes.join("1/gpu_id"), "4672\n").unwrap();
    fs::write(
        nodes.join("1/properties"),
        concat!(
            "cpu_cores_count 0\nsimd_count 192\n",
            "gfx_target_version 110000\nvendor_id 4098\n",
            "location_id 768\ndomain 0\ndrm_render_minor 128\n",
            "unique_id 9004149066219862957\n",
        ),
    )
    .unwrap();
}

#[test]
fn kfd_pci_minor_uuid_and_architecture_are_one_exact_identity() {
    let root = tempfile::tempdir().unwrap();
    topology(root.path());
    let identity =
        HipIdentity::from_topology(root.path(), "0000:03:00.0", 128, "7cf529dea4b33bad").unwrap();
    assert_eq!(
        identity.backend_uuid,
        "GPU-37636635323964656134623333626164"
    );
    assert_eq!(identity.architecture, "gfx1100");
    assert_eq!(identity.kfd_gpu_id, 4672);
    assert!(
        HipIdentity::from_topology(root.path(), "0000:04:00.0", 128, "7cf529dea4b33bad").is_err()
    );
    assert!(
        HipIdentity::from_topology(root.path(), "0000:03:00.0", 129, "7cf529dea4b33bad").is_err()
    );
    assert!(
        HipIdentity::from_topology(root.path(), "0000:03:00.0", 128, "0000000000000000").is_err()
    );
    assert!(
        HipIdentity::from_topology(root.path(), "0000:03:00.0", 128, "7cf529dea4b33bae").is_err()
    );
}

#[test]
fn second_kfd_gpu_is_rejected_even_when_selected_metadata_matches() {
    let root = tempfile::tempdir().unwrap();
    topology(root.path());
    fs::create_dir(root.path().join("nodes/2")).unwrap();
    fs::write(root.path().join("nodes/2/gpu_id"), "4800\n").unwrap();
    fs::write(root.path().join("nodes/2/properties"), "simd_count 8\n").unwrap();
    assert!(
        HipIdentity::from_topology(root.path(), "0000:03:00.0", 128, "7cf529dea4b33bad").is_err()
    );
}

#[test]
fn malformed_properties_and_symlinks_cannot_supply_device_identity() {
    let root = tempfile::tempdir().unwrap();
    topology(root.path());
    let path = root.path().join("nodes/1/properties");
    let original = fs::read_to_string(&path).unwrap();
    for suffix in ["domain 0\n", "domain nope\n", "unknown 1 trailing\n"] {
        fs::write(&path, format!("{original}{suffix}")).unwrap();
        assert!(
            HipIdentity::from_topology(root.path(), "0000:03:00.0", 128, "7cf529dea4b33bad")
                .is_err()
        );
    }
    fs::write(&path, &original).unwrap();
    fs::rename(&path, root.path().join("foreign")).unwrap();
    std::os::unix::fs::symlink(root.path().join("foreign"), &path).unwrap();
    assert!(
        HipIdentity::from_topology(root.path(), "0000:03:00.0", 128, "7cf529dea4b33bad").is_err()
    );
}
