use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "lesson_progress")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub learner_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub lesson_id: String,
    pub completed_exercises: i32,
    pub xp: i64,
    pub updated_at: DateTimeUtc,
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
