use crate::error::MyError;
use crate::response::{NoteData, NoteListResponse, NoteResponse, SingleNoteResponse};
use crate::{
    error::MyError::*, model::NoteModel, schema::CreateNoteSchema, schema::UpdateNoteSchema,
};
use chrono::prelude::*;
use futures::StreamExt;
use mongodb::bson::{doc, oid::ObjectId, Document};
use mongodb::options::{FindOneAndUpdateOptions, FindOptions, IndexOptions, ReturnDocument};
use mongodb::{bson, options::ClientOptions, Client, Collection, IndexModel};
use std::str::FromStr;

#[derive(Clone, Debug)]
pub struct DB {
    pub note_collection: Collection<NoteModel>,
    pub collection: Collection<Document>,
}

type Result<T> = std::result::Result<T, MyError>;

impl DB {
    pub async fn init() -> Result<Self> {
        let mongodb_uri = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set.");
        let database_name =
            std::env::var("MONGO_INITDB_DATABASE").expect("MONGO_INITDB_DATABASE must be set.");
        let collection_name =
            std::env::var("MONGODB_NOTE_COLLECTION").expect("MONGODB_NOTE_COLLECTION must be set.");

        let mut client_options = ClientOptions::parse(mongodb_uri).await?;
        client_options.app_name = Some(database_name.to_string());

        let client = Client::with_options(client_options)?;
        let database = client.database(database_name.as_str());

        let note_collection = database.collection(collection_name.as_str());
        let collection = database.collection::<Document>(collection_name.as_str());

        println!("✅ Database connected successfully");

        Ok(Self {
            note_collection,
            collection,
        })
    }

    pub async fn fetch_notes(&self, limit: i64, page: i64) -> Result<NoteListResponse> {
        let find_options = FindOptions::builder()
            .limit(limit)
            .skip(u64::try_from((page - 1) * limit).unwrap())
            .build();

        let mut cursor = self
            .note_collection
            .find(None, find_options)
            .await
            .map_err(MongoQueryError)?;

        let mut json_result: Vec<NoteResponse> = Vec::new();
        while let Some(doc) = cursor.next().await {
            json_result.push(self.doc_to_note(&doc.unwrap())?);
        }

        Ok(NoteListResponse {
            status: "success",
            results: json_result.len(),
            notes: json_result,
        })
    }

    pub async fn create_note(&self, body: &CreateNoteSchema) -> Result<SingleNoteResponse> {
        let published = body.published.to_owned().unwrap_or(false);
        let category = body.category.to_owned().unwrap_or_default();

        let document = self.create_note_document(body, published, category)?;

        let options = IndexOptions::builder().unique(true).build();
        let index = IndexModel::builder()
            .keys(doc! {"title": 1})
            .options(options)
            .build();

        match self.note_collection.create_index(index, None).await {
            Ok(_) => {}
            Err(e) => return Err(MongoQueryError(e)),
        };

        let insert_result = match self.collection.insert_one(&document, None).await {
            Ok(result) => result,
            Err(e) => {
                if e.to_string()
                    .contains("E11000 duplicate key error collection")
                {
                    return Err(MongoDuplicateError(e));
                }
                return Err(MongoQueryError(e));
            }
        };

        let new_id = insert_result
            .inserted_id
            .as_object_id()
            .expect("issue with new _id");

        let note_doc = match self
            .note_collection
            .find_one(doc! {"_id": new_id}, None)
            .await
        {
            Ok(Some(doc)) => doc,
            Ok(None) => return Err(NotFoundError(new_id.to_string())),
            Err(e) => return Err(MongoQueryError(e)),
        };

        Ok(SingleNoteResponse {
            status: "success",
            data: NoteData {
                note: self.doc_to_note(&note_doc)?,
            },
        })
    }

    pub async fn get_note(&self, id: &str) -> Result<SingleNoteResponse> {
        let oid = ObjectId::from_str(id).map_err(|_| InvalidIDError(id.to_owned()))?;

        let note_doc = self
            .note_collection
            .find_one(doc! {"_id":oid }, None)
            .await
            .map_err(MongoQueryError)?;

        match note_doc {
            Some(doc) => {
                let note = self.doc_to_note(&doc)?;
                Ok(SingleNoteResponse {
                    status: "success",
                    data: NoteData { note },
                })
            }
            None => Err(NotFoundError(id.to_string())),
        }
    }

    pub async fn edit_note(&self, id: &str, body: &UpdateNoteSchema) -> Result<SingleNoteResponse> {
        let oid = ObjectId::from_str(id).map_err(|_| InvalidIDError(id.to_owned()))?;

        let update = doc! {
            "$set": bson::to_document(body).map_err(MongoSerializeBsonError)?,
        };

        let options = FindOneAndUpdateOptions::builder()
            .return_document(ReturnDocument::After)
            .build();

        if let Some(doc) = self
            .note_collection
            .find_one_and_update(doc! {"_id": oid}, update, options)
            .await
            .map_err(MongoQueryError)?
        {
            let note = self.doc_to_note(&doc)?;
            let note_response = SingleNoteResponse {
                status: "success",
                data: NoteData { note },
            };
            Ok(note_response)
        } else {
            Err(NotFoundError(id.to_string()))
        }
    }

    pub async fn delete_note(&self, id: &str) -> Result<()> {
        let oid = ObjectId::from_str(id).map_err(|_| InvalidIDError(id.to_owned()))?;
        let filter = doc! {"_id": oid };

        let result = self
            .collection
            .delete_one(filter, None)
            .await
            .map_err(MongoQueryError)?;

        match result.deleted_count {
            0 => Err(NotFoundError(id.to_string())),
            _ => Ok(()),
        }
    }

    fn doc_to_note(&self, note: &NoteModel) -> Result<NoteResponse> {
        let note_response = NoteResponse {
            id: note.id.to_hex(),
            title: note.title.to_owned(),
            content: note.content.to_owned(),
            category: note.category.to_owned().unwrap(),
            published: note.published.unwrap(),
            createdAt: note.createdAt,
            updatedAt: note.updatedAt,
        };

        Ok(note_response)
    }

    fn create_note_document(
        &self,
        body: &CreateNoteSchema,
        published: bool,
        category: String,
    ) -> Result<bson::Document> {
        let serialized_data = bson::to_bson(body).map_err(MongoSerializeBsonError)?;
        let document = serialized_data.as_document().unwrap();

        let datetime = Utc::now();

        let mut doc_with_dates = doc! {
            "createdAt": datetime,
            "updatedAt": datetime,
            "published": published,
            "category": category
        };
        doc_with_dates.extend(document.clone());

        Ok(doc_with_dates)
    }
}
