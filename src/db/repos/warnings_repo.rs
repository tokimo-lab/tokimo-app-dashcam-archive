use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder};
use uuid::Uuid;

use crate::db::entities::warnings;

pub struct WarningsRepo;

impl WarningsRepo {
    pub async fn add<C: ConnectionTrait>(
        db: &C,
        group_id: Uuid,
        warning_key: String,
        count: i32,
        first_example: Option<String>,
    ) -> anyhow::Result<warnings::Model> {
        let active = warnings::ActiveModel {
            id: Set(Uuid::new_v4()),
            group_id: Set(group_id),
            warning_key: Set(warning_key),
            count: Set(count),
            first_example: Set(first_example),
        };
        Ok(warnings::Entity::insert(active).exec_with_returning(db).await?)
    }

    pub async fn list_for_groups<C: ConnectionTrait>(
        db: &C,
        group_ids: Vec<Uuid>,
    ) -> anyhow::Result<Vec<warnings::Model>> {
        if group_ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(warnings::Entity::find()
            .filter(warnings::Column::GroupId.is_in(group_ids))
            .order_by_asc(warnings::Column::WarningKey)
            .all(db)
            .await?)
    }
}
