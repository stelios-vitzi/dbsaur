use super::*;

pub(super) async fn page(input: &ConnectionInput, query: &SqlQuery) -> AppResult<TablePage> {
    let (find, filter, mut sort, projection) = parse(query)?;
    let (page, page_size) = query.bounds().map_err(invalid)?;
    let skip = u64::from(page) * u64::from(page_size);
    if skip > 100_000 {
        return Err(invalid(
            "MongoDB browsing is limited to the first 100,000 documents. Narrow the filter.",
        ));
    }
    if !sort.contains_key("_id") {
        sort.insert("_id", 1);
    }
    let client = client(input)?;
    let collection = client
        .database(&input.database)
        .collection::<Document>(&find.collection);
    let pipeline = find.pipeline()?;
    let editable = pipeline.is_none()
        && projection.is_empty()
        && !input.read_only
        && !find.collection.starts_with("system.");
    let mut cursor = if let Some(mut pipeline) = pipeline {
        pipeline.push(doc! { "$skip": skip as i64 });
        pipeline.push(doc! { "$limit": i64::from(page_size) + 1 });
        collection
            .aggregate(pipeline)
            .allow_disk_use(false)
            .batch_size(page_size.min(50))
            .max_time(TIMEOUT)
            .await
            .map_err(upstream)?
    } else {
        collection
            .find(filter)
            .sort(sort)
            .projection(projection)
            .skip(skip)
            .limit(i64::from(page_size) + 1)
            .batch_size(page_size.min(50))
            .max_time(TIMEOUT)
            .await
            .map_err(upstream)?
    };
    let mut originals = Vec::new();
    let mut rows = Vec::new();
    let mut bytes = 0;
    while cursor.advance().await.map_err(upstream)? {
        let document: Document = cursor.deserialize_current().map_err(upstream)?;
        if editable {
            let snapshot = write::snapshot(&document)?;
            bytes += serde_json::to_vec(&snapshot)
                .map_err(|_| invalid("Cannot encode document snapshot."))?
                .len();
            originals.push(snapshot);
        }
        let value = Bson::Document(document).into_canonical_extjson();
        bytes += value.to_string().len();
        if bytes > 8 * 1024 * 1024 {
            return Err(invalid(
                "Document page exceeds 8 MiB. Reduce the page size or use a projection.",
            ));
        }
        if let Value::Object(document) = value {
            rows.push(document);
        }
    }
    let has_more = rows.len() > page_size as usize;
    rows.truncate(page_size as usize);
    originals.truncate(page_size as usize);
    let editing = editable.then(|| QueryEditing {
        schema: input.database.clone(),
        table: find.collection,
        columns: Default::default(),
        originals,
    });
    Ok(TablePage {
        columns: vec![],
        rows,
        has_more,
        page,
        page_size,
        total_rows: None,
        editing,
    })
}
