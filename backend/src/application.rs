use crate::domain::*;
use async_trait::async_trait;
use std::{collections::HashMap, sync::Arc};

#[derive(Debug)]
pub enum ErrorKind {
    InvalidInput,
    NotConnected,
    NotFound,
    Busy,
    Upstream,
    Timeout,
    Internal,
}
#[derive(Debug)]
pub struct AppError(pub ErrorKind, pub &'static str);
pub type AppResult<T> = Result<T, AppError>;
pub fn invalid(message: &'static str) -> AppError {
    AppError(ErrorKind::InvalidInput, message)
}

#[async_trait]
/// Provider boundary: keep browsing read only, bound request cost, and replace
/// driver details with safe errors. Database privileges remain the security boundary.
pub trait DatabaseAdapter: Send + Sync {
    async fn indexes(
        &self,
        _input: &ConnectionInput,
        _schema: &str,
        _table: &str,
    ) -> AppResult<Vec<IndexInfo>> {
        Err(invalid("Index details are unavailable for this provider."))
    }

    async fn geometry(
        &self,
        _input: &ConnectionInput,
        _request: &GeometryRequest,
    ) -> AppResult<GeometryPreview> {
        Err(invalid(
            "Map previews are available for PostGIS connections.",
        ))
    }
    async fn update_settings(
        &self,
        _input: &ConnectionInput,
        _request: &UpdateDatabaseSettings,
    ) -> AppResult<()> {
        Err(invalid(
            "Changing settings is unavailable for this provider.",
        ))
    }
    async fn settings(&self, _input: &ConnectionInput) -> AppResult<DatabaseSettings> {
        Err(invalid(
            "Database settings are unavailable for this provider.",
        ))
    }
    async fn mutate(
        &self,
        _input: &ConnectionInput,
        _mutation: &Mutation,
    ) -> AppResult<MutationResult> {
        Err(invalid("Editing is unavailable for this provider."))
    }
    fn provider(&self) -> Provider;
    /// Default connection database for instance discovery, if supported.
    fn maintenance_database(&self) -> Option<&'static str> {
        None
    }
    async fn databases(&self, input: &ConnectionInput) -> AppResult<Vec<String>> {
        Ok(vec![input.database.clone()])
    }
    async fn test(&self, input: &ConnectionInput) -> AppResult<()>;
    async fn tables(&self, input: &ConnectionInput) -> AppResult<Vec<Table>>;
    async fn rows(&self, input: &ConnectionInput, query: &BrowseQuery) -> AppResult<TablePage>;
    async fn query(&self, input: &ConnectionInput, query: &SqlQuery) -> AppResult<TablePage>;
}

#[async_trait]
/// Installation-wide storage: encrypt credentials at rest and list only labels.
pub trait ConnectionRepository: Send + Sync {
    async fn list(&self) -> AppResult<Vec<SavedConnection>>;
    async fn save(&self, name: &str, input: &ConnectionInput) -> AppResult<String>;
    async fn load(&self, id: &str) -> AppResult<ConnectionInput>;
    async fn delete(&self, id: &str) -> AppResult<()>;
}

pub struct DatabaseService {
    adapters: HashMap<&'static str, Arc<dyn DatabaseAdapter>>,
}
impl DatabaseService {
    pub fn new(adapters: Vec<Arc<dyn DatabaseAdapter>>) -> Self {
        Self {
            adapters: adapters.into_iter().map(|a| (a.provider().id, a)).collect(),
        }
    }
    pub fn providers(&self) -> Vec<Provider> {
        let mut providers: Vec<_> = self.adapters.values().map(|a| a.provider()).collect();
        providers.sort_by_key(|p| p.id);
        providers
    }
    pub fn resolve(&self, id: &str) -> AppResult<Arc<dyn DatabaseAdapter>> {
        self.adapters
            .get(id)
            .cloned()
            .ok_or_else(|| invalid("Choose a supported database type."))
    }
}
