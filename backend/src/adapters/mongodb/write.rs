use super::*;
use mongodb::options::Collation;

const MAX_DOCUMENT_BYTES: usize = 1024 * 1024;

// Retain the original BSON field order and exact types independently of the JSON view.
pub(super) fn snapshot(document: &Document) -> AppResult<Map<String, Value>> {
    let bytes = mongodb::bson::to_vec(document).map_err(|_| invalid("Cannot encode document."))?;
    let mut original = Map::new();
    if bytes.len() <= MAX_DOCUMENT_BYTES && document.contains_key("_id") {
        let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        original.insert("snapshot".into(), Value::String(hex));
    }
    Ok(original)
}

fn original(snapshot: &str) -> AppResult<Document> {
    if snapshot.is_empty()
        || snapshot.len() > MAX_DOCUMENT_BYTES * 2
        || !snapshot.len().is_multiple_of(2)
    {
        return Err(invalid("Reload the complete document before editing."));
    }
    let bytes: Result<Vec<u8>, _> = snapshot
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            std::str::from_utf8(pair)
                .ok()
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                .ok_or_else(|| invalid("Invalid document snapshot."))
        })
        .collect();
    let document: Document =
        mongodb::bson::from_slice(&bytes?).map_err(|_| invalid("Invalid document snapshot."))?;
    if !document.contains_key("_id") {
        return Err(invalid("Document edits require an _id."));
    }
    Ok(document)
}

fn replacement(value: &Value) -> AppResult<Document> {
    if value.to_string().len() > MAX_DOCUMENT_BYTES {
        return Err(invalid(
            "Document editing is limited to 1 MiB of Extended JSON.",
        ));
    }
    let Bson::Document(document) =
        Bson::try_from(value.clone()).map_err(|_| invalid("Invalid MongoDB Extended JSON."))?
    else {
        return Err(invalid("Enter a complete document as a JSON object."));
    };
    if document
        .keys()
        .any(|key| key.starts_with('$') || key.contains('\0'))
    {
        return Err(invalid(
            "Document field names cannot start with $ or contain a null byte.",
        ));
    }
    Ok(document)
}

fn unchanged(document: Document) -> Document {
    // Literal values cannot become query operators, including operator-shaped _ids.
    doc! { "_id": { "$eq": document.get("_id").unwrap().clone() }, "$expr": { "$and": [
        { "$eq": ["$_id", { "$literal": document.get("_id").unwrap().clone() }] },
        { "$eq": ["$$ROOT", { "$literal": document }] }
    ] } }
}

pub(super) async fn mutate(
    input: &ConnectionInput,
    mutation: &Mutation,
) -> AppResult<MutationResult> {
    if input.read_only {
        return Err(invalid("This connection is read only."));
    }
    let (schema, table, snapshot, value) = match mutation {
        Mutation::InsertDocument {
            schema,
            table,
            document,
        } => (schema, table, None, Some(document)),
        Mutation::ReplaceDocument {
            schema,
            table,
            snapshot,
            document,
        } => (schema, table, Some(snapshot), Some(document)),
        Mutation::DeleteDocument {
            schema,
            table,
            snapshot,
        } => (schema, table, Some(snapshot), None),
        _ => return Err(invalid("Use explicit single-document changes for MongoDB.")),
    };
    if schema != &input.database
        || table.is_empty()
        || table.len() > 128
        || table.contains('\0')
        || table.starts_with("system.")
    {
        return Err(invalid(
            "Choose a collection in the current database. System collections cannot be edited.",
        ));
    }
    let original = snapshot.map(|snapshot| original(snapshot)).transpose()?;
    let value = value.map(replacement).transpose()?;
    if let (Some(original), Some(value)) = (&original, &value)
        && value.get("_id") != original.get("_id")
    {
        return Err(invalid("The document _id cannot be changed or removed."));
    }
    let collection = client(input)?
        .database(schema)
        .collection::<Document>(table);
    let collation = || Collation::builder().locale("simple".to_owned()).build();
    let affected_rows = match (original, value) {
        (None, Some(value)) => {
            collection.insert_one(value).await.map_err(upstream)?;
            1
        }
        (Some(original), Some(value)) => {
            collection
                .replace_one(unchanged(original), value)
                .collation(collation())
                .await
                .map_err(upstream)?
                .matched_count
        }
        (Some(original), None) => {
            collection
                .delete_one(unchanged(original))
                .collation(collation())
                .await
                .map_err(upstream)?
                .deleted_count
        }
        _ => unreachable!(),
    };
    if affected_rows != 1 {
        return Err(invalid(
            "The document changed or was deleted. Refresh and review it before trying again.",
        ));
    }
    Ok(MutationResult { affected_rows })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn unsafe_document_writes_are_rejected_before_connecting() {
        let mut input: ConnectionInput = serde_json::from_value(serde_json::json!({"provider":"mongodb","host":"localhost","port":1,"database":"test","username":"","password":"","ssl":false})).unwrap();
        let snapshot = snapshot(&doc! { "_id": "original" }).unwrap()["snapshot"]
            .as_str()
            .unwrap()
            .to_owned();
        for mutation in [
            Mutation::ReplaceDocument {
                schema: "test".into(),
                table: "items".into(),
                snapshot,
                document: serde_json::json!({"_id":"changed"}),
            },
            Mutation::DeleteDocument {
                schema: "test".into(),
                table: "items".into(),
                snapshot: "invalid".into(),
            },
            Mutation::InsertDocument {
                schema: "other".into(),
                table: "items".into(),
                document: serde_json::json!({}),
            },
            Mutation::InsertDocument {
                schema: "test".into(),
                table: "system.users".into(),
                document: serde_json::json!({}),
            },
        ] {
            assert!(mutate(&input, &mutation).await.is_err());
        }
        input.read_only = true;
        assert!(
            mutate(
                &input,
                &Mutation::InsertDocument {
                    schema: "test".into(),
                    table: "items".into(),
                    document: serde_json::json!({})
                }
            )
            .await
            .is_err()
        );
    }
    #[test]
    fn snapshots_preserve_order_types_and_operator_shaped_data() {
        let document = doc! { "_id": { "$ne": "data" }, "z": 1_i64, "a": { "z": 2, "a": 3 } };
        let snapshot = snapshot(&document).unwrap();
        let restored = original(snapshot["snapshot"].as_str().unwrap()).unwrap();
        assert_eq!(
            mongodb::bson::to_vec(&restored).unwrap(),
            mongodb::bson::to_vec(&document).unwrap()
        );
        let filter = unchanged(restored);
        assert!(filter.contains_key("$expr"));
        assert!(filter.get_document("_id").unwrap().contains_key("$eq"));
        for snapshot in ["", "0", "zz", "é"] {
            assert!(original(snapshot).is_err());
        }
    }
    #[test]
    fn document_validation_preserves_bson() {
        let document = replacement(&serde_json::json!({"n":{"$numberLong":"9007199254740993"},"nullable":null,"nested":{"$where":"literal data"}})).unwrap();
        assert_eq!(document.get_i64("n").unwrap(), 9007199254740993);
        assert!(replacement(&Value::Null).is_err());
        assert!(replacement(&serde_json::json!({"$set":{}})).is_err());
    }
}
