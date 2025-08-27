#!/usr/bin/env python3
"""
QUIC + Cap'n Proto Performance Test Visualization Tool

Creates charts and visualizations from benchmark results to support the
specification's requirement for graphs and visual analysis.
"""

import json
import sys
import os
import argparse
from pathlib import Path
from typing import Dict, List, Any, Optional
import statistics

def load_json_results(directory: str) -> Dict[str, List[Dict[str, Any]]]:
    """Load all JSON results from a results directory."""
    results = {"quinn": [], "tquic": []}
    
    for backend in ["quinn", "tquic"]:
        backend_dir = Path(directory) / backend
        if backend_dir.exists():
            for json_file in backend_dir.glob("*.json"):
                try:
                    with open(json_file) as f:
                        result = json.load(f)
                        result['filename'] = json_file.name
                        results[backend].append(result)
                except (json.JSONDecodeError, IOError) as e:
                    print(f"Warning: Could not load {json_file}: {e}")
    
    return results

def create_ascii_histogram(values: List[float], title: str, bins: int = 20) -> str:
    """Create an ASCII histogram for latency distributions."""
    if not values:
        return f"{title}: No data\n"
    
    min_val = min(values)
    max_val = max(values)
    bin_width = (max_val - min_val) / bins
    
    # Create bins
    histogram = [0] * bins
    for value in values:
        bin_index = min(int((value - min_val) / bin_width), bins - 1)
        histogram[bin_index] += 1
    
    # Find max count for scaling
    max_count = max(histogram) if histogram else 1
    
    # Create ASCII chart
    chart = f"\n{title}\n" + "=" * len(title) + "\n"
    
    for i, count in enumerate(histogram):
        bin_start = min_val + i * bin_width
        bin_end = min_val + (i + 1) * bin_width
        bar_length = int((count / max_count) * 50) if max_count > 0 else 0
        bar = "█" * bar_length
        chart += f"{bin_start:6.1f}-{bin_end:6.1f}: {bar} ({count})\n"
    
    return chart

def create_comparison_table(quinn_data: List[Dict], tquic_data: List[Dict], test_type: str) -> str:
    """Create a comparison table for specific test type."""
    table = f"\n{test_type.upper()} COMPARISON TABLE\n"
    table += "=" * (len(test_type) + 17) + "\n\n"
    
    if test_type == "latency":
        table += "| Metric          | Quinn          | tQUIC          | Winner     |\n"
        table += "|-----------------|----------------|----------------|------------|\n"
        
        quinn_latency = [r for r in quinn_data if r.get('test_case') == 'latency']
        tquic_latency = [r for r in tquic_data if r.get('test_case') == 'latency']
        
        if quinn_latency and tquic_latency:
            q_metrics = quinn_latency[0]['metrics']
            t_metrics = tquic_latency[0]['metrics']
            
            metrics = [
                ('Mean RTT (μs)', 'mean_us'),
                ('Median RTT (μs)', 'median_us'),
                ('P95 RTT (μs)', 'p95_us'),
                ('P99 RTT (μs)', 'p99_us'),
                ('Max RTT (μs)', 'max_us')
            ]
            
            for metric_name, metric_key in metrics:
                q_val = q_metrics.get(metric_key, 0)
                t_val = t_metrics.get(metric_key, 0)
                winner = "Quinn" if q_val < t_val else "tQUIC" if t_val < q_val else "Tie"
                table += f"| {metric_name:<15} | {q_val:>12.1f} | {t_val:>12.1f} | {winner:<10} |\n"
    
    elif test_type == "flood":
        table += "| Metric            | Quinn          | tQUIC          | Winner     |\n"
        table += "|-------------------|----------------|----------------|------------|\n"
        
        quinn_flood = [r for r in quinn_data if r.get('test_case') == 'flood']
        tquic_flood = [r for r in tquic_data if r.get('test_case') == 'flood']
        
        if quinn_flood and tquic_flood:
            q_metrics = quinn_flood[0]['metrics']
            t_metrics = tquic_flood[0]['metrics']
            
            metrics = [
                ('Messages/sec', 'messages_per_second'),
                ('Total Messages', 'total_messages'),
                ('Throughput (Mbps)', 'throughput_mbps'),
                ('Mean Latency (μs)', 'mean_latency_us'),
                ('P99 Latency (μs)', 'p99_latency_us')
            ]
            
            for metric_name, metric_key in metrics:
                q_val = q_metrics.get(metric_key, 0)
                t_val = t_metrics.get(metric_key, 0)
                
                # For latency, lower is better; for others, higher is better
                if 'latency' in metric_key.lower():
                    winner = "Quinn" if q_val < t_val else "tQUIC" if t_val < q_val else "Tie"
                else:
                    winner = "Quinn" if q_val > t_val else "tQUIC" if t_val > q_val else "Tie"
                
                if metric_key in ['messages_per_second', 'total_messages']:
                    table += f"| {metric_name:<17} | {q_val:>12.0f} | {t_val:>12.0f} | {winner:<10} |\n"
                else:
                    table += f"| {metric_name:<17} | {q_val:>12.1f} | {t_val:>12.1f} | {winner:<10} |\n"
    
    elif test_type == "stream":
        table += "| Metric            | Quinn          | tQUIC          | Winner     |\n"
        table += "|-------------------|----------------|----------------|------------|\n"
        
        quinn_stream = [r for r in quinn_data if r.get('test_case') == 'stream']
        tquic_stream = [r for r in tquic_data if r.get('test_case') == 'stream']
        
        if quinn_stream and tquic_stream:
            q_metrics = quinn_stream[0]['metrics']
            t_metrics = tquic_stream[0]['metrics']
            
            metrics = [
                ('Throughput (Gbps)', 'throughput_gbps'),
                ('Duration (s)', 'duration_s'),
                ('Chunks/sec', 'chunk_rate_per_s'),
                ('Total Chunks', 'chunks_transferred')
            ]
            
            for metric_name, metric_key in metrics:
                q_val = q_metrics.get(metric_key, 0)
                t_val = t_metrics.get(metric_key, 0)
                
                # For duration, lower is better; for others, higher is better
                if metric_key == 'duration_s':
                    winner = "Quinn" if q_val < t_val else "tQUIC" if t_val < q_val else "Tie"
                else:
                    winner = "Quinn" if q_val > t_val else "tQUIC" if t_val > q_val else "Tie"
                
                if metric_key in ['chunks_transferred']:
                    table += f"| {metric_name:<17} | {q_val:>12.0f} | {t_val:>12.0f} | {winner:<10} |\n"
                else:
                    table += f"| {metric_name:<17} | {q_val:>12.2f} | {t_val:>12.2f} | {winner:<10} |\n"
    
    return table + "\n"

def generate_summary_statistics(results: Dict[str, List[Dict]]) -> str:
    """Generate summary statistics for all backends."""
    summary = "\nSUMMARY STATISTICS\n"
    summary += "==================\n\n"
    
    for backend, data in results.items():
        if not data:
            continue
            
        summary += f"{backend.upper()} Backend Summary:\n"
        summary += "-" * (len(backend) + 16) + "\n"
        
        # Group by test case
        test_cases = {}
        for result in data:
            test_case = result.get('test_case', 'unknown')
            if test_case not in test_cases:
                test_cases[test_case] = []
            test_cases[test_case].append(result)
        
        for test_case, test_results in test_cases.items():
            summary += f"\n{test_case.capitalize()} Tests: {len(test_results)} runs\n"
            
            if test_case == 'latency':
                latencies = [r['metrics']['mean_us'] for r in test_results if 'metrics' in r]
                if latencies:
                    summary += f"  Mean RTT range: {min(latencies):.1f} - {max(latencies):.1f} μs\n"
                    summary += f"  Average Mean RTT: {statistics.mean(latencies):.1f} μs\n"
            
            elif test_case == 'flood':
                rates = [r['metrics']['messages_per_second'] for r in test_results if 'metrics' in r]
                if rates:
                    summary += f"  Message rate range: {min(rates):.0f} - {max(rates):.0f} msg/s\n"
                    summary += f"  Average rate: {statistics.mean(rates):.0f} msg/s\n"
            
            elif test_case == 'stream':
                throughputs = [r['metrics']['throughput_gbps'] for r in test_results if 'metrics' in r]
                if throughputs:
                    summary += f"  Throughput range: {min(throughputs):.2f} - {max(throughputs):.2f} Gbps\n"
                    summary += f"  Average throughput: {statistics.mean(throughputs):.2f} Gbps\n"
        
        summary += "\n"
    
    return summary

def create_visualization_report(results_directory: str, output_file: Optional[str] = None) -> str:
    """Create a comprehensive visualization report."""
    results = load_json_results(results_directory)
    
    if not any(results.values()):
        return "No valid test results found in the specified directory."
    
    report = "QUIC + CAP'N PROTO PERFORMANCE VISUALIZATION REPORT\n"
    report += "=" * 54 + "\n"
    report += f"Generated from: {results_directory}\n"
    report += f"Timestamp: {Path(results_directory).name}\n\n"
    
    # Summary statistics
    report += generate_summary_statistics(results)
    
    # Comparison tables for each test type
    for test_type in ['latency', 'flood', 'stream']:
        if (any(r.get('test_case') == test_type for r in results['quinn']) and 
            any(r.get('test_case') == test_type for r in results['tquic'])):
            report += create_comparison_table(results['quinn'], results['tquic'], test_type)
    
    # Latency distribution histograms
    quinn_latency = [r for r in results['quinn'] if r.get('test_case') == 'latency']
    tquic_latency = [r for r in results['tquic'] if r.get('test_case') == 'latency']
    
    if quinn_latency and 'histogram' in quinn_latency[0].get('metrics', {}):
        # Extract histogram data for visualization
        q_hist = quinn_latency[0]['metrics']['histogram']
        quinn_values = []
        for value, count in q_hist:
            quinn_values.extend([value] * int(count))
        
        if quinn_values:
            report += create_ascii_histogram(quinn_values, "Quinn Latency Distribution (μs)")
    
    if tquic_latency and 'histogram' in tquic_latency[0].get('metrics', {}):
        t_hist = tquic_latency[0]['metrics']['histogram']
        tquic_values = []
        for value, count in t_hist:
            tquic_values.extend([value] * int(count))
        
        if tquic_values:
            report += create_ascii_histogram(tquic_values, "tQUIC Latency Distribution (μs)")
    
    # Performance recommendations
    report += "\nPERFORMANCE RECOMMENDATIONS\n"
    report += "===========================\n\n"
    
    recommendations = []
    
    # Analyze latency performance
    if (quinn_latency and tquic_latency and 
        'metrics' in quinn_latency[0] and 'metrics' in tquic_latency[0]):
        q_p99 = quinn_latency[0]['metrics'].get('p99_us', float('inf'))
        t_p99 = tquic_latency[0]['metrics'].get('p99_us', float('inf'))
        
        if q_p99 < t_p99:
            recommendations.append("🚀 For latency-sensitive applications: Choose Quinn (lower P99 latency)")
        elif t_p99 < q_p99:
            recommendations.append("🚀 For latency-sensitive applications: Choose tQUIC (lower P99 latency)")
    
    # Analyze throughput performance
    quinn_flood = [r for r in results['quinn'] if r.get('test_case') == 'flood']
    tquic_flood = [r for r in results['tquic'] if r.get('test_case') == 'flood']
    
    if (quinn_flood and tquic_flood and 
        'metrics' in quinn_flood[0] and 'metrics' in tquic_flood[0]):
        q_rate = quinn_flood[0]['metrics'].get('messages_per_second', 0)
        t_rate = tquic_flood[0]['metrics'].get('messages_per_second', 0)
        
        if q_rate > t_rate:
            recommendations.append("⚡ For high-throughput applications: Choose Quinn (higher message rate)")
        elif t_rate > q_rate:
            recommendations.append("⚡ For high-throughput applications: Choose tQUIC (higher message rate)")
    
    # Analyze streaming performance
    quinn_stream = [r for r in results['quinn'] if r.get('test_case') == 'stream']
    tquic_stream = [r for r in results['tquic'] if r.get('test_case') == 'stream']
    
    if (quinn_stream and tquic_stream and 
        'metrics' in quinn_stream[0] and 'metrics' in tquic_stream[0]):
        q_gbps = quinn_stream[0]['metrics'].get('throughput_gbps', 0)
        t_gbps = tquic_stream[0]['metrics'].get('throughput_gbps', 0)
        
        if q_gbps > t_gbps:
            recommendations.append("🌊 For bandwidth-intensive applications: Choose Quinn (higher streaming throughput)")
        elif t_gbps > q_gbps:
            recommendations.append("🌊 For bandwidth-intensive applications: Choose tQUIC (higher streaming throughput)")
    
    if recommendations:
        for rec in recommendations:
            report += f"{rec}\n"
    else:
        report += "📊 Performance is comparable between backends - choose based on other factors\n"
    
    report += "\nNEXT STEPS\n"
    report += "==========\n"
    report += "1. Run tests with production-like hardware for validation\n"
    report += "2. Test with real network conditions (latency, packet loss)\n"
    report += "3. Conduct longer-duration stability tests\n"
    report += "4. Profile memory allocation patterns with heaptrack\n"
    report += "5. Test with application-specific message patterns\n"
    
    # Save to file if requested
    if output_file:
        with open(output_file, 'w') as f:
            f.write(report)
        print(f"Visualization report saved to: {output_file}")
    
    return report

def main():
    parser = argparse.ArgumentParser(description='Generate visualization report from benchmark results')
    parser.add_argument('results_dir', help='Directory containing benchmark results')
    parser.add_argument('-o', '--output', help='Output file path (optional)')
    parser.add_argument('--print', action='store_true', help='Print report to stdout')
    
    args = parser.parse_args()
    
    if not os.path.exists(args.results_dir):
        print(f"Error: Results directory '{args.results_dir}' does not exist")
        sys.exit(1)
    
    report = create_visualization_report(args.results_dir, args.output)
    
    if args.print or not args.output:
        print(report)

if __name__ == "__main__":
    main()