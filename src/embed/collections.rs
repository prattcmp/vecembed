use std::fmt;

use sea_orm::{ColumnTrait, EntityTrait};

use crate::entities::{contents, uploaded_files};
use crate::grpc::server::vecembed_rpc::EmbeddableModel;

pub const COLLECTION_NAME: &str = "silatus_documents";

pub trait EmbeddableMarker {}

#[allow(dead_code)]
pub trait EmbeddableEntity<E>
where
    E: EntityTrait + Send + Sync,
{
}

#[allow(dead_code)]
pub trait EmbeddableEntityColumn<E, C>
where
    E: EntityTrait + Send + Sync,
    C: ColumnTrait,
{
    type OrderByColumnType: sea_orm::query::IntoSimpleExpr;

    fn order_by_column() -> Self::OrderByColumnType;
    fn primary_key_column() -> C;
    fn user_id_column() -> Option<C>;
    fn text_column() -> C;
    fn updated_at_column() -> C;
    fn qdrant_sync_column() -> C;
}

macro_rules! embeddable_entity {
    ($entity:ty, $column:ty, $primary_key:expr, $user_id:expr, $order_by:expr, $text_column:expr, $updated_at_column:expr, $qdrant_sync_column:expr) => {
        impl EmbeddableMarker for $entity {}

        impl EmbeddableEntity<$entity> for $entity where
            $entity: sea_orm::EntityTrait + Send + Sync + EmbeddableMarker
        {
        }

        impl EmbeddableEntityColumn<$entity, $column> for $entity
        where
            $entity: sea_orm::EntityTrait + Send + Sync + EmbeddableMarker,
            $column: sea_orm::ColumnTrait,
        {
            type OrderByColumnType = <$entity as sea_orm::EntityTrait>::Column;

            fn order_by_column() -> Self::OrderByColumnType {
                $order_by
            }

            fn primary_key_column() -> <$entity as sea_orm::EntityTrait>::Column {
                $primary_key
            }

            fn user_id_column() -> Option<<$entity as sea_orm::EntityTrait>::Column> {
                $user_id
            }

            fn text_column() -> <$entity as sea_orm::EntityTrait>::Column {
                $text_column
            }

            fn updated_at_column() -> <$entity as sea_orm::EntityTrait>::Column {
                $updated_at_column
            }

            fn qdrant_sync_column() -> <$entity as sea_orm::EntityTrait>::Column {
                $qdrant_sync_column
            }
        }
    };
}

embeddable_entity!(
    contents::Entity,
    contents::Column,
    contents::Column::Id,
    None,
    contents::Column::Id,
    contents::Column::Body,
    contents::Column::UpdatedAt,
    contents::Column::QdrantSyncAt
);

embeddable_entity!(
    uploaded_files::Entity,
    uploaded_files::Column,
    uploaded_files::Column::Id,
    Some(uploaded_files::Column::UserId),
    uploaded_files::Column::Id,
    uploaded_files::Column::Text,
    uploaded_files::Column::UpdatedAt,
    uploaded_files::Column::QdrantSyncAt
);

impl fmt::Display for EmbeddableModel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
