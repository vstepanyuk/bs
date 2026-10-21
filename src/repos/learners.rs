//! Learner profile persistence.

use chrono::Utc;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DatabaseTransaction, EntityTrait,
    QueryFilter, QuerySelect, sea_query::Expr,
};
use uuid::Uuid;

use crate::{
    dto::learners::{LearnerProfile, PatchLearner},
    entities::learner,
    error::AppError,
};

pub async fn learner_exists(db: &DatabaseConnection, learner_id: Uuid) -> Result<bool, AppError> {
    Ok(learner::Entity::find_by_id(learner_id)
        .select_only()
        .column(learner::Column::Id)
        .into_tuple::<Uuid>()
        .one(db)
        .await?
        .is_some())
}

pub async fn get_learner<C: ConnectionTrait>(
    db: &C,
    learner_id: Uuid,
) -> Result<LearnerProfile, AppError> {
    learner::Entity::find_by_id(learner_id)
        .select_only()
        .columns([
            learner::Column::Id,
            learner::Column::ExternalId,
            learner::Column::DisplayName,
            learner::Column::CefrLevel,
            learner::Column::TotalXp,
            learner::Column::CreatedAt,
        ])
        .into_model::<LearnerProfile>()
        .one(db)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn patch_learner(
    db: &DatabaseConnection,
    learner_id: Uuid,
    patch: &PatchLearner,
) -> Result<LearnerProfile, AppError> {
    db.transaction_async(async move |transaction| {
        patch_learner_in_transaction(transaction, learner_id, patch).await
    })
    .await
    .map_err(AppError::from)
}

async fn patch_learner_in_transaction(
    transaction: &DatabaseTransaction,
    learner_id: Uuid,
    patch: &PatchLearner,
) -> Result<LearnerProfile, AppError> {
    // The update takes the row lock before selecting the response, and touches
    // only supplied fields so concurrent changes to other columns survive.
    let mut update = learner::Entity::update_many()
        .col_expr(learner::Column::UpdatedAt, Expr::val(Utc::now()))
        .filter(learner::Column::Id.eq(learner_id));
    if let Some(display_name) = &patch.display_name {
        update = update.col_expr(
            learner::Column::DisplayName,
            Expr::val(display_name.as_ref()),
        );
    }
    if let Some(cefr_level) = &patch.cefr_level {
        let level: &'static str = (*cefr_level).into();
        update = update.col_expr(learner::Column::CefrLevel, Expr::val(level));
    }
    if update.exec(transaction).await?.rows_affected == 0 {
        return Err(AppError::NotFound);
    }
    get_learner(transaction, learner_id).await
}
