use crate::metrics::BenchmarkResult;
use async_trait::async_trait;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{info, error, debug};

#[async_trait]
pub trait ResultStorage: Send + Sync {
    async fn store_result(&self, result: &BenchmarkResult) -> Result<()>;
    async fn get_results(&self, filters: Option<&ResultFilters>) -> Result<Vec<BenchmarkResult>>;
    async fn get_result(&self, test_id: &str) -> Result<Option<BenchmarkResult>>;
    async fn delete_result(&self, test_id: &str) -> Result<()>;
    async fn get_metadata(&self) -> Result<StorageMetadata>;
}

#[derive(Debug, Clone, Default)]
pub struct ResultFilters {
    pub implementation_name: Option<String>,
    pub scenario_name: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub success_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageMetadata {
    pub total_results: usize,
    pub implementations: Vec<String>,
    pub scenarios: Vec<String>,
    pub date_range: Option<(DateTime<Utc>, DateTime<Utc>)>,
    pub storage_size_bytes: u64,
}

pub struct FileSystemStorage {
    base_path: PathBuf,
    compression_enabled: bool,
}

impl FileSystemStorage {
    pub fn new(base_path: PathBuf, compression_enabled: bool) -> Self {
        Self {
            base_path,
            compression_enabled,
        }
    }

    fn get_result_path(&self, test_id: &str) -> PathBuf {
        self.base_path.join(format!("{}.json", test_id))
    }

    fn get_index_path(&self) -> PathBuf {
        self.base_path.join("index.json")
    }

    async fn ensure_directory(&self) -> Result<()> {
        if !self.base_path.exists() {
            fs::create_dir_all(&self.base_path).await?;
        }
        Ok(())
    }

    async fn load_index(&self) -> Result<ResultIndex> {
        let index_path = self.get_index_path();
        if index_path.exists() {
            let content = fs::read_to_string(&index_path).await?;
            let index: ResultIndex = serde_json::from_str(&content)?;
            Ok(index)
        } else {
            Ok(ResultIndex::default())
        }
    }

    async fn save_index(&self, index: &ResultIndex) -> Result<()> {
        let index_path = self.get_index_path();
        let content = serde_json::to_string_pretty(index)?;
        fs::write(&index_path, content).await?;
        Ok(())
    }

    async fn update_index(&self, result: &BenchmarkResult) -> Result<()> {
        let mut index = self.load_index().await?;
        
        let entry = IndexEntry {
            test_id: result.test_id.clone(),
            implementation_name: result.implementation_name.clone(),
            scenario_name: result.scenario_name.clone(),
            start_time: result.start_time,
            end_time: result.end_time,
            success: result.success,
            file_path: self.get_result_path(&result.test_id),
        };

        index.entries.insert(result.test_id.clone(), entry);
        index.last_updated = Utc::now();
        
        self.save_index(&index).await?;
        Ok(())
    }
}

#[async_trait]
impl ResultStorage for FileSystemStorage {
    async fn store_result(&self, result: &BenchmarkResult) -> Result<()> {
        self.ensure_directory().await?;
        
        let result_path = self.get_result_path(&result.test_id);
        let content = if self.compression_enabled {
            // In a real implementation, you'd compress the data here
            serde_json::to_string_pretty(result)?
        } else {
            serde_json::to_string_pretty(result)?
        };

        fs::write(&result_path, content).await?;
        self.update_index(result).await?;
        
        info!("Stored result for test_id: {}", result.test_id);
        Ok(())
    }

    async fn get_results(&self, filters: Option<&ResultFilters>) -> Result<Vec<BenchmarkResult>> {
        let index = self.load_index().await?;
        let mut results = Vec::new();

        for entry in index.entries.values() {
            // Apply filters
            if let Some(filters) = filters {
                if let Some(impl_name) = &filters.implementation_name {
                    if &entry.implementation_name != impl_name {
                        continue;
                    }
                }

                if let Some(scenario_name) = &filters.scenario_name {
                    if &entry.scenario_name != scenario_name {
                        continue;
                    }
                }

                if let Some(start_time) = filters.start_time {
                    if entry.start_time < start_time {
                        continue;
                    }
                }

                if let Some(end_time) = filters.end_time {
                    if entry.end_time > end_time {
                        continue;
                    }
                }

                if let Some(success_only) = filters.success_only {
                    if entry.success != success_only {
                        continue;
                    }
                }
            }

            // Load the actual result
            if let Ok(content) = fs::read_to_string(&entry.file_path).await {
                if let Ok(result) = serde_json::from_str::<BenchmarkResult>(&content) {
                    results.push(result);
                } else {
                    error!("Failed to parse result for test_id: {}", entry.test_id);
                }
            } else {
                error!("Failed to read result file: {:?}", entry.file_path);
            }
        }

        Ok(results)
    }

    async fn get_result(&self, test_id: &str) -> Result<Option<BenchmarkResult>> {
        let result_path = self.get_result_path(test_id);
        
        if !result_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&result_path).await?;
        let result: BenchmarkResult = serde_json::from_str(&content)?;
        
        Ok(Some(result))
    }

    async fn delete_result(&self, test_id: &str) -> Result<()> {
        let result_path = self.get_result_path(test_id);
        
        if result_path.exists() {
            fs::remove_file(&result_path).await?;
        }

        // Update index
        let mut index = self.load_index().await?;
        index.entries.remove(test_id);
        index.last_updated = Utc::now();
        self.save_index(&index).await?;

        info!("Deleted result for test_id: {}", test_id);
        Ok(())
    }

    async fn get_metadata(&self) -> Result<StorageMetadata> {
        let index = self.load_index().await?;
        
        let mut implementations = std::collections::HashSet::new();
        let mut scenarios = std::collections::HashSet::new();
        let mut start_times = Vec::new();
        let mut end_times = Vec::new();
        let mut total_size = 0u64;

        for entry in index.entries.values() {
            implementations.insert(entry.implementation_name.clone());
            scenarios.insert(entry.scenario_name.clone());
            start_times.push(entry.start_time);
            end_times.push(entry.end_time);

            if let Ok(metadata) = fs::metadata(&entry.file_path).await {
                total_size += metadata.len();
            }
        }

        let date_range = if !start_times.is_empty() {
            Some((
                start_times.iter().min().unwrap().clone(),
                end_times.iter().max().unwrap().clone(),
            ))
        } else {
            None
        };

        Ok(StorageMetadata {
            total_results: index.entries.len(),
            implementations: implementations.into_iter().collect(),
            scenarios: scenarios.into_iter().collect(),
            date_range,
            storage_size_bytes: total_size,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ResultIndex {
    entries: HashMap<String, IndexEntry>,
    last_updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexEntry {
    test_id: String,
    implementation_name: String,
    scenario_name: String,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    success: bool,
    file_path: PathBuf,
}

pub struct InMemoryStorage {
    results: std::sync::Arc<tokio::sync::RwLock<HashMap<String, BenchmarkResult>>>,
}

impl InMemoryStorage {
    pub fn new() -> Self {
        Self {
            results: std::sync::Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl ResultStorage for InMemoryStorage {
    async fn store_result(&self, result: &BenchmarkResult) -> Result<()> {
        let mut results = self.results.write().await;
        results.insert(result.test_id.clone(), result.clone());
        info!("Stored result in memory for test_id: {}", result.test_id);
        Ok(())
    }

    async fn get_results(&self, filters: Option<&ResultFilters>) -> Result<Vec<BenchmarkResult>> {
        let results = self.results.read().await;
        let mut filtered_results = Vec::new();

        for result in results.values() {
            if let Some(filters) = filters {
                if let Some(impl_name) = &filters.implementation_name {
                    if &result.implementation_name != impl_name {
                        continue;
                    }
                }

                if let Some(scenario_name) = &filters.scenario_name {
                    if &result.scenario_name != scenario_name {
                        continue;
                    }
                }

                if let Some(start_time) = filters.start_time {
                    if result.start_time < start_time {
                        continue;
                    }
                }

                if let Some(end_time) = filters.end_time {
                    if result.end_time > end_time {
                        continue;
                    }
                }

                if let Some(success_only) = filters.success_only {
                    if result.success != success_only {
                        continue;
                    }
                }
            }

            filtered_results.push(result.clone());
        }

        Ok(filtered_results)
    }

    async fn get_result(&self, test_id: &str) -> Result<Option<BenchmarkResult>> {
        let results = self.results.read().await;
        Ok(results.get(test_id).cloned())
    }

    async fn delete_result(&self, test_id: &str) -> Result<()> {
        let mut results = self.results.write().await;
        results.remove(test_id);
        info!("Deleted result from memory for test_id: {}", test_id);
        Ok(())
    }

    async fn get_metadata(&self) -> Result<StorageMetadata> {
        let results = self.results.read().await;
        
        let mut implementations = std::collections::HashSet::new();
        let mut scenarios = std::collections::HashSet::new();
        let mut start_times = Vec::new();
        let mut end_times = Vec::new();

        for result in results.values() {
            implementations.insert(result.implementation_name.clone());
            scenarios.insert(result.scenario_name.clone());
            start_times.push(result.start_time);
            end_times.push(result.end_time);
        }

        let date_range = if !start_times.is_empty() {
            Some((
                start_times.iter().min().unwrap().clone(),
                end_times.iter().max().unwrap().clone(),
            ))
        } else {
            None
        };

        Ok(StorageMetadata {
            total_results: results.len(),
            implementations: implementations.into_iter().collect(),
            scenarios: scenarios.into_iter().collect(),
            date_range,
            storage_size_bytes: 0, // In-memory storage doesn't have persistent size
        })
    }
}