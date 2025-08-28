use quiccrunch_core::{power::PowerMonitor};
use quiccrunch_core::transport::{QuicTransport, Transport, TransportSettings, TlsConfig};
use std::time::Duration;
use tokio::time;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut monitor = PowerMonitor::new()?;
    
    // Configure based on power state
    let settings = match monitor.get_cpu_governor()?.as_str() {
        "powersave" => {
            TransportSettings { max_concurrent_uni_streams: 100, max_concurrent_bidi_streams: 100, keep_alive_interval_ms: Some(10_000), ..Default::default() }
        }
        "performance" => {
            TransportSettings { max_concurrent_uni_streams: 1000, max_concurrent_bidi_streams: 1000, keep_alive_interval_ms: Some(5_000), ..Default::default() }
        }
        _ => TransportSettings::default()
    };
    
    let tls = TlsConfig { cert: rustls::Certificate(vec![]), key: rustls::PrivateKey(vec![]), ca_cert: rustls::Certificate(vec![]) };
    let _transport = QuicTransport::with_settings(tls, settings)?;
    
    // Monitor power metrics
    let mut interval = time::interval(Duration::from_secs(1));
    loop {
        interval.tick().await;
        monitor.update_metrics()?;
        
        let efficiency = (monitor.get_package_power()? + monitor.get_dram_power()?) /
                        monitor.get_cpu_frequency()?;
                        
        println!("Power efficiency: {:.3}W/MHz", efficiency);
    }
}