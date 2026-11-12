use super::mysql;
use crate::adapters::sqlx;
use crate::{application::*, domain::*};
use async_trait::async_trait;

pub struct MariaDbAdapter;
#[async_trait]
impl DatabaseAdapter for MariaDbAdapter {
    async fn indexes(
        &self,
        input: &ConnectionInput,
        schema: &str,
        table: &str,
    ) -> AppResult<Vec<IndexInfo>> {
        super::read_query::bounded(mysql::indexes(input, schema, table)).await
    }

    async fn mutate(
        &self,
        input: &ConnectionInput,
        mutation: &Mutation,
    ) -> AppResult<MutationResult> {
        super::write::bounded(mysql::write::mutate(input, mutation)).await
    }
    fn maintenance_database(&self) -> Option<&'static str> {
        Some("")
    }
    fn provider(&self) -> Provider {
        Provider {
            id: "mariadb",
            name: "MariaDB",
        }
    }
    async fn test(&self, input: &ConnectionInput) -> AppResult<()> {
        super::read_query::bounded(async {
            let mut c = mysql::connect(input).await?;
            sqlx::query("SELECT 1").execute(&mut c).await?;
            Ok(())
        })
        .await
    }
    async fn databases(&self, input: &ConnectionInput) -> AppResult<Vec<String>> {
        super::read_query::bounded(mysql::databases(input)).await
    }
    async fn tables(&self, input: &ConnectionInput) -> AppResult<Vec<Table>> {
        super::read_query::bounded(mysql::tables(input)).await
    }
    async fn rows(&self, input: &ConnectionInput, q: &BrowseQuery) -> AppResult<TablePage> {
        super::read_query::bounded(mysql::rows(input, q)).await
    }
    async fn query(&self, input: &ConnectionInput, q: &SqlQuery) -> AppResult<TablePage> {
        super::read_query::bounded(mysql::query(input, q)).await
    }
}
