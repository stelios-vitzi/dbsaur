use dbsaur::{adapters::mongodb::MongoAdapter, application::DatabaseAdapter, domain::*};
use mongodb::{
    Client,
    bson::doc,
    options::{ClientOptions, Credential, ServerAddress},
};

#[tokio::test]
#[ignore = "Requires WEBDB_TEST_MONGODB_CONNECTION_JSON naming an explicitly disposable database"]
async fn browse_filter_projection_and_pagination() {
    let input: ConnectionInput = serde_json::from_str(
        &std::env::var("WEBDB_TEST_MONGODB_CONNECTION_JSON")
            .expect("Configure a disposable MongoDB database"),
    )
    .unwrap();
    assert_eq!(input.provider, "mongodb");
    assert!(!input.all_databases && !input.database.is_empty());
    assert!(
        !input.read_only,
        "The disposable fixture connection must permit writes"
    );
    input.validate().unwrap();
    // Fixture writes occur only in this explicitly configured, ignored test.
    let mut options = ClientOptions::builder()
        .hosts(vec![ServerAddress::Tcp {
            host: input.host.clone(),
            port: Some(input.port),
        }])
        .build();
    if !input.username.is_empty() {
        options.credential = Some(
            Credential::builder()
                .username(input.username.clone())
                .password(input.password.clone())
                .source(input.auth_source.clone().unwrap_or_else(|| "admin".into()))
                .build(),
        );
    }
    options.tls = Some(if input.ssl {
        mongodb::options::Tls::Enabled(Default::default())
    } else {
        mongodb::options::Tls::Disabled
    });
    let client = Client::with_options(options).unwrap();
    let name = format!("dbsaur_test_{}", uuid::Uuid::new_v4().simple());
    let collection = client
        .database(&input.database)
        .collection::<mongodb::bson::Document>(&name);
    collection
        .insert_many([
            doc! { "n": 9007199254740993_i64, "nested": { "ok": true } },
            doc! { "n": 9007199254740994_i64, "empty": mongodb::bson::Bson::Null },
        ])
        .await
        .unwrap();
    let result = async move {
        MongoAdapter.test(&input).await.unwrap();
        assert!(
            MongoAdapter
                .tables(&input)
                .await
                .unwrap()
                .iter()
                .any(|table| table.name == name)
        );
        let query = SqlQuery {
            sql: serde_json::json!({"collection":name,"sort":{"n":1},"projection":{"n":1,"_id":0}})
                .to_string(),
            page: Some(0),
            page_size: Some(1),
        };
        let first = MongoAdapter.query(&input, &query).await.unwrap();
        assert!(first.has_more);
        assert_eq!(first.rows[0]["n"]["$numberLong"], "9007199254740993");
        assert!(!first.rows[0].contains_key("_id"));
        assert!(first.editing.is_none());
        let second = MongoAdapter
            .query(
                &input,
                &SqlQuery {
                    page: Some(1),
                    ..query
                },
            )
            .await
            .unwrap();
        assert!(!second.has_more);
        assert_eq!(second.rows[0]["n"]["$numberLong"], "9007199254740994");
        let filtered = MongoAdapter
            .query(
                &input,
                &SqlQuery {
                    sql: serde_json::json!({"collection":name,"filter":{"nested.ok":true}})
                        .to_string(),
                    page: None,
                    page_size: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(filtered.rows.len(), 1);
        assert_eq!(filtered.rows[0]["nested"]["ok"], true);
        let snapshot = filtered.editing.as_ref().unwrap().originals[0]["snapshot"]
            .as_str()
            .unwrap()
            .to_owned();
        let mut replacement = filtered.rows[0].clone();
        replacement.insert(
            "edited".into(),
            serde_json::json!({"$numberDecimal":"1.20"}),
        );
        let replace = Mutation::ReplaceDocument {
            schema: input.database.clone(),
            table: name.clone(),
            snapshot: snapshot.clone(),
            document: serde_json::Value::Object(replacement),
        };
        assert_eq!(
            MongoAdapter
                .mutate(&input, &replace)
                .await
                .unwrap()
                .affected_rows,
            1
        );
        // The original snapshot must not overwrite or delete the now-changed document.
        assert!(MongoAdapter.mutate(&input, &replace).await.is_err());
        assert!(
            MongoAdapter
                .mutate(
                    &input,
                    &Mutation::DeleteDocument {
                        schema: input.database.clone(),
                        table: name.clone(),
                        snapshot
                    }
                )
                .await
                .is_err()
        );
        let aggregate = MongoAdapter.query(&input, &SqlQuery { sql: serde_json::json!({"collection":name,"pipeline":[{"$group":{"_id":null,"count":{"$sum":1}}}]}).to_string(), page: None, page_size: None }).await.unwrap();
        assert_eq!(aggregate.rows[0]["count"]["$numberInt"], "2");
        assert!(aggregate.editing.is_none());
        assert_eq!(MongoAdapter.mutate(&input, &Mutation::InsertDocument { schema: input.database.clone(), table: name.clone(), document: serde_json::json!({"_id":"inserted","nested":[null,true],"n":{"$numberLong":"9007199254740993"}}) }).await.unwrap().affected_rows, 1);
        let inserted = MongoAdapter
            .query(
                &input,
                &SqlQuery {
                    sql: serde_json::json!({"collection":name,"filter":{"_id":"inserted"}})
                        .to_string(),
                    page: None,
                    page_size: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(inserted.rows[0]["n"]["$numberLong"], "9007199254740993");
        let snapshot = inserted.editing.as_ref().unwrap().originals[0]["snapshot"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            MongoAdapter
                .mutate(
                    &input,
                    &Mutation::DeleteDocument {
                        schema: input.database.clone(),
                        table: name.clone(),
                        snapshot
                    }
                )
                .await
                .unwrap()
                .affected_rows,
            1
        );
    };
    // Clean the fixture even if assertions fail.
    let result = tokio::spawn(result).await;
    collection.drop().await.unwrap();
    result.unwrap();
}
