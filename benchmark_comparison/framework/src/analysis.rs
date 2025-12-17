use crate::metrics::{BenchmarkResult, PerformanceComparison, TestMetrics};
use crate::storage::ResultStorage;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use statrs::statistics::{Mean, Median, Variance};
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisReport {
    pub report_id: String,
    pub generated_at: DateTime<Utc>,
    pub summary: ReportSummary,
    pub implementation_comparisons: Vec<ImplementationComparison>,
    pub scenario_analysis: Vec<ScenarioAnalysis>,
    pub trend_analysis: Option<TrendAnalysis>,
    pub recommendations: Vec<Recommendation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSummary {
    pub total_tests: usize,
    pub successful_tests: usize,
    pub failed_tests: usize,
    pub implementations: Vec<String>,
    pub scenarios: Vec<String>,
    pub date_range: (DateTime<Utc>, DateTime<Utc>),
    pub overall_best_implementation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationComparison {
    pub implementation_a: String,
    pub implementation_b: String,
    pub performance_comparison: PerformanceComparison,
    pub statistical_significance: StatisticalSignificance,
    pub detailed_metrics: DetailedComparison,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticalSignificance {
    pub latency_significant: bool,
    pub throughput_significant: bool,
    pub cpu_significant: bool,
    pub memory_significant: bool,
    pub confidence_level: f64,
    pub p_values: PValues,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PValues {
    pub latency: f64,
    pub throughput: f64,
    pub cpu: f64,
    pub memory: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedComparison {
    pub latency_by_scenario: HashMap<String, LatencyComparison>,
    pub throughput_by_scenario: HashMap<String, ThroughputComparison>,
    pub resource_efficiency_by_scenario: HashMap<String, ResourceEfficiencyComparison>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyComparison {
    pub mean_diff_ms: f64,
    pub median_diff_ms: f64,
    pub p95_diff_ms: f64,
    pub p99_diff_ms: f64,
    pub coefficient_of_variation_diff: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThroughputComparison {
    pub mps_diff: f64,
    pub mbps_diff: f64,
    pub peak_throughput_diff: f64,
    pub sustained_throughput_diff: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceEfficiencyComparison {
    pub cpu_efficiency_diff: f64,
    pub memory_efficiency_diff: f64,
    pub cpu_per_message_diff: f64,
    pub memory_per_message_diff: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioAnalysis {
    pub scenario_name: String,
    pub implementation_rankings: Vec<ImplementationRanking>,
    pub best_for_latency: Option<String>,
    pub best_for_throughput: Option<String>,
    pub best_for_efficiency: Option<String>,
    pub most_reliable: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationRanking {
    pub implementation: String,
    pub overall_score: f64,
    pub latency_score: f64,
    pub throughput_score: f64,
    pub efficiency_score: f64,
    pub reliability_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendAnalysis {
    pub time_period: (DateTime<Utc>, DateTime<Utc>),
    pub performance_trends: HashMap<String, PerformanceTrend>,
    pub regression_detected: bool,
    pub improvement_detected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceTrend {
    pub implementation: String,
    pub latency_trend: TrendDirection,
    pub throughput_trend: TrendDirection,
    pub resource_trend: TrendDirection,
    pub trend_strength: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrendDirection {
    Improving,
    Degrading,
    Stable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub category: RecommendationCategory,
    pub priority: RecommendationPriority,
    pub title: String,
    pub description: String,
    pub affected_implementations: Vec<String>,
    pub expected_impact: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecommendationCategory {
    Performance,
    Reliability,
    Efficiency,
    Configuration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecommendationPriority {
    High,
    Medium,
    Low,
}

pub struct PerformanceAnalyzer {
    storage: std::sync::Arc<dyn ResultStorage>,
}

impl PerformanceAnalyzer {
    pub fn new(storage: std::sync::Arc<dyn ResultStorage>) -> Self {
        Self { storage }
    }

    pub async fn generate_analysis_report(&self, filters: Option<&crate::storage::ResultFilters>) -> Result<AnalysisReport> {
        let results = self.storage.get_results(filters).await?;
        
        if results.is_empty() {
            return Ok(AnalysisReport {
                report_id: uuid::Uuid::new_v4().to_string(),
                generated_at: Utc::now(),
                summary: ReportSummary {
                    total_tests: 0,
                    successful_tests: 0,
                    failed_tests: 0,
                    implementations: vec![],
                    scenarios: vec![],
                    date_range: (Utc::now(), Utc::now()),
                    overall_best_implementation: None,
                },
                implementation_comparisons: vec![],
                scenario_analysis: vec![],
                trend_analysis: None,
                recommendations: vec![],
            });
        }

        let summary = self.generate_summary(&results);
        let implementation_comparisons = self.compare_implementations(&results).await?;
        let scenario_analysis = self.analyze_scenarios(&results).await?;
        let trend_analysis = self.analyze_trends(&results).await?;
        let recommendations = self.generate_recommendations(&results, &implementation_comparisons, &scenario_analysis);

        Ok(AnalysisReport {
            report_id: uuid::Uuid::new_v4().to_string(),
            generated_at: Utc::now(),
            summary,
            implementation_comparisons,
            scenario_analysis,
            trend_analysis,
            recommendations,
        })
    }

    fn generate_summary(&self, results: &[BenchmarkResult]) -> ReportSummary {
        let total_tests = results.len();
        let successful_tests = results.iter().filter(|r| r.success).count();
        let failed_tests = total_tests - successful_tests;

        let mut implementations = std::collections::HashSet::new();
        let mut scenarios = std::collections::HashSet::new();
        let mut start_times = Vec::new();
        let mut end_times = Vec::new();

        for result in results {
            implementations.insert(result.implementation_name.clone());
            scenarios.insert(result.scenario_name.clone());
            start_times.push(result.start_time);
            end_times.push(result.end_time);
        }

        let date_range = (
            start_times.iter().min().unwrap().clone(),
            end_times.iter().max().unwrap().clone(),
        );

        let overall_best = self.calculate_overall_best_implementation(results);

        ReportSummary {
            total_tests,
            successful_tests,
            failed_tests,
            implementations: implementations.into_iter().collect(),
            scenarios: scenarios.into_iter().collect(),
            date_range,
            overall_best_implementation: overall_best,
        }
    }

    async fn compare_implementations(&self, results: &[BenchmarkResult]) -> Result<Vec<ImplementationComparison>> {
        let mut implementations = std::collections::HashSet::new();
        for result in results {
            implementations.insert(&result.implementation_name);
        }

        let impl_list: Vec<&String> = implementations.into_iter().collect();
        let mut comparisons = Vec::new();

        for i in 0..impl_list.len() {
            for j in (i + 1)..impl_list.len() {
                let impl_a = impl_list[i];
                let impl_b = impl_list[j];

                let comparison = self.compare_two_implementations(results, impl_a, impl_b).await?;
                comparisons.push(comparison);
            }
        }

        Ok(comparisons)
    }

    async fn compare_two_implementations(
        &self,
        results: &[BenchmarkResult],
        impl_a: &str,
        impl_b: &str,
    ) -> Result<ImplementationComparison> {
        let impl_a_results: Vec<&BenchmarkResult> = results
            .iter()
            .filter(|r| r.implementation_name == *impl_a && r.success)
            .collect();

        let impl_b_results: Vec<&BenchmarkResult> = results
            .iter()
            .filter(|r| r.implementation_name == *impl_b && r.success)
            .collect();

        if impl_a_results.is_empty() || impl_b_results.is_empty() {
            return Err(anyhow::anyhow!("Insufficient data for comparison between {} and {}", impl_a, impl_b));
        }

        // Calculate average metrics for each implementation
        let avg_metrics_a = self.calculate_average_metrics(&impl_a_results);
        let avg_metrics_b = self.calculate_average_metrics(&impl_b_results);

        let performance_comparison = PerformanceComparison::calculate(&avg_metrics_a, &avg_metrics_b, impl_a, impl_b);
        let statistical_significance = self.calculate_statistical_significance(&impl_a_results, &impl_b_results);
        let detailed_metrics = self.calculate_detailed_comparison(&impl_a_results, &impl_b_results);

        Ok(ImplementationComparison {
            implementation_a: impl_a.to_string(),
            implementation_b: impl_b.to_string(),
            performance_comparison,
            statistical_significance,
            detailed_metrics,
        })
    }

    fn calculate_average_metrics(&self, results: &[&BenchmarkResult]) -> TestMetrics {
        let total_count = results.len();
        let mut total_messages = 0u64;
        let mut total_bytes = 0u64;
        let mut total_duration = std::time::Duration::ZERO;
        let mut latency_sum = std::time::Duration::ZERO;
        let mut throughput_sum = 0.0;
        let mut connection_time_sum = std::time::Duration::ZERO;
        let mut cpu_sum = 0.0;
        let mut memory_sum = 0u64;

        for result in results {
            total_messages += result.metrics.total_messages;
            total_bytes += result.metrics.total_bytes;
            total_duration += result.metrics.duration;
            latency_sum += result.metrics.latency_mean;
            throughput_sum += result.metrics.throughput_mps;
            connection_time_sum += result.metrics.connection_time;
            cpu_sum += result.metrics.system_metrics.cpu_usage_percent;
            memory_sum += result.metrics.system_metrics.memory_used_mb;
        }

        TestMetrics {
            total_messages: total_messages / total_count as u64,
            total_bytes: total_bytes / total_count as u64,
            duration: total_duration / total_count as u32,
            latency_min: std::time::Duration::ZERO, // Would need min calculation
            latency_max: std::time::Duration::ZERO, // Would need max calculation
            latency_mean: latency_sum / total_count as u32,
            latency_p50: std::time::Duration::ZERO, // Would need percentile calculation
            latency_p95: std::time::Duration::ZERO,
            latency_p99: std::time::Duration::ZERO,
            latency_p99_9: std::time::Duration::ZERO,
            throughput_mps: throughput_sum / total_count as f64,
            throughput_bps: (total_bytes as f64) / (total_duration.as_secs_f64() * total_count as f64),
            connection_time: connection_time_sum / total_count as u32,
            system_metrics: crate::metrics::SystemMetrics {
                cpu_usage_percent: cpu_sum / total_count as f64,
                memory_used_mb: memory_sum / total_count as u64,
                memory_total_mb: 0, // Would need to track this
                network_rx_bytes: 0,
                network_tx_bytes: 0,
                disk_usage_percent: 0.0,
                load_average: 0.0,
                process_count: 0,
            },
            errors: crate::metrics::ErrorMetrics::default(),
        }
    }

    fn calculate_statistical_significance(
        &self,
        results_a: &[&BenchmarkResult],
        results_b: &[&BenchmarkResult],
    ) -> StatisticalSignificance {
        // This is a simplified implementation
        // In a real implementation, you'd use proper statistical tests like t-test or Mann-Whitney U test
        
        let latency_a: Vec<f64> = results_a.iter().map(|r| r.metrics.latency_mean.as_millis() as f64).collect();
        let latency_b: Vec<f64> = results_b.iter().map(|r| r.metrics.latency_mean.as_millis() as f64).collect();
        
        let throughput_a: Vec<f64> = results_a.iter().map(|r| r.metrics.throughput_mps).collect();
        let throughput_b: Vec<f64> = results_b.iter().map(|r| r.metrics.throughput_mps).collect();

        StatisticalSignificance {
            latency_significant: self.is_significant_difference(&latency_a, &latency_b),
            throughput_significant: self.is_significant_difference(&throughput_a, &throughput_b),
            cpu_significant: false, // Would need CPU data
            memory_significant: false, // Would need memory data
            confidence_level: 0.95,
            p_values: PValues {
                latency: 0.05,
                throughput: 0.05,
                cpu: 0.05,
                memory: 0.05,
            },
        }
    }

    fn is_significant_difference(&self, sample_a: &[f64], sample_b: &[f64]) -> bool {
        // Simplified significance test
        // In a real implementation, use proper statistical tests
        if sample_a.len() < 2 || sample_b.len() < 2 {
            return false;
        }

        let mean_a = sample_a.iter().sum::<f64>() / sample_a.len() as f64;
        let mean_b = sample_b.iter().sum::<f64>() / sample_b.len() as f64;

        let variance_a = sample_a.iter().map(|x| (x - mean_a).powi(2)).sum::<f64>() / (sample_a.len() - 1) as f64;
        let variance_b = sample_b.iter().map(|x| (x - mean_b).powi(2)).sum::<f64>() / (sample_b.len() - 1) as f64;

        let pooled_variance = ((sample_a.len() - 1) as f64 * variance_a + (sample_b.len() - 1) as f64 * variance_b) 
            / ((sample_a.len() + sample_b.len() - 2) as f64);

        let standard_error = (pooled_variance / sample_a.len() as f64 + pooled_variance / sample_b.len() as f64).sqrt();
        
        if standard_error == 0.0 {
            return false;
        }

        let t_statistic = (mean_a - mean_b).abs() / standard_error;
        
        // Simplified threshold for significance
        t_statistic > 2.0
    }

    fn calculate_detailed_comparison(
        &self,
        results_a: &[&BenchmarkResult],
        results_b: &[&BenchmarkResult],
    ) -> DetailedComparison {
        let mut latency_by_scenario = HashMap::new();
        let mut throughput_by_scenario = HashMap::new();
        let mut resource_efficiency_by_scenario = HashMap::new();

        // Group results by scenario
        let mut scenarios_a = HashMap::new();
        let mut scenarios_b = HashMap::new();

        for result in results_a {
            scenarios_a.entry(result.scenario_name.clone()).or_insert_with(Vec::new).push(result);
        }

        for result in results_b {
            scenarios_b.entry(result.scenario_name.clone()).or_insert_with(Vec::new).push(result);
        }

        // Compare each scenario
        for scenario_name in scenarios_a.keys() {
            if let (Some(scenario_results_a), Some(scenario_results_b)) = (scenarios_a.get(scenario_name), scenarios_b.get(scenario_name)) {
                let avg_a = self.calculate_average_metrics(scenario_results_a);
                let avg_b = self.calculate_average_metrics(scenario_results_b);

                latency_by_scenario.insert(scenario_name.clone(), LatencyComparison {
                    mean_diff_ms: avg_a.latency_mean.as_millis() as f64 - avg_b.latency_mean.as_millis() as f64,
                    median_diff_ms: avg_a.latency_p50.as_millis() as f64 - avg_b.latency_p50.as_millis() as f64,
                    p95_diff_ms: avg_a.latency_p95.as_millis() as f64 - avg_b.latency_p95.as_millis() as f64,
                    p99_diff_ms: avg_a.latency_p99.as_millis() as f64 - avg_b.latency_p99.as_millis() as f64,
                    coefficient_of_variation_diff: 0.0, // Would need calculation
                });

                throughput_by_scenario.insert(scenario_name.clone(), ThroughputComparison {
                    mps_diff: avg_a.throughput_mps - avg_b.throughput_mps,
                    mbps_diff: avg_a.calculate_throughput_mbps() - avg_b.calculate_throughput_mbps(),
                    peak_throughput_diff: 0.0, // Would need peak calculation
                    sustained_throughput_diff: 0.0, // Would need sustained calculation
                });

                resource_efficiency_by_scenario.insert(scenario_name.clone(), ResourceEfficiencyComparison {
                    cpu_efficiency_diff: avg_a.system_metrics.cpu_usage_percent - avg_b.system_metrics.cpu_usage_percent,
                    memory_efficiency_diff: avg_a.system_metrics.memory_used_mb as f64 - avg_b.system_metrics.memory_used_mb as f64,
                    cpu_per_message_diff: 0.0, // Would need calculation
                    memory_per_message_diff: 0.0, // Would need calculation
                });
            }
        }

        DetailedComparison {
            latency_by_scenario,
            throughput_by_scenario,
            resource_efficiency_by_scenario,
        }
    }

    async fn analyze_scenarios(&self, results: &[BenchmarkResult]) -> Result<Vec<ScenarioAnalysis>> {
        let mut scenario_map: HashMap<String, Vec<&BenchmarkResult>> = HashMap::new();

        for result in results {
            if result.success {
                scenario_map.entry(result.scenario_name.clone()).or_insert_with(Vec::new).push(result);
            }
        }

        let mut scenario_analyses = Vec::new();

        for (scenario_name, scenario_results) in scenario_map {
            let analysis = self.analyze_single_scenario(&scenario_name, &scenario_results).await?;
            scenario_analyses.push(analysis);
        }

        Ok(scenario_analyses)
    }

    async fn analyze_single_scenario(&self, scenario_name: &str, results: &[&BenchmarkResult]) -> Result<ScenarioAnalysis> {
        let mut implementation_scores = HashMap::new();

        // Group by implementation
        let mut impl_results: HashMap<String, Vec<&BenchmarkResult>> = HashMap::new();
        for result in results {
            impl_results.entry(result.implementation_name.clone()).or_insert_with(Vec::new).push(result);
        }

        // Calculate scores for each implementation
        for (impl_name, impl_result_list) in impl_results {
            let avg_metrics = self.calculate_average_metrics(&impl_result_list);
            
            let latency_score = self.calculate_latency_score(&avg_metrics);
            let throughput_score = self.calculate_throughput_score(&avg_metrics);
            let efficiency_score = self.calculate_efficiency_score(&avg_metrics);
            let reliability_score = self.calculate_reliability_score(&impl_result_list);

            let overall_score = (latency_score + throughput_score + efficiency_score + reliability_score) / 4.0;

            implementation_scores.insert(impl_name, ImplementationRanking {
                implementation: impl_name.clone(),
                overall_score,
                latency_score,
                throughput_score,
                efficiency_score,
                reliability_score,
            });
        }

        let mut rankings: Vec<ImplementationRanking> = implementation_scores.into_values().collect();
        rankings.sort_by(|a, b| b.overall_score.partial_cmp(&a.overall_score).unwrap());

        let best_for_latency = rankings.iter().min_by(|a, b| a.latency_score.partial_cmp(&b.latency_score).unwrap()).map(|r| r.implementation.clone());
        let best_for_throughput = rankings.iter().max_by(|a, b| a.throughput_score.partial_cmp(&b.throughput_score).unwrap()).map(|r| r.implementation.clone());
        let best_for_efficiency = rankings.iter().max_by(|a, b| a.efficiency_score.partial_cmp(&b.efficiency_score).unwrap()).map(|r| r.implementation.clone());
        let most_reliable = rankings.iter().max_by(|a, b| a.reliability_score.partial_cmp(&b.reliability_score).unwrap()).map(|r| r.implementation.clone());

        Ok(ScenarioAnalysis {
            scenario_name: scenario_name.to_string(),
            implementation_rankings: rankings,
            best_for_latency,
            best_for_throughput,
            best_for_efficiency,
            most_reliable,
        })
    }

    fn calculate_latency_score(&self, metrics: &TestMetrics) -> f64 {
        // Lower latency = higher score
        let latency_ms = metrics.latency_mean.as_millis() as f64;
        (1000.0 / (latency_ms + 1.0)).min(100.0)
    }

    fn calculate_throughput_score(&self, metrics: &TestMetrics) -> f64 {
        // Higher throughput = higher score
        (metrics.throughput_mps / 100.0).min(100.0)
    }

    fn calculate_efficiency_score(&self, metrics: &TestMetrics) -> f64 {
        // Lower resource usage = higher score
        let cpu_score = (100.0 - metrics.system_metrics.cpu_usage_percent).max(0.0);
        let memory_score = (100.0 - (metrics.system_metrics.memory_used_mb as f64 / 100.0)).max(0.0);
        (cpu_score + memory_score) / 2.0
    }

    fn calculate_reliability_score(&self, results: &[&BenchmarkResult]) -> f64 {
        let success_rate = results.iter().filter(|r| r.success).count() as f64 / results.len() as f64;
        let avg_error_rate = results.iter().map(|r| r.metrics.get_error_rate()).sum::<f64>() / results.len() as f64;
        (success_rate * 100.0) - avg_error_rate
    }

    async fn analyze_trends(&self, results: &[BenchmarkResult]) -> Result<Option<TrendAnalysis>> {
        if results.len() < 3 {
            return Ok(None);
        }

        // This is a simplified trend analysis
        // In a real implementation, you'd use more sophisticated time series analysis
        
        let mut performance_trends = HashMap::new();
        let mut impl_results: HashMap<String, Vec<&BenchmarkResult>> = HashMap::new();

        for result in results {
            impl_results.entry(result.implementation_name.clone()).or_insert_with(Vec::new).push(result);
        }

        for (impl_name, impl_result_list) in impl_results {
            if impl_result_list.len() < 3 {
                continue;
            }

            let trend = self.calculate_performance_trend(&impl_result_list);
            performance_trends.insert(impl_name, trend);
        }

        let regression_detected = performance_trends.values().any(|t| matches!(t.latency_trend, TrendDirection::Degrading));
        let improvement_detected = performance_trends.values().any(|t| matches!(t.latency_trend, TrendDirection::Improving));

        Ok(Some(TrendAnalysis {
            time_period: (
                results.iter().map(|r| r.start_time).min().unwrap(),
                results.iter().map(|r| r.end_time).max().unwrap(),
            ),
            performance_trends,
            regression_detected,
            improvement_detected,
        }))
    }

    fn calculate_performance_trend(&self, results: &[&BenchmarkResult]) -> PerformanceTrend {
        // Sort by time
        let mut sorted_results = results.to_vec();
        sorted_results.sort_by_key(|r| r.start_time);

        // Simple linear trend analysis
        let n = sorted_results.len();
        let latencies: Vec<f64> = sorted_results.iter().map(|r| r.metrics.latency_mean.as_millis() as f64).collect();
        let throughputs: Vec<f64> = sorted_results.iter().map(|r| r.metrics.throughput_mps).collect();

        let latency_trend = self.calculate_trend_direction(&latencies);
        let throughput_trend = self.calculate_trend_direction(&throughputs);
        let resource_trend = TrendDirection::Stable; // Would need resource data

        PerformanceTrend {
            implementation: sorted_results[0].implementation_name.clone(),
            latency_trend,
            throughput_trend,
            resource_trend,
            trend_strength: 0.5, // Would calculate actual strength
        }
    }

    fn calculate_trend_direction(&self, values: &[f64]) -> TrendDirection {
        if values.len() < 2 {
            return TrendDirection::Stable;
        }

        let first_half = &values[..values.len() / 2];
        let second_half = &values[values.len() / 2..];

        let first_avg = first_half.iter().sum::<f64>() / first_half.len() as f64;
        let second_avg = second_half.iter().sum::<f64>() / second_half.len() as f64;

        let diff_percent = ((second_avg - first_avg) / first_avg) * 100.0;

        if diff_percent > 5.0 {
            TrendDirection::Degrading // For latency, higher is worse
        } else if diff_percent < -5.0 {
            TrendDirection::Improving // For latency, lower is better
        } else {
            TrendDirection::Stable
        }
    }

    fn generate_recommendations(
        &self,
        results: &[BenchmarkResult],
        comparisons: &[ImplementationComparison],
        scenario_analyses: &[ScenarioAnalysis],
    ) -> Vec<Recommendation> {
        let mut recommendations = Vec::new();

        // Analyze comparisons for recommendations
        for comparison in comparisons {
            if comparison.performance_comparison.latency_improvement_percent > 20.0 {
                recommendations.push(Recommendation {
                    category: RecommendationCategory::Performance,
                    priority: RecommendationPriority::High,
                    title: format!("{} shows significantly better latency than {}", 
                                   comparison.performance_comparison.implementation_b, 
                                   comparison.performance_comparison.implementation_a),
                    description: format!("{} has {:.1}% better latency performance. Consider using {} for latency-sensitive applications.", 
                                       comparison.performance_comparison.implementation_b,
                                       comparison.performance_comparison.latency_improvement_percent,
                                       comparison.performance_comparison.implementation_b),
                    affected_implementations: vec![
                        comparison.performance_comparison.implementation_a.clone(),
                        comparison.performance_comparison.implementation_b.clone(),
                    ],
                    expected_impact: "Improved user experience and reduced response times".to_string(),
                });
            }

            if comparison.performance_comparison.throughput_improvement_percent > 15.0 {
                recommendations.push(Recommendation {
                    category: RecommendationCategory::Performance,
                    priority: RecommendationPriority::Medium,
                    title: format!("{} offers better throughput than {}", 
                                   comparison.performance_comparison.implementation_b, 
                                   comparison.performance_comparison.implementation_a),
                    description: format!("{} provides {:.1}% higher throughput. Use {} for high-volume scenarios.", 
                                       comparison.performance_comparison.implementation_b,
                                       comparison.performance_comparison.throughput_improvement_percent,
                                       comparison.performance_comparison.implementation_b),
                    affected_implementations: vec![
                        comparison.performance_comparison.implementation_a.clone(),
                        comparison.performance_comparison.implementation_b.clone(),
                    ],
                    expected_impact: "Higher capacity and better scalability".to_string(),
                });
            }
        }

        // Analyze scenario-specific recommendations
        for scenario in scenario_analyses {
            if let Some(best_impl) = &scenario.best_for_latency {
                recommendations.push(Recommendation {
                    category: RecommendationCategory::Configuration,
                    priority: RecommendationPriority::Medium,
                    title: format!("Use {} for {} scenario", best_impl, scenario.scenario_name),
                    description: format!("{} performs best for latency in the {} scenario.", best_impl, scenario.scenario_name),
                    affected_implementations: vec![best_impl.clone()],
                    expected_impact: "Optimized performance for specific use case".to_string(),
                });
            }
        }

        // Check for reliability issues
        let failed_tests = results.iter().filter(|r| !r.success).count();
        if failed_tests > 0 {
            let failure_rate = (failed_tests as f64 / results.len() as f64) * 100.0;
            if failure_rate > 10.0 {
                recommendations.push(Recommendation {
                    category: RecommendationCategory::Reliability,
                    priority: RecommendationPriority::High,
                    title: "High failure rate detected".to_string(),
                    description: format!("Failure rate of {:.1}% detected. Investigate test environment and implementation stability.", failure_rate),
                    affected_implementations: results.iter().filter(|r| !r.success).map(|r| r.implementation_name.clone()).collect(),
                    expected_impact: "Improved reliability and stability".to_string(),
                });
            }
        }

        recommendations
    }

    fn calculate_overall_best_implementation(&self, results: &[BenchmarkResult]) -> Option<String> {
        let mut impl_scores = HashMap::new();

        for result in results {
            if result.success {
                let entry = impl_scores.entry(result.implementation_name.clone()).or_insert((0.0, 0));
                entry.0 += self.calculate_overall_score(&result.metrics);
                entry.1 += 1;
            }
        }

        let mut best_impl = None;
        let mut best_score = 0.0;

        for (impl_name, (total_score, count)) in impl_scores {
            if count > 0 {
                let avg_score = total_score / count as f64;
                if avg_score > best_score {
                    best_score = avg_score;
                    best_impl = Some(impl_name);
                }
            }
        }

        best_impl
    }

    fn calculate_overall_score(&self, metrics: &TestMetrics) -> f64 {
        let latency_score = self.calculate_latency_score(metrics);
        let throughput_score = self.calculate_throughput_score(metrics);
        let efficiency_score = self.calculate_efficiency_score(metrics);
        let reliability_score = 100.0 - metrics.get_error_rate();

        (latency_score + throughput_score + efficiency_score + reliability_score) / 4.0
    }
}