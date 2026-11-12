use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FindQuery {
    pub collection: String,
    #[serde(default = "empty_object")]
    filter: Value,
    #[serde(default = "empty_object")]
    sort: Value,
    #[serde(default = "empty_object")]
    pub projection: Value,
    pipeline: Option<Vec<Value>>,
    #[serde(skip)]
    ordered: Document,
}
pub(super) fn empty_object() -> Value {
    Value::Object(Map::new())
}
pub(super) fn document(value: Value) -> AppResult<Document> {
    fn check(value: &Value, depth: usize) -> bool {
        if depth > 32 {
            return false;
        }
        match value {
            Value::Object(map) => map.iter().all(|(key, value)| {
                !["$where", "$function", "$accumulator"].contains(&key.as_str())
                    && check(value, depth + 1)
            }),
            Value::Array(items) => items.iter().all(|value| check(value, depth + 1)),
            _ => true,
        }
    }
    if !value.is_object() || !check(&value, 0) {
        return Err(invalid(
            "Use JSON objects with at most 32 nesting levels. Server-side JavaScript is unavailable.",
        ));
    }
    match Bson::try_from(value).map_err(|_| invalid("Invalid MongoDB Extended JSON."))? {
        Bson::Document(document) => Ok(document),
        _ => Err(invalid("Expected a JSON object.")),
    }
}
pub(super) fn parse(query: &SqlQuery) -> AppResult<(FindQuery, Document, Document, Document)> {
    query.bounds().map_err(invalid)?;
    let mut find: FindQuery = serde_json::from_str(&query.sql).map_err(|_| {
        invalid("Use a JSON object with collection and filter/sort/projection or a pipeline array.")
    })?;
    // BSON's streaming deserializer keeps compound sort precedence and embedded
    // document field order, which serde_json::Value's sorted maps do not retain.
    find.ordered =
        serde_json::from_str(&query.sql).map_err(|_| invalid("Invalid MongoDB Extended JSON."))?;
    if find.pipeline.is_some()
        && (find.filter != empty_object()
            || find.sort != empty_object()
            || find.projection != empty_object())
    {
        return Err(invalid(
            "Use either a find query or an aggregation pipeline.",
        ));
    }
    if let Some(pipeline) = &find.pipeline {
        validate_pipeline(pipeline, 0)?;
    }
    if find.collection.is_empty() || find.collection.len() > 128 || find.collection.contains('\0') {
        return Err(invalid("Choose a collection name of at most 128 bytes."));
    }
    document(find.filter.clone())?;
    document(find.sort.clone())?;
    let filter = find
        .ordered
        .get_document("filter")
        .cloned()
        .unwrap_or_default();
    let sort = find
        .ordered
        .get_document("sort")
        .cloned()
        .unwrap_or_default();
    if sort
        .values()
        .any(|value| !matches!(value, Bson::Int32(1 | -1) | Bson::Int64(1 | -1)))
    {
        return Err(invalid("Sort directions must be 1 or -1."));
    }
    document(find.projection.clone())?;
    let projection = find
        .ordered
        .get_document("projection")
        .cloned()
        .unwrap_or_default();
    if projection.values().any(|value| {
        !matches!(
            value,
            Bson::Int32(0 | 1) | Bson::Int64(0 | 1) | Bson::Boolean(_)
        )
    }) {
        return Err(invalid("Projection values must be 0, 1, false, or true."));
    }
    Ok((find, filter, sort, projection))
}

fn validate_pipeline(stages: &[Value], depth: usize) -> AppResult<()> {
    if stages.len() > 50 || depth > 8 {
        return Err(invalid(
            "Use at most 50 aggregation stages and 8 nested facets.",
        ));
    }
    for stage in stages {
        let stage = document(stage.clone())?;
        if stage.len() != 1 {
            return Err(invalid("Each aggregation stage must contain one operator."));
        }
        let (operator, value) = stage.iter().next().unwrap();
        if !matches!(
            operator.as_str(),
            "$match"
                | "$project"
                | "$sort"
                | "$limit"
                | "$skip"
                | "$group"
                | "$count"
                | "$unwind"
                | "$addFields"
                | "$set"
                | "$unset"
                | "$replaceRoot"
                | "$replaceWith"
                | "$sortByCount"
                | "$bucket"
                | "$bucketAuto"
                | "$facet"
        ) {
            return Err(invalid(
                "Unsupported aggregation stage. Use read-only filtering, grouping, sorting, projection, or facets.",
            ));
        }
        if operator == "$facet" {
            let Bson::Document(facets) = value else {
                return Err(invalid("Facets must be an object of pipelines."));
            };
            for pipeline in facets.values() {
                let Bson::Array(stages) = pipeline else {
                    return Err(invalid("Each facet must be a pipeline array."));
                };
                validate_pipeline(
                    &stages
                        .iter()
                        .cloned()
                        .map(Bson::into_canonical_extjson)
                        .collect::<Vec<_>>(),
                    depth + 1,
                )?;
            }
        }
    }
    Ok(())
}

impl FindQuery {
    pub fn pipeline(&self) -> AppResult<Option<Vec<Document>>> {
        self.ordered
            .get_array("pipeline")
            .ok()
            .map(|stages| {
                stages
                    .iter()
                    .map(|stage| {
                        stage
                            .as_document()
                            .cloned()
                            .ok_or_else(|| invalid("Each stage must be a JSON object."))
                    })
                    .collect()
            })
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compound_sort_and_embedded_filter_keep_input_order() {
        let query = SqlQuery { sql: r#"{"collection":"items","filter":{"embedded":{"z":1,"a":2}},"sort":{"z":-1,"a":1},"pipeline":null}"#.into(), page: None, page_size: None };
        let (_, filter, sort, _) = parse(&query).unwrap();
        assert_eq!(
            sort.keys().map(String::as_str).collect::<Vec<_>>(),
            ["z", "a"]
        );
        assert_eq!(
            filter
                .get_document("embedded")
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["z", "a"]
        );
        let query = SqlQuery {
            sql: r#"{"collection":"items","pipeline":[{"$sort":{"z":-1,"a":1}}]}"#.into(),
            ..query
        };
        let (find, ..) = parse(&query).unwrap();
        let stages = find.pipeline().unwrap().unwrap();
        assert_eq!(
            stages[0]
                .get_document("$sort")
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["z", "a"]
        );
    }
    fn parse_value(value: Value) -> AppResult<(FindQuery, Document, Document, Document)> {
        parse(&SqlQuery {
            sql: value.to_string(),
            page: None,
            page_size: None,
        })
    }
    #[test]
    fn aggregation_allows_read_stages_and_rejects_writes_scripts_and_unknown_stages() {
        assert!(parse_value(serde_json::json!({"collection":"items","pipeline":[{"$match":{"n":{"$numberLong":"9007199254740993"}}},{"$group":{"_id":"$status","count":{"$sum":1}}},{"$facet":{"counts":[{"$count":"total"}]}}]})).is_ok());
        for pipeline in [
            serde_json::json!([{"$out":"items"}]),
            serde_json::json!([{"$merge":"items"}]),
            serde_json::json!([{"$lookup":{}}]),
            serde_json::json!([{"$facet":{"bad":[{"$out":"items"}]}}]),
            serde_json::json!([{"$project":{"n":{"$function":{}}}}]),
            serde_json::json!([{"$group":{"_id":null,"n":{"$accumulator":{}}}}]),
            serde_json::json!([{"$match":{"$where":"true"}}]),
            serde_json::json!([{}]),
            serde_json::json!([{"$sort":{},"$limit":1}]),
            serde_json::json!([{"$facet":{"bad":{}}}]),
            serde_json::json!([null]),
            Value::Array(vec![serde_json::json!({"$match":{}}); 51]),
        ] {
            assert!(
                parse_value(serde_json::json!({"collection":"items","pipeline":pipeline})).is_err()
            );
        }
        assert!(
            parse_value(serde_json::json!({"collection":"items","filter":{"x":1},"pipeline":[]}))
                .is_err()
        );
    }
}
