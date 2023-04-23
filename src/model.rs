use chrono::prelude::*;
use mongodb::bson::{self, oid::ObjectId};
use serde::{Deserialize, Serialize};

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NoteModel {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub title: String,
    pub content: String,
    pub category: Option<String>,
    pub published: Option<bool>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub createdAt: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub updatedAt: DateTime<Utc>,
}
