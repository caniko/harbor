use std::process::Command;

use anyhow::Result;

use crate::{DocsEngine, ProjectConfig, ensure_tool, run_command};

pub fn run_docs_serve(cfg: &ProjectConfig, site: &str) -> Result<()> {
    let site = cfg.docs_site(site)?;
    let tool = match site.engine {
        DocsEngine::Zola => ensure_tool("zola")?,
        DocsEngine::Mdbook => ensure_tool("mdbook")?,
    };

    let mut command = Command::new(tool);
    command.current_dir(cfg.resolve(&site.root)).arg("serve");
    run_command(&mut command)
}
