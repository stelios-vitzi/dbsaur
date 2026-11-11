use dbsaur::adapters::sqlx;
use dbsaur::{
    adapters::postgres::{PostgresAdapter, quote_identifier},
    application::DatabaseAdapter,
    domain::{BrowseQuery, ConnectionInput, GeometryRequest, Mutation, SqlQuery},
};
use sqlx::{
    Connection, PgConnection,
    postgres::{PgConnectOptions, PgSslMode},
};

#[tokio::test]
#[ignore = "requires WEBDB_TEST_CONNECTION_JSON for a disposable database with PostGIS installed"]
async fn postgis_browse_query_preview_and_snapshot_editing() {
    let input: ConnectionInput = serde_json::from_str(
        &std::env::var("WEBDB_TEST_CONNECTION_JSON").expect("set disposable test connection JSON"),
    )
    .unwrap();
    assert_eq!(input.provider, "postgres");
    let options = PgConnectOptions::new()
        .host(&input.host)
        .port(input.port)
        .database(&input.database)
        .username(&input.username)
        .password(&input.password)
        .ssl_mode(if input.ssl {
            PgSslMode::VerifyFull
        } else {
            PgSslMode::Disable
        });
    let mut setup = PgConnection::connect_with(&options).await.unwrap();
    let extension_schema: String = sqlx::query_scalar("SELECT n.nspname::text FROM pg_extension e JOIN pg_namespace n ON n.oid=e.extnamespace WHERE e.extname='postgis'").fetch_one(&mut setup).await.expect("PostGIS must already be installed");
    let gis = quote_identifier(&extension_schema);
    let schema = format!("postgis_{}", uuid::Uuid::new_v4().simple());
    let table = format!("{}.\"places\"", quote_identifier(&schema));
    sqlx::query(sqlx::AssertSqlSafe(
        format!("CREATE SCHEMA {}", quote_identifier(&schema)).as_str(),
    ))
    .execute(&mut setup)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE TABLE {table} (id int PRIMARY KEY, note text, \"odd geom\" {gis}.geometry, geog {gis}.geography)").as_str())).execute(&mut setup).await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("INSERT INTO {table} VALUES (1, 'before', {gis}.ST_Transform({gis}.ST_GeomFromText('POINT(23.7275 37.9838)',4326),3857), {gis}.ST_GeogFromText('SRID=4326;POINT(23 38)')), (2, NULL, {gis}.ST_GeomFromText('POINT EMPTY',4326), NULL), (3, NULL, {gis}.ST_GeomFromText('POINT(1 2)'), NULL), (4, NULL, NULL, NULL)").as_str())).execute(&mut setup).await.unwrap();
    let adapter = PostgresAdapter;
    let page = adapter
        .rows(
            &input,
            &BrowseQuery {
                schema: schema.clone(),
                table: "places".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(page.columns[2].data_type, "geometry");
    assert_eq!(page.columns[3].data_type, "geography");
    assert!(page.rows[3]["odd geom"].is_null());
    let query = adapter
        .query(
            &input,
            &SqlQuery {
                sql: format!("SELECT \"odd geom\", geog FROM {table} ORDER BY id"),
                page: None,
                page_size: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(query.columns[0].data_type, "geometry");
    assert_eq!(query.rows[0]["odd geom"], page.rows[0]["odd geom"]);
    for (index, column, expected_srid) in [
        (0, "odd geom", 3857),
        (0, "geog", 4326),
        (1, "odd geom", 4326),
        (2, "odd geom", 0),
    ] {
        let preview = adapter
            .geometry(
                &input,
                &GeometryRequest {
                    value: page.rows[index][column].as_str().unwrap().into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(preview.srid, expected_srid);
        if index == 0 {
            let geometry = preview.geojson.unwrap();
            assert_eq!(geometry["type"], "Point");
            if column == "odd geom" {
                assert!((geometry["coordinates"][0].as_f64().unwrap() - 23.7275).abs() < 1e-7);
            }
        } else {
            assert!(preview.geojson.is_none());
            assert_eq!(preview.empty, index == 1);
        }
    }
    assert!(
        adapter
            .geometry(
                &input,
                &GeometryRequest {
                    value: "DEADBEEF".into()
                }
            )
            .await
            .is_err()
    );
    // Reading EWKB must preserve the snapshot comparison used by record editing.
    let result = adapter
        .mutate(
            &input,
            &Mutation::Update {
                schema: schema.clone(),
                table: "places".into(),
                original: page.rows[0].clone(),
                values: [("note".into(), Some("after".into()))].into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(result.affected_rows, 1);
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP SCHEMA {} CASCADE", quote_identifier(&schema)).as_str(),
    ))
    .execute(&mut setup)
    .await
    .unwrap();
}
