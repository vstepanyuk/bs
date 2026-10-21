use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "learners")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub external_id: String,
    pub display_name: String,
    pub cefr_level: String,
    pub email: String,
    pub auth_subject: String,
    pub total_xp: i64,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::lesson_progress::Entity")]
    LessonProgress,
    #[sea_orm(has_many = "super::progress_event::Entity")]
    ProgressEvent,
}

impl Related<super::lesson_progress::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::LessonProgress.def()
    }
}

impl Related<super::progress_event::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ProgressEvent.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
