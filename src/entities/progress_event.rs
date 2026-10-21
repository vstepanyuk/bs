use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "progress_events")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub learner_id: Uuid,
    pub lesson_id: String,
    pub exercise_id: String,
    pub event_type: String,
    pub xp_awarded: i32,
    pub idempotency_key: String,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::learner::Entity",
        from = "Column::LearnerId",
        to = "super::learner::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Learner,
}

impl Related<super::learner::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Learner.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
