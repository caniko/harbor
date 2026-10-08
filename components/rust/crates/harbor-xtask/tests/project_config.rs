use std::path::PathBuf;

use harbor_xtask::{DocsSite, NixPackage, ProjectConfig};

#[test]
fn resolves_relative_paths_and_named_entries() {
    let mut cfg = ProjectConfig::from_workspace_root("/workspace");
    cfg.docs.push(DocsSite::zola("docs", "docs/site"));
    cfg.nix_packages.push(NixPackage {
        name: String::from("site"),
        flake_ref: String::from(".#site"),
    });

    assert_eq!(
        cfg.resolve("docs/site"),
        PathBuf::from("/workspace/docs/site")
    );
    assert_eq!(
        cfg.docs_site("docs").expect("docs site").root,
        PathBuf::from("docs/site")
    );
    assert_eq!(
        cfg.nix_package("site").expect("nix package").flake_ref,
        ".#site"
    );
}
