use harbor_cad::resources::try_reserve_cards;

#[test]
fn physical_card_reservations_cross_job_roots_and_share_roles() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("cards");
    let compute = try_reserve_cards(&root, &["0000:0a:00.0".into()])
        .unwrap()
        .unwrap();
    assert!(
        try_reserve_cards(&root, &["0000:0A:00.0".into()])
            .unwrap()
            .is_none()
    );
    assert!(
        try_reserve_cards(&root, &["0000:0a:00.0".into(), "0000:0b:00.0".into()])
            .unwrap()
            .is_none()
    );
    let separate = try_reserve_cards(&root, &["0000:0b:00.0".into()])
        .unwrap()
        .unwrap();
    drop(compute);
    assert!(
        try_reserve_cards(&root, &["0000:0a:00.0".into(), "0000:0b:00.0".into()])
            .unwrap()
            .is_none()
    );
    let render_and_media =
        try_reserve_cards(&root, &["0000:0a:00.0".into(), "0000:0A:00.0".into()])
            .unwrap()
            .unwrap();
    assert_eq!(render_and_media.len(), 1);
    drop(separate);
    drop(render_and_media);
    assert!(
        try_reserve_cards(&root, &["0000:0b:00.0".into(), "0000:0a:00.0".into()])
            .unwrap()
            .is_some()
    );
    assert!(try_reserve_cards(&root, &["../../escape".into()]).is_err());
    std::os::unix::fs::symlink("/etc/passwd", root.join("0000:0c:00.0")).unwrap();
    assert!(try_reserve_cards(&root, &["0000:0c:00.0".into()]).is_err());
}
