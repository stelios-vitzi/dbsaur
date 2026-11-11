use crate::{application::*, domain::*};
use sqlparser::{
    ast::{
        Expr, GroupByExpr, Ident, ObjectNamePart, SelectItem, SelectItemQualifiedWildcardKind,
        SetExpr, Statement, TableFactor, WildcardAdditionalOptions,
    },
    dialect::Dialect,
    parser::Parser,
};
use std::collections::BTreeMap;

pub(super) struct Candidate {
    statement: Statement,
    pub name: Vec<String>,
    qualifier: Ident,
}
pub(super) struct Plan {
    pub sql: String,
    schema: String,
    table: String,
    base: Vec<Column>,
    visible: Vec<usize>,
    hidden: Vec<String>,
}

fn identifier(ident: &Ident, postgres: bool) -> String {
    if postgres && ident.quote_style.is_none() {
        ident.value.to_lowercase()
    } else {
        ident.value.clone()
    }
}

pub(super) fn candidate(sql: &str, dialect: &dyn Dialect, postgres: bool) -> Option<Candidate> {
    let mut statements = Parser::parse_sql(dialect, sql).ok()?;
    if statements.len() != 1 {
        return None;
    }
    let statement = statements.remove(0);
    let Statement::Query(query) = &statement else {
        return None;
    };
    if query.with.is_some()
        || !query.locks.is_empty()
        || query.for_clause.is_some()
        || !query.pipe_operators.is_empty()
    {
        return None;
    }
    let SetExpr::Select(select) = query.body.as_ref() else {
        return None;
    };
    if select.from.len() != 1
        || !select.from[0].joins.is_empty()
        || select.distinct.is_some()
        || select.having.is_some()
        || select.qualify.is_some()
        || select.into.is_some()
        || select.select_modifiers.is_some()
        || select.exclude.is_some()
        || !select.lateral_views.is_empty()
        || !select.connect_by.is_empty()
        || !select.named_window.is_empty()
        || select.value_table_mode.is_some()
    {
        return None;
    }
    if !matches!(&select.group_by, GroupByExpr::Expressions(values,modifiers) if values.is_empty() && modifiers.is_empty())
    {
        return None;
    }
    let TableFactor::Table {
        name,
        alias,
        args: None,
        version: None,
        sample: None,
        json_path: None,
        with_ordinality: false,
        partitions,
        ..
    } = &select.from[0].relation
    else {
        return None;
    };
    if !partitions.is_empty()
        || !(1..=2).contains(&name.0.len())
        || alias
            .as_ref()
            .is_some_and(|alias| !alias.columns.is_empty())
    {
        return None;
    }
    let qualifier =
        alias
            .as_ref()
            .map(|alias| alias.name.clone())
            .or_else(|| match name.0.last()? {
                ObjectNamePart::Identifier(ident) => Some(ident.clone()),
                _ => None,
            })?;
    let name = name
        .0
        .iter()
        .map(|part| match part {
            ObjectNamePart::Identifier(ident) => Some(identifier(ident, postgres)),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    // Only direct columns are editable. Aggregates, expressions, joins and views
    // cannot acquire write provenance by returning a conveniently named field.
    if select.projection.iter().any(|item| match item {
        SelectItem::UnnamedExpr(Expr::Identifier(_) | Expr::CompoundIdentifier(_)) => false,
        SelectItem::ExprWithAlias {expr:Expr::Identifier(_) | Expr::CompoundIdentifier(_),..} => false,
        SelectItem::Wildcard(options) | SelectItem::QualifiedWildcard(SelectItemQualifiedWildcardKind::ObjectName(_),options) => options != &WildcardAdditionalOptions::default(),
        _=>true,
    }) { return None; }
    Some(Candidate {
        statement,
        name,
        qualifier,
    })
}

pub(super) fn prepare(
    mut candidate: Candidate,
    schema: String,
    table: String,
    base: Vec<Column>,
    dialect: &dyn Dialect,
    provider: &str,
) -> Option<Plan> {
    if base.is_empty() || (provider != "postgres" && !base.iter().any(|column| column.primary_key))
    {
        return None;
    }
    let Statement::Query(query) = &mut candidate.statement else {
        return None;
    };
    let SetExpr::Select(select) = query.body.as_mut() else {
        return None;
    };
    let mut visible = vec![];
    for item in &select.projection {
        match item {
            SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _) => {
                visible.extend(0..base.len())
            }
            SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
                let ident = match expr {
                    Expr::Identifier(ident) => ident,
                    Expr::CompoundIdentifier(parts) => parts.last()?,
                    _ => return None,
                };
                let name = identifier(ident, provider == "postgres");
                visible.push(base.iter().position(|column| {
                    if provider == "postgres" {
                        column.name == name
                    } else {
                        column.name.eq_ignore_ascii_case(&name)
                    }
                })?);
            }
            _ => return None,
        }
    }
    let quote = if provider == "mysql" || provider == "mariadb" {
        '`'
    } else if provider == "sqlserver" {
        '['
    } else {
        '"'
    };
    let prefix = format!("dbsaur_{}_", uuid::Uuid::new_v4().simple());
    let hidden = base
        .iter()
        .enumerate()
        .map(|(index, _)| format!("{prefix}{index}"))
        .collect::<Vec<_>>();
    for (column, hidden) in base.iter().zip(&hidden) {
        select.projection.push(SelectItem::ExprWithAlias {
            expr: Expr::CompoundIdentifier(vec![
                candidate.qualifier.clone(),
                Ident::with_quote(quote, &column.name),
            ]),
            alias: Ident::with_quote(quote, hidden),
        });
    }
    let sql = super::sql::canonical(&candidate.statement, dialect).ok()?;
    Some(Plan {
        sql,
        schema,
        table,
        base,
        visible,
        hidden,
    })
}

pub(super) fn finish(page: &mut TablePage, plan: Plan) -> AppResult<()> {
    if page.columns.len() != plan.visible.len() + plan.hidden.len() {
        return Err(invalid(
            "The query result structure changed. Run the query again.",
        ));
    }
    let mut mapping = BTreeMap::new();
    for (index, base_index) in plan.visible.iter().enumerate() {
        let name = page.columns[index].name.clone();
        let mut column = plan.base[*base_index].clone();
        mapping.insert(name.clone(), column.name.clone());
        column.name = name;
        page.columns[index] = column;
    }
    page.columns.truncate(plan.visible.len());
    let mut originals = Vec::with_capacity(page.rows.len());
    for row in &mut page.rows {
        let mut original = serde_json::Map::new();
        for (column, hidden) in plan.base.iter().zip(&plan.hidden) {
            let value = row.remove(hidden).ok_or_else(|| {
                invalid("The query record snapshot is unavailable. Run the query again.")
            })?;
            original.insert(column.name.clone(), value);
        }
        originals.push(original);
    }
    page.editing = Some(QueryEditing {
        schema: plan.schema,
        table: plan.table,
        columns: mapping,
        originals,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::dialect::PostgreSqlDialect;
    #[test]
    fn only_unambiguous_direct_table_projections_receive_snapshots() {
        let dialect = &PostgreSqlDialect {};
        for sql in [
            "SELECT * FROM public.items",
            "SELECT t.name AS label FROM public.items t WHERE id=1 ORDER BY id LIMIT 10",
        ] {
            assert!(candidate(sql, dialect, true).is_some());
        }
        for sql in [
            "SELECT count(*) FROM items",
            "SELECT DISTINCT name FROM items",
            "SELECT * FROM a JOIN b ON a.id=b.id",
            "SELECT * FROM (SELECT * FROM items) t",
            "WITH x AS (SELECT * FROM items) SELECT * FROM x",
            "SELECT id+1 AS id FROM items",
            "SELECT * FROM items UNION ALL SELECT * FROM items",
        ] {
            assert!(candidate(sql, dialect, true).is_none(), "{sql}");
        }
    }
}
