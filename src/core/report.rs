use uuid::Uuid;

pub struct RunReport<'a> {
    pub run_id: Uuid,
    pub status: &'a str,
    pub files: usize,
    pub groups: usize,
    pub warnings: usize,
}
