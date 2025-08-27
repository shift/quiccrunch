#!/usr/bin/env python3
"""
QUIC + Cap'n Proto Performance Analysis Tool

Analyzes benchmark results and generates comparison reports.
"""

import json
import argparse
import sys
import os
import glob
from datetime import datetime
from pathlib import Path
import statistics
from typing import Dict, List, Optional, Any

try:
    import matplotlib.pyplot as plt
    import pandas as pd
    HAS_PLOTTING = True
except ImportError:
    HAS_PLOTTING = False
    plt = None
    pd = None
    print("Warning: matplotlib/pandas not available. Install for plotting support.")

def load_benchmark_results(directory: str) -> List[Dict[str, Any]]:
    """Load all benchmark result JSON files from directory."""
    results = []
    pattern = os.path.join(directory, "*.json")
    
    for file_path in glob.glob(pattern):
        try:
            with open(file_path, 'r') as f:
                data = json.load(f)
                data['_file'] = file_path
                results.append(data)
        except (json.JSONDecodeError, KeyError) as e:
            print(f"Warning: Skipping invalid file {file_path}: {e}")
    
    return results

def filter_results(results: List[Dict], backend: Optional[str] = None, 
                  test_case: Optional[str] = None) -> List[Dict]:
    """Filter results by backend and/or test case."""
    filtered = results
    
    if backend:
        filtered = [r for r in filtered if r.get('backend') == backend]
    
    if test_case:
        filtered = [r for r in filtered if r.get('test_case') == test_case]
    
    return filtered

def analyze_latency_results(results: List[Dict]) -> Dict[str, Any]:
    """Analyze latency test results."""
    if not results:
        return {}
    
    analysis = {
        'total_runs': len(results),
        'backends': {},
    }
    
    # Group by backend
    by_backend = {}
    for result in results:
        backend = result['backend']
        if backend not in by_backend:
            by_backend[backend] = []
        by_backend[backend].append(result)
    
    for backend, backend_results in by_backend.items():
        metrics = [r['metrics'] for r in backend_results]
        
        # Aggregate statistics
        mean_latencies = [m['mean_us'] for m in metrics]
        p99_latencies = [m['p99_us'] for m in metrics]
        
        analysis['backends'][backend] = {
            'runs': len(backend_results),
            'mean_latency': {
                'avg': statistics.mean(mean_latencies),
                'min': min(mean_latencies),
                'max': max(mean_latencies),
                'stddev': statistics.stdev(mean_latencies) if len(mean_latencies) > 1 else 0
            },
            'p99_latency': {
                'avg': statistics.mean(p99_latencies),
                'min': min(p99_latencies),
                'max': max(p99_latencies),
                'stddev': statistics.stdev(p99_latencies) if len(p99_latencies) > 1 else 0
            }
        }
    
    return analysis

def analyze_flood_results(results: List[Dict]) -> Dict[str, Any]:
    """Analyze flood test results."""
    if not results:
        return {}
    
    analysis = {
        'total_runs': len(results),
        'backends': {},
    }
    
    # Group by backend
    by_backend = {}
    for result in results:
        backend = result['backend']
        if backend not in by_backend:
            by_backend[backend] = []
        by_backend[backend].append(result)
    
    for backend, backend_results in by_backend.items():
        metrics = [r['metrics'] for r in backend_results]
        
        # Aggregate statistics
        msg_rates = [m['messages_per_second'] for m in metrics]
        throughputs = [m['throughput_mbps'] for m in metrics]
        latencies = [m['mean_latency_us'] for m in metrics]
        
        analysis['backends'][backend] = {
            'runs': len(backend_results),
            'message_rate': {
                'avg': statistics.mean(msg_rates),
                'min': min(msg_rates),
                'max': max(msg_rates),
                'stddev': statistics.stdev(msg_rates) if len(msg_rates) > 1 else 0
            },
            'throughput_mbps': {
                'avg': statistics.mean(throughputs),
                'min': min(throughputs),
                'max': max(throughputs),
                'stddev': statistics.stdev(throughputs) if len(throughputs) > 1 else 0
            },
            'mean_latency_us': {
                'avg': statistics.mean(latencies),
                'min': min(latencies),
                'max': max(latencies),
                'stddev': statistics.stdev(latencies) if len(latencies) > 1 else 0
            }
        }
    
    return analysis

def analyze_stream_results(results: List[Dict]) -> Dict[str, Any]:
    """Analyze stream test results."""
    if not results:
        return {}
    
    analysis = {
        'total_runs': len(results),
        'backends': {},
    }
    
    # Group by backend
    by_backend = {}
    for result in results:
        backend = result['backend']
        if backend not in by_backend:
            by_backend[backend] = []
        by_backend[backend].append(result)
    
    for backend, backend_results in by_backend.items():
        metrics = [r['metrics'] for r in backend_results]
        
        # Aggregate statistics
        throughputs = [m['throughput_gbps'] for m in metrics]
        durations = [m['duration_s'] for m in metrics]
        
        analysis['backends'][backend] = {
            'runs': len(backend_results),
            'throughput_gbps': {
                'avg': statistics.mean(throughputs),
                'min': min(throughputs),
                'max': max(throughputs),
                'stddev': statistics.stdev(throughputs) if len(throughputs) > 1 else 0
            },
            'duration_s': {
                'avg': statistics.mean(durations),
                'min': min(durations),
                'max': max(durations),
                'stddev': statistics.stdev(durations) if len(durations) > 1 else 0
            }
        }
    
    return analysis

def compare_backends(analysis: Dict[str, Any]) -> Optional[Dict[str, Any]]:
    """Compare two backends if both are present."""
    backends = list(analysis.get('backends', {}).keys())
    
    if len(backends) != 2:
        return None
    
    backend1, backend2 = backends
    b1_data = analysis['backends'][backend1]
    b2_data = analysis['backends'][backend2]
    
    comparison = {
        'backend1': backend1,
        'backend2': backend2,
        'comparisons': {}
    }
    
    # Compare all available metrics
    for metric_category in b1_data.keys():
        if metric_category == 'runs':
            continue
            
        if metric_category in b2_data:
            b1_avg = b1_data[metric_category]['avg']
            b2_avg = b2_data[metric_category]['avg']
            
            if b1_avg > 0:  # Avoid division by zero
                improvement = ((b2_avg - b1_avg) / b1_avg) * 100
                comparison['comparisons'][metric_category] = {
                    f'{backend1}_avg': b1_avg,
                    f'{backend2}_avg': b2_avg,
                    'improvement_percent': improvement,
                    'winner': backend2 if improvement > 0 else backend1
                }
    
    return comparison

def generate_text_report(results: List[Dict], output_file: Optional[str] = None):
    """Generate a text report from benchmark results."""
    report_lines = []
    
    report_lines.append("QUIC + Cap'n Proto Performance Test Report")
    report_lines.append("=" * 50)
    report_lines.append(f"Generated: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    report_lines.append(f"Total Results: {len(results)}")
    report_lines.append("")
    
    # Analyze each test type
    test_types = ['latency', 'flood', 'stream']
    
    for test_type in test_types:
        test_results = filter_results(results, test_case=test_type)
        if not test_results:
            continue
            
        report_lines.append(f"{test_type.title()} Test Analysis")
        report_lines.append("-" * 30)
        
        if test_type == 'latency':
            analysis = analyze_latency_results(test_results)
        elif test_type == 'flood':
            analysis = analyze_flood_results(test_results)
        elif test_type == 'stream':
            analysis = analyze_stream_results(test_results)
        
        for backend, data in analysis.get('backends', {}).items():
            report_lines.append(f"\n{backend.upper()} Backend ({data['runs']} runs):")
            
            for metric, stats in data.items():
                if metric == 'runs':
                    continue
                report_lines.append(f"  {metric}:")
                report_lines.append(f"    Average: {stats['avg']:.3f}")
                report_lines.append(f"    Range: {stats['min']:.3f} - {stats['max']:.3f}")
                if stats['stddev'] > 0:
                    report_lines.append(f"    Std Dev: {stats['stddev']:.3f}")
        
        # Add comparison if two backends
        comparison = compare_backends(analysis)
        if comparison:
            report_lines.append(f"\nBackend Comparison ({comparison['backend1']} vs {comparison['backend2']}):")
            for metric, comp in comparison['comparisons'].items():
                improvement = comp['improvement_percent']
                winner = comp['winner']
                report_lines.append(f"  {metric}: {winner} wins by {abs(improvement):.1f}%")
        
        report_lines.append("")
    
    # Success criteria evaluation
    report_lines.append("Success Criteria Evaluation")
    report_lines.append("-" * 30)
    
    flood_results = filter_results(results, test_case='flood')
    if flood_results:
        for backend in ['quinn', 'tquic']:
            backend_floods = filter_results(flood_results, backend=backend)
            if backend_floods:
                latest = max(backend_floods, key=lambda x: x['timestamp'])
                msg_rate = latest['metrics']['messages_per_second']
                status = "✅ PASS" if msg_rate >= 1000 else "❌ FAIL"
                report_lines.append(f"{backend}: {msg_rate:.0f} msg/s {status}")
    
    report_lines.append("")
    report_lines.append("Raw Data Files:")
    for result in results:
        report_lines.append(f"  {result.get('_file', 'unknown')}")
    
    report_text = "\n".join(report_lines)
    
    if output_file:
        with open(output_file, 'w') as f:
            f.write(report_text)
        print(f"Report saved to: {output_file}")
    else:
        print(report_text)

def generate_plots(results: List[Dict], output_dir: str):
    """Generate performance comparison plots."""
    if not HAS_PLOTTING or plt is None:
        print("Plotting libraries not available. Install matplotlib and pandas.")
        return
    
    os.makedirs(output_dir, exist_ok=True)
    
    # Plot latency comparison
    latency_results = filter_results(results, test_case='latency')
    if latency_results:
        plt.figure(figsize=(12, 8))
        
        backends = {}
        for result in latency_results:
            backend = result['backend']
            if backend not in backends:
                backends[backend] = []
            backends[backend].append(result['metrics'])
        
        # Plot mean latency
        plt.subplot(2, 2, 1)
        for backend, metrics_list in backends.items():
            mean_latencies = [m['mean_us'] for m in metrics_list]
            plt.hist(mean_latencies, alpha=0.7, label=f'{backend} mean', bins=20)
        plt.xlabel('Mean Latency (μs)')
        plt.ylabel('Frequency')
        plt.title('Latency Distribution - Mean')
        plt.legend()
        
        # Plot P99 latency
        plt.subplot(2, 2, 2)
        for backend, metrics_list in backends.items():
            p99_latencies = [m['p99_us'] for m in metrics_list]
            plt.hist(p99_latencies, alpha=0.7, label=f'{backend} P99', bins=20)
        plt.xlabel('P99 Latency (μs)')
        plt.ylabel('Frequency')
        plt.title('Latency Distribution - P99')
        plt.legend()
        
        plt.tight_layout()
        plt.savefig(os.path.join(output_dir, 'latency_comparison.png'), dpi=300)
        plt.close()
    
    # Plot throughput comparison
    flood_results = filter_results(results, test_case='flood')
    if flood_results:
        plt.figure(figsize=(12, 6))
        
        backends = {}
        for result in flood_results:
            backend = result['backend']
            if backend not in backends:
                backends[backend] = []
            backends[backend].append(result['metrics'])
        
        # Plot message rate
        plt.subplot(1, 2, 1)
        backend_names = []
        msg_rates = []
        for backend, metrics_list in backends.items():
            backend_names.append(backend)
            rates = [m['messages_per_second'] for m in metrics_list]
            msg_rates.append(rates)
        
        plt.boxplot(msg_rates, labels=backend_names)
        plt.ylabel('Messages per Second')
        plt.title('Message Throughput Comparison')
        plt.grid(True, alpha=0.3)
        
        # Plot bandwidth
        plt.subplot(1, 2, 2)
        bandwidths = []
        for backend, metrics_list in backends.items():
            bw = [m['throughput_mbps'] for m in metrics_list]
            bandwidths.append(bw)
        
        plt.boxplot(bandwidths, labels=backend_names)
        plt.ylabel('Throughput (Mbps)')
        plt.title('Bandwidth Comparison')
        plt.grid(True, alpha=0.3)
        
        plt.tight_layout()
        plt.savefig(os.path.join(output_dir, 'throughput_comparison.png'), dpi=300)
        plt.close()
    
    print(f"Plots saved to: {output_dir}")

def main():
    parser = argparse.ArgumentParser(
        description="Analyze QUIC + Cap'n Proto benchmark results"
    )
    parser.add_argument('directory', help='Directory containing benchmark result JSON files')
    parser.add_argument('--backend', choices=['quinn', 'tquic'], 
                       help='Filter results by backend')
    parser.add_argument('--test-case', choices=['latency', 'flood', 'stream'],
                       help='Filter results by test case')
    parser.add_argument('--output', '-o', help='Output file for text report')
    parser.add_argument('--plots', help='Directory to save comparison plots')
    parser.add_argument('--json', help='Output analysis as JSON file')
    
    args = parser.parse_args()
    
    if not os.path.isdir(args.directory):
        print(f"Error: Directory {args.directory} does not exist")
        sys.exit(1)
    
    # Load results
    results = load_benchmark_results(args.directory)
    if not results:
        print(f"No benchmark results found in {args.directory}")
        sys.exit(1)
    
    print(f"Loaded {len(results)} benchmark results")
    
    # Filter results
    filtered_results = filter_results(results, args.backend, args.test_case)
    if not filtered_results:
        print("No results match the specified filters")
        sys.exit(1)
    
    print(f"Analyzing {len(filtered_results)} filtered results")
    
    # Generate text report
    generate_text_report(filtered_results, args.output)
    
    # Generate plots if requested
    if args.plots:
        generate_plots(filtered_results, args.plots)
    
    # Generate JSON analysis if requested
    if args.json:
        analysis = {}
        
        for test_type in ['latency', 'flood', 'stream']:
            test_results = filter_results(filtered_results, test_case=test_type)
            if test_results:
                if test_type == 'latency':
                    analysis[test_type] = analyze_latency_results(test_results)
                elif test_type == 'flood':
                    analysis[test_type] = analyze_flood_results(test_results)
                elif test_type == 'stream':
                    analysis[test_type] = analyze_stream_results(test_results)
        
        with open(args.json, 'w') as f:
            json.dump(analysis, f, indent=2)
        print(f"JSON analysis saved to: {args.json}")

if __name__ == '__main__':
    main()