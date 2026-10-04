use crate::{Error, Result, contracts::*};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
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
        Ok(self.connection.query_row(
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
        )?)
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
            .prepare("SELECT manifest FROM artifacts WHERE job=?1 ORDER BY path LIMIT 256")?;
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
    pub fn job_dir(&self, id: &str) -> Result<PathBuf> {
        self.job(id)?;
        let dir = self.root.join("artifacts").join(id);
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
