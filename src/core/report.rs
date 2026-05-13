use std::path::Path;

use uuid::Uuid;

pub struct RunReport<'a> {
    pub run_id: Uuid,
    pub status: &'a str,
    pub files: usize,
    pub groups: usize,
    pub warnings: usize,
}

pub async fn write_report(path: &Path, report: &RunReport<'_>) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let body = format!(
        "# 转码警告汇总\n\nrun_id: {}\nstatus: {}\nfiles: {}\ngroups: {}\nwarnings: {}\n",
        report.run_id, report.status, report.files, report.groups, report.warnings
    );
    tokio::fs::write(path, body).await?;
    Ok(())
}
