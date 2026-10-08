use harbor_cad::storage::snapshot_cad_input;
use sha2::{Digest, Sha256};

#[test]
fn cad_snapshot_binds_approved_closed_bytes_without_a_mutable_source_alias() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("design.FCStd");
    let job = temp.path().join("job");
    std::fs::create_dir(&job).unwrap();
    std::fs::write(&source, b"approved CAD bytes").unwrap();
    let expected = format!("{:x}", Sha256::digest(b"approved CAD bytes"));
    let artifact = snapshot_cad_input(&source, &job, &expected, 1024).unwrap();
    std::fs::write(&source, b"changed after admission").unwrap();
    assert_eq!(
        std::fs::read(job.join(&artifact.path)).unwrap(),
        b"approved CAD bytes"
    );
    assert_eq!(artifact.sha256, expected);
    assert_eq!(artifact.bytes, 18);
    assert!(artifact.provenance.contains("read-only"));
}

#[test]
fn bad_digest_oversize_and_symlink_cad_never_publish_an_input_snapshot() {
    for failure in ["digest", "budget", "symlink"] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("design.FCStd");
        let job = temp.path().join("job");
        std::fs::create_dir(&job).unwrap();
        std::fs::write(&source, b"CAD bytes").unwrap();
        let mut expected = format!("{:x}", Sha256::digest(b"CAD bytes"));
        let mut maximum = 1024;
        match failure {
            "digest" => expected = "0".repeat(64),
            "budget" => maximum = 2,
            "symlink" => {
                let link = temp.path().join("link.FCStd");
                std::os::unix::fs::symlink(&source, &link).unwrap();
                assert!(snapshot_cad_input(&link, &job, &expected, maximum).is_err());
                continue;
            }
            _ => unreachable!(),
        }
        assert!(snapshot_cad_input(&source, &job, &expected, maximum).is_err());
        assert_eq!(std::fs::read_dir(job).unwrap().count(), 0);
        assert_eq!(std::fs::read(source).unwrap(), b"CAD bytes");
    }
}
