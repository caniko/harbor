use crate::{Error, Result, contracts::*};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs().min(i64::MAX as u64) as i64)
}
pub fn private_dir(root: &Path) -> Result<()> {
    if root.exists() {
        let m = fs::symlink_metadata(root)?;
        use std::os::unix::fs::MetadataExt;
        if !m.is_dir()
            || m.uid() != unsafe { libc::geteuid() }
            || m.permissions().mode() & 0o077 != 0
        {
            return Err(invalid(
                "state directory must be owned, nonsymlink and mode 0700",
            ));
        }
    } else {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(root)?;
    }
    Ok(())
}
pub fn safe_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let p = Path::new(relative);
    if relative.is_empty() || p.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(invalid(
            "only relative non-traversing artifact paths allowed",
        ));
    }
    let mut out = root.to_path_buf();
    for component in p.components() {
        out.push(component);
        match fs::symlink_metadata(&out) {
            Ok(m) if m.file_type().is_symlink() => return Err(invalid("symlink in artifact path")),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(out)
}
pub fn commit_artifact(
    root: &Path,
    name: &str,
    data: &[u8],
    format: &str,
    provenance: &str,
) -> Result<ArtifactManifest> {
    let path = safe_path(root, name)?;
    let parent = path.parent().ok_or_else(|| invalid("artifact parent"))?;
    fs::create_dir_all(parent)?;
    let partial = parent.join(format!(".partial-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&partial)?;
        file.write_all(data)?;
        file.sync_all()?;
        // Same-filesystem, no-clobber commit. The temporary inode is already closed and immutable.
        fs::hard_link(&partial, &path)?;
        fs::File::open(parent)?.sync_all()?;
        Ok(ArtifactManifest {
            schema_version: 1,
            path: name.into(),
            sha256: format!("{:x}", Sha256::digest(data)),
            bytes: data.len() as u64,
            format: format.into(),
            provenance: provenance.into(),
            units: None,
            time_s: None,
            association: None,
        })
    })();
    let _ = fs::remove_file(&partial);
    result
}
fn copy_verified(source: &Path, destination: &Path, artifact: &ArtifactManifest) -> Result<()> {
    let source = safe_path(source, &artifact.path)?;
    let destination = safe_path(destination, &artifact.path)?;
    let mut input = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(source)?;
    let metadata = input.metadata()?;
    if !metadata.is_file() || metadata.len() != artifact.bytes {
        return Err(invalid("artifact type/size mismatch"));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| invalid("artifact parent"))?;
    fs::create_dir_all(parent)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&destination)?;
    let mut hash = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .ok_or_else(|| invalid("artifact size overflow"))?;
        if total > artifact.bytes {
            return Err(invalid("artifact grew during export"));
        }
        hash.update(&buffer[..count]);
        output.write_all(&buffer[..count])?;
    }
    if total != artifact.bytes || format!("{:x}", hash.finalize()) != artifact.sha256 {
        return Err(invalid("artifact checksum/size mismatch"));
    }
    output.sync_all()?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

fn publish_directory(partial: &Path, destination: &Path) -> Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let partial =
        CString::new(partial.as_os_str().as_bytes()).map_err(|_| invalid("export path"))?;
    let destination =
        CString::new(destination.as_os_str().as_bytes()).map_err(|_| invalid("export path"))?;
    // Atomic whole-directory publication, with no replacement even if a competing
    // exporter creates the destination after the initial check.
    let status = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            partial.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if status != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
fn sync_directories(root: &Path) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            sync_directories(&entry.path())?;
        }
    }
    fs::File::open(root)?.sync_all()?;
    Ok(())
}
pub fn ingest_native_tree(
    source: &Path,
    destination: &Path,
    maximum: u64,
) -> Result<Vec<ArtifactManifest>> {
    fn collect(
        root: &Path,
        prefix: &Path,
        paths: &mut Vec<String>,
        total: &mut u64,
        maximum: u64,
    ) -> Result<()> {
        for entry in fs::read_dir(root.join(prefix))? {
            let entry = entry?;
            let relative = prefix.join(entry.file_name());
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.file_type().is_symlink() || (!metadata.is_file() && !metadata.is_dir()) {
                return Err(invalid("native output symlink/special file rejected"));
            }
            if metadata.is_dir() {
                collect(root, &relative, paths, total, maximum)?;
            } else {
                let name = relative
                    .to_str()
                    .ok_or_else(|| invalid("native output path encoding"))?;
                if name.ends_with(".partial")
                    || entry.file_name().to_string_lossy().starts_with('.')
                {
                    return Err(invalid("incomplete native output rejected"));
                }
                if name == "plan.json" {
                    continue;
                }
                *total = total
                    .checked_add(metadata.len())
                    .ok_or_else(|| invalid("native size overflow"))?;
                if *total > maximum || paths.len() >= 8192 {
                    return Err(Error::Resource(
                        "bounded native output ingestion budget exhausted".into(),
                    ));
                }
                paths.push(name.into());
            }
        }
        Ok(())
    }
    let mut paths = Vec::new();
    collect(source, Path::new(""), &mut paths, &mut 0, maximum)?;
    paths.sort();
    let mut artifacts = Vec::new();
    for path in paths {
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(safe_path(source, &path)?)?;
        let bytes = file.metadata()?.len();
        let mut hash = Sha256::new();
        let mut count = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            count = count
                .checked_add(n as u64)
                .ok_or_else(|| invalid("native size overflow"))?;
            if count > bytes || count > maximum {
                return Err(invalid("native output changed during ingestion"));
            }
            hash.update(&buffer[..n]);
        }
        if count != bytes {
            return Err(invalid("native output changed during ingestion"));
        }
        let format = Path::new(&path)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("binary")
            .to_owned();
        let manifest = ArtifactManifest {
            schema_version: 1,
            path,
            sha256: format!("{:x}", hash.finalize()),
            bytes,
            format,
            provenance:
                "closed isolated native output; model/device evidence in companion receipts".into(),
            units: None,
            time_s: None,
            association: None,
        };
        copy_verified(source, destination, &manifest)?;
        artifacts.push(manifest);
    }
    sync_directories(destination)?;
    Ok(artifacts)
}
#[derive(Debug, Serialize)]
pub struct Job {
    pub id: String,
    pub plan_digest: String,
    pub state: String,
    pub unit: String,
    pub invocation_id: Option<String>,
    pub exit_code: Option<i32>,
    pub created_at: i64,
    pub error: Option<String>,
}
pub struct Store {
    pub connection: Connection,
    pub root: PathBuf,
}
impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        private_dir(root)?;
        let db = safe_path(root, "jobs.sqlite3")?;
        let connection = Connection::open(db)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
          CREATE TABLE IF NOT EXISTS jobs(id TEXT PRIMARY KEY, idem TEXT UNIQUE NOT NULL, digest TEXT NOT NULL,
            plan TEXT NOT NULL, state TEXT NOT NULL, unit TEXT NOT NULL, invocation TEXT, exit_code INTEGER,
            created INTEGER NOT NULL, error TEXT);
          CREATE TABLE IF NOT EXISTS events(seq INTEGER PRIMARY KEY AUTOINCREMENT, job TEXT NOT NULL,
            time INTEGER NOT NULL, kind TEXT NOT NULL, message TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS artifacts(job TEXT NOT NULL, path TEXT NOT NULL, manifest TEXT NOT NULL,
            PRIMARY KEY(job,path));")?;
        Ok(Self {
            connection,
            root: root.into(),
        })
    }
    pub fn submit(&self, plan: &ExecutionPlan, key: &str) -> Result<Job> {
        plan.validate()?;
        if !token(key) {
            return Err(invalid("bounded idempotency key"));
        }
        let digest = plan.id()?;
        let tx = self.connection.unchecked_transaction()?;
        if let Some((id, prior)) = tx
            .query_row("SELECT id,digest FROM jobs WHERE idem=?1", [key], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .optional()?
        {
            if prior != digest {
                return Err(Error::IdempotencyConflict);
            }
            tx.commit()?;
            return self.job(&id);
        }
        let id = uuid::Uuid::new_v4().to_string();
        let unit = format!("harbor-cad-job-{id}.service");
        tx.execute("INSERT INTO jobs(id,idem,digest,plan,state,unit,created) VALUES(?1,?2,?3,?4,'queued',?5,?6)",
            params![id,key,digest,serde_json::to_string(plan)?,unit,now()])?;
        tx.execute(
            "INSERT INTO events(job,time,kind,message) VALUES(?1,?2,'submitted',?3)",
            params![id, now(), digest],
        )?;
        tx.commit()?;
        self.job(&id)
    }
    pub fn job(&self, id: &str) -> Result<Job> {
        if uuid::Uuid::parse_str(id).is_err() {
            return Err(invalid("job UUID required"));
        }
        let job: Job = self.connection.query_row(
            "SELECT id,digest,state,unit,invocation,exit_code,created,error FROM jobs WHERE id=?1",
            [id],
            |r| {
                Ok(Job {
                    id: r.get(0)?,
                    plan_digest: r.get(1)?,
                    state: r.get(2)?,
                    unit: r.get(3)?,
                    invocation_id: r.get(4)?,
                    exit_code: r.get(5)?,
                    created_at: r.get(6)?,
                    error: r.get(7)?,
                })
            },
        )?;
        if job.unit != format!("harbor-cad-job-{}.service", job.id) {
            return Err(invalid("persisted unit does not match owned job identity"));
        }
        Ok(job)
    }
    pub fn plan(&self, id: &str) -> Result<ExecutionPlan> {
        let job = self.job(id)?;
        let data: String =
            self.connection
                .query_row("SELECT plan FROM jobs WHERE id=?1", [id], |r| r.get(0))?;
        let plan: ExecutionPlan = serde_json::from_str(&data)?;
        if plan.id()? != job.plan_digest {
            return Err(invalid("persisted immutable plan digest mismatch"));
        }
        plan.validate()?;
        Ok(plan)
    }
    pub fn event(&self, id: &str, kind: &str, message: &str) -> Result<()> {
        if message.len() > 4096 {
            return Err(invalid("event limit"));
        }
        self.connection.execute(
            "INSERT INTO events(job,time,kind,message) VALUES(?1,?2,?3,?4)",
            params![id, now(), kind, message],
        )?;
        Ok(())
    }
    pub fn transition(&self, id: &str, from: &str, to: &str, error: Option<&str>) -> Result<bool> {
        Ok(self.connection.execute(
            "UPDATE jobs SET state=?1,error=?2 WHERE id=?3 AND state=?4",
            params![to, error, id, from],
        )? == 1)
    }
    pub fn finish(&self, id: &str, code: i32, error: Option<&str>) -> Result<()> {
        let tx = self.connection.unchecked_transaction()?;
        tx.execute("UPDATE jobs SET state=CASE WHEN state='cancelling' THEN 'cancelled' WHEN ?1=0 THEN 'succeeded' ELSE 'failed' END,exit_code=?1,error=?2 WHERE id=?3 AND state IN ('running','starting','cancelling')", params![code,error,id])?;
        tx.execute(
            "INSERT INTO events(job,time,kind,message) VALUES(?1,?2,'exit',?3)",
            params![id, now(), code.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn active(&self) -> Result<Vec<String>> {
        let mut stmt = self.connection.prepare("SELECT id FROM jobs WHERE state IN ('queued','starting','running','cancelling') ORDER BY created")?;
        Ok(stmt
            .query_map([], |r| r.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }
    pub fn artifacts(&self, id: &str) -> Result<Vec<ArtifactManifest>> {
        self.job(id)?;
        let mut stmt = self
            .connection
            .prepare("SELECT manifest FROM artifacts WHERE job=?1 ORDER BY path")?;
        let rows = stmt
            .query_map([id], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.iter().map(|s| Ok(serde_json::from_str(s)?)).collect()
    }
    pub fn add_artifact(&self, id: &str, manifest: &ArtifactManifest) -> Result<()> {
        self.connection.execute(
            "INSERT INTO artifacts(job,path,manifest) VALUES(?1,?2,?3)",
            params![id, manifest.path, serde_json::to_string(manifest)?],
        )?;
        Ok(())
    }
    pub fn export(&self, id: &str, destination: &Path) -> Result<usize> {
        if self.job(id)?.state != "succeeded" {
            return Err(invalid("only successful committed jobs may be exported"));
        }
        match fs::symlink_metadata(destination) {
            Ok(_) => return Err(invalid("export destination already exists")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let name = destination
            .file_name()
            .ok_or_else(|| invalid("export destination name"))?;
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let parent = fs::canonicalize(parent)?;
        let destination = parent.join(name);
        let partial = parent.join(format!(".harbor-cad-export-{}", uuid::Uuid::new_v4()));
        private_dir(&partial)?;
        let result = (|| {
            let artifacts = self.artifacts(id)?;
            let source = self.job_dir(id)?;
            for artifact in &artifacts {
                copy_verified(&source, &partial, artifact)?;
            }
            commit_artifact(
                &partial,
                "manifest.json",
                &serde_json::to_vec_pretty(&artifacts)?,
                "json",
                "portable verified export",
            )?;
            sync_directories(&partial)?;
            publish_directory(&partial, &destination)?;
            fs::File::open(&parent)?.sync_all()?;
            Ok(artifacts.len())
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&partial);
        }
        result
    }
    pub fn job_dir(&self, id: &str) -> Result<PathBuf> {
        self.job(id)?;
        let dir = safe_path(&self.root, &format!("artifacts/{id}"))?;
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }
    pub fn logs(&self, id: &str, after: u64, limit: u32) -> Result<serde_json::Value> {
        self.job(id)?;
        if limit == 0 || limit > 100 {
            return Err(invalid("log limit 1..100"));
        }
        let mut stmt = self.connection.prepare("SELECT seq,time,kind,message FROM events WHERE job=?1 AND seq>?2 ORDER BY seq LIMIT ?3")?;
        let after = i64::try_from(after).map_err(|_| invalid("log cursor outside SQLite range"))?;
        let rows = stmt.query_map(params![id,after,limit], |r| Ok(serde_json::json!({"seq":r.get::<_,i64>(0)?,"time":r.get::<_,i64>(1)?,"kind":r.get::<_,String>(2)?,"message":r.get::<_,String>(3)?})))?;
        Ok(serde_json::Value::Array(
            rows.collect::<std::result::Result<Vec<_>, _>>()?,
        ))
    }
}
