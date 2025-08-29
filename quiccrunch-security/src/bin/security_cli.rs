//! Security CLI tool for managing QuicCrunch security framework

use clap::{Parser, Subcommand};
use quiccrunch_security::{
    SecurityFramework, SecurityConfig, SecurityContext,
    auth::AuthenticationManager,
    rbac::RbacManager,
    monitoring::SecurityMonitor,
    logging::SecurityLogger,
    testing::{SecurityTestFramework, SecurityTestConfig, SecurityTestCategory},
    compliance::{ComplianceManager, ComplianceFramework},
};
use serde_json;
use std::process;
use tokio;

#[derive(Parser)]
#[command(author, version, about = "QuicCrunch Security Management CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Authentication management
    Auth {
        #[command(subcommand)]
        auth_command: AuthCommands,
    },
    /// Role-based access control
    Rbac {
        #[command(subcommand)]
        rbac_command: RbacCommands,
    },
    /// Security monitoring
    Monitor {
        #[command(subcommand)]
        monitor_command: MonitorCommands,
    },
    /// Security testing
    Test {
        #[command(subcommand)]
        test_command: TestCommands,
    },
    /// Compliance management
    Compliance {
        #[command(subcommand)]
        compliance_command: ComplianceCommands,
    },
    /// Generate security report
    Report {
        /// Output format (json, text)
        #[arg(short, long, default_value = "text")]
        format: String,
        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
    },
}

#[derive(Subcommand)]
enum AuthCommands {
    /// Create a new user
    CreateUser {
        /// User ID
        #[arg(short, long)]
        user_id: String,
        /// Username
        #[arg(short = 'n', long)]
        username: String,
        /// Email address
        #[arg(short, long)]
        email: Option<String>,
        /// User roles (comma-separated)
        #[arg(short, long)]
        roles: String,
    },
    /// Authenticate user
    Login {
        /// Username
        #[arg(short, long)]
        username: String,
        /// Password
        #[arg(short, long)]
        password: String,
    },
    /// Validate JWT token
    ValidateToken {
        /// JWT token
        #[arg(short, long)]
        token: String,
    },
    /// List active sessions
    Sessions,
}

#[derive(Subcommand)]
enum RbacCommands {
    /// Create a new role
    CreateRole {
        /// Role name
        #[arg(short, long)]
        name: String,
        /// Role description
        #[arg(short, long)]
        description: String,
        /// Permissions (comma-separated)
        #[arg(short, long)]
        permissions: String,
    },
    /// Assign role to user
    AssignRole {
        /// User ID
        #[arg(short, long)]
        user_id: String,
        /// Role name
        #[arg(short, long)]
        role: String,
    },
    /// Check user permissions
    CheckPermission {
        /// User ID
        #[arg(short, long)]
        user_id: String,
        /// Resource
        #[arg(short, long)]
        resource: String,
        /// Action
        #[arg(short, long)]
        action: String,
    },
    /// List all roles
    ListRoles,
    /// Get user roles
    UserRoles {
        /// User ID
        #[arg(short, long)]
        user_id: String,
    },
}

#[derive(Subcommand)]
enum MonitorCommands {
    /// Get recent security alerts
    Alerts {
        /// Number of alerts to show
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },
    /// Get threat indicators
    Threats,
    /// Generate security metrics report
    Metrics {
        /// Hours to look back
        #[arg(short, long, default_value = "24")]
        hours: i64,
    },
    /// Acknowledge alert
    AckAlert {
        /// Alert ID
        #[arg(short, long)]
        alert_id: String,
    },
}

#[derive(Subcommand)]
enum TestCommands {
    /// Run security tests
    Run {
        /// Target host
        #[arg(short = 'H', long, default_value = "localhost")]
        host: String,
        /// Target port
        #[arg(short, long, default_value = "8080")]
        port: u16,
        /// Test categories (comma-separated)
        #[arg(short, long)]
        categories: Option<String>,
    },
    /// Show test results
    Results,
    /// Generate test report
    Report {
        /// Output format
        #[arg(short, long, default_value = "json")]
        format: String,
    },
}

#[derive(Subcommand)]
enum ComplianceCommands {
    /// Perform compliance assessment
    Assess {
        /// Compliance framework
        #[arg(short, long, default_value = "soc2")]
        framework: String,
        /// Assessor name
        #[arg(short, long)]
        assessor: String,
        /// Assessment scope
        #[arg(short, long)]
        scope: String,
    },
    /// Show compliance metrics
    Metrics,
    /// List compliance controls
    Controls {
        /// Framework filter
        #[arg(short, long)]
        framework: Option<String>,
    },
    /// Show open findings
    Findings,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Initialize security configuration
    let config = match SecurityConfig::from_env() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to load security configuration: {}", e);
            process::exit(1);
        }
    };

    // Validate configuration
    if let Err(e) = config.validate() {
        eprintln!("Invalid security configuration: {}", e);
        process::exit(1);
    }

    // Initialize security framework
    let framework = match SecurityFramework::new(config) {
        Ok(framework) => framework,
        Err(e) => {
            eprintln!("Failed to initialize security framework: {}", e);
            process::exit(1);
        }
    };

    match cli.command {
        Commands::Auth { auth_command } => {
            if let Err(e) = handle_auth_command(framework.auth(), auth_command).await {
                eprintln!("Authentication command failed: {}", e);
                process::exit(1);
            }
        }
        Commands::Rbac { rbac_command } => {
            if let Err(e) = handle_rbac_command(framework.rbac(), rbac_command).await {
                eprintln!("RBAC command failed: {}", e);
                process::exit(1);
            }
        }
        Commands::Monitor { monitor_command } => {
            if let Err(e) = handle_monitor_command(framework.monitor(), monitor_command).await {
                eprintln!("Monitoring command failed: {}", e);
                process::exit(1);
            }
        }
        Commands::Test { test_command } => {
            if let Err(e) = handle_test_command(test_command).await {
                eprintln!("Testing command failed: {}", e);
                process::exit(1);
            }
        }
        Commands::Compliance { compliance_command } => {
            if let Err(e) = handle_compliance_command(compliance_command).await {
                eprintln!("Compliance command failed: {}", e);
                process::exit(1);
            }
        }
        Commands::Report { format, output } => {
            if let Err(e) = handle_report_command(&format, output.as_deref()).await {
                eprintln!("Report generation failed: {}", e);
                process::exit(1);
            }
        }
    }
}

async fn handle_auth_command(
    auth: &AuthenticationManager,
    command: AuthCommands,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        AuthCommands::CreateUser { user_id, username, email, roles } => {
            let role_list: Vec<String> = roles.split(',').map(|s| s.trim().to_string()).collect();
            let user = auth.create_user(user_id, username, email, role_list).await?;
            println!("Created user: {}", serde_json::to_string_pretty(&user)?);
        }
        AuthCommands::Login { username, password } => {
            let token = auth.authenticate_user(&username, &password).await?;
            println!("Authentication successful:");
            println!("Access Token: {}", token.access_token);
            if let Some(refresh_token) = token.refresh_token {
                println!("Refresh Token: {}", refresh_token);
            }
            println!("Expires in: {} seconds", token.expires_in);
        }
        AuthCommands::ValidateToken { token } => {
            let context = auth.validate_token(&token).await?;
            println!("Token is valid:");
            println!("User ID: {:?}", context.user_id);
            println!("Session ID: {}", context.session_id);
            println!("Roles: {:?}", context.roles);
            println!("Authenticated: {}", context.authenticated);
        }
        AuthCommands::Sessions => {
            let sessions = auth.get_active_sessions().await?;
            println!("Active sessions: {}", sessions.len());
            for session in sessions {
                println!("  Session {}: User {} (expires: {})", 
                    session.session_id, 
                    session.user_id, 
                    session.expires_at.format("%Y-%m-%d %H:%M:%S UTC")
                );
            }
        }
    }
    Ok(())
}

async fn handle_rbac_command(
    rbac: &RbacManager,
    command: RbacCommands,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        RbacCommands::CreateRole { name, description, permissions } => {
            let permission_list: Vec<String> = permissions.split(',').map(|s| s.trim().to_string()).collect();
            let role = rbac.create_role(name, description, permission_list, Vec::new()).await?;
            println!("Created role: {}", serde_json::to_string_pretty(&role)?);
        }
        RbacCommands::AssignRole { user_id, role } => {
            rbac.assign_role_to_user(&user_id, &role).await?;
            println!("Assigned role '{}' to user '{}'", role, user_id);
        }
        RbacCommands::CheckPermission { user_id, resource, action } => {
            let decision = rbac.check_permission(&user_id, &resource, &action).await?;
            println!("Permission check result:");
            println!("  Granted: {}", decision.granted);
            println!("  Reason: {}", decision.reason);
            println!("  Evaluation time: {}ms", decision.evaluation_time_ms);
        }
        RbacCommands::ListRoles => {
            let roles = rbac.get_all_roles().await?;
            println!("Available roles:");
            for role in roles {
                println!("  {}: {} ({})", role.name, role.description, 
                    if role.active { "active" } else { "inactive" });
                println!("    Permissions: {:?}", role.permissions);
            }
        }
        RbacCommands::UserRoles { user_id } => {
            let roles = rbac.get_user_roles(&user_id).await?;
            println!("User '{}' roles: {:?}", user_id, roles);
        }
    }
    Ok(())
}

async fn handle_monitor_command(
    monitor: &SecurityMonitor,
    command: MonitorCommands,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        MonitorCommands::Alerts { limit } => {
            let alerts = monitor.get_recent_alerts(limit).await?;
            println!("Recent security alerts ({}):", alerts.len());
            for alert in alerts {
                println!("  [{}] {}: {} (Risk: {:.2})", 
                    alert.level, alert.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
                    alert.title, alert.risk_score);
                println!("    {}", alert.description);
            }
        }
        MonitorCommands::Threats => {
            let indicators = monitor.get_high_confidence_indicators().await?;
            println!("High-confidence threat indicators ({}):", indicators.len());
            for indicator in indicators {
                println!("  {} ({}): {} - Confidence: {:.2}", 
                    indicator.indicator_type, indicator.severity,
                    indicator.value, indicator.confidence);
            }
        }
        MonitorCommands::Metrics { hours } => {
            let since = chrono::Utc::now() - chrono::Duration::hours(hours);
            let report = monitor.generate_metrics_report(since).await?;
            println!("Security metrics report:");
            println!("  Period: {} to {}", 
                report.period_start.format("%Y-%m-%d %H:%M:%S UTC"),
                report.period_end.format("%Y-%m-%d %H:%M:%S UTC"));
            println!("  Total events: {}", report.total_events);
            println!("  Authentication events: {}", report.authentication_events);
            println!("  Authentication failures: {}", report.authentication_failures);
            println!("  Authorization events: {}", report.authorization_events);
            println!("  Security violations: {}", report.security_violations);
            println!("  Critical alerts: {}", report.critical_alerts);
            println!("  Auth success rate: {:.2}%", report.auth_success_rate);
        }
        MonitorCommands::AckAlert { alert_id } => {
            monitor.acknowledge_alert(&alert_id).await?;
            println!("Alert '{}' acknowledged", alert_id);
        }
    }
    Ok(())
}

async fn handle_test_command(
    command: TestCommands,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        TestCommands::Run { host, port, categories } => {
            let test_categories = if let Some(cats) = categories {
                cats.split(',')
                    .map(|s| match s.trim().to_lowercase().as_str() {
                        "auth" => SecurityTestCategory::Authentication,
                        "authz" => SecurityTestCategory::Authorization,
                        "input" => SecurityTestCategory::InputValidation,
                        "encryption" => SecurityTestCategory::Encryption,
                        "session" => SecurityTestCategory::SessionManagement,
                        "rate" => SecurityTestCategory::RateLimiting,
                        "dos" => SecurityTestCategory::DosSecurity,
                        "data" => SecurityTestCategory::DataValidation,
                        "config" => SecurityTestCategory::ConfigurationSecurity,
                        "network" => SecurityTestCategory::NetworkSecurity,
                        _ => SecurityTestCategory::Authentication, // Default
                    })
                    .collect()
            } else {
                vec![
                    SecurityTestCategory::Authentication,
                    SecurityTestCategory::Authorization,
                    SecurityTestCategory::InputValidation,
                ]
            };

            let config = SecurityTestConfig {
                target_host: host,
                target_port: port,
                test_categories,
                max_concurrent_tests: 5,
                timeout_seconds: 30,
                credentials: None,
                custom_payloads: std::collections::HashMap::new(),
            };

            let mut framework = SecurityTestFramework::new(config);
            println!("Running security tests...");
            
            let results = framework.run_security_tests().await?;
            
            println!("Security test results:");
            for result in &results {
                let status = if result.passed { "PASS" } else { "FAIL" };
                println!("  [{}] {}: {} ({}ms)", 
                    status, result.test_id, result.test_name, result.execution_time_ms);
                if !result.passed {
                    println!("    Details: {}", result.details);
                }
            }

            let report = framework.generate_report();
            println!("\nSummary:");
            println!("  Total tests: {}", report.total_tests);
            println!("  Passed: {}", report.passed_tests);
            println!("  Failed: {}", report.failed_tests);
            println!("  Overall score: {:.1}%", report.overall_score);
        }
        TestCommands::Results => {
            println!("Test results command not implemented yet");
        }
        TestCommands::Report { format } => {
            println!("Test report command not implemented yet (format: {})", format);
        }
    }
    Ok(())
}

async fn handle_compliance_command(
    command: ComplianceCommands,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut compliance_manager = ComplianceManager::new();

    match command {
        ComplianceCommands::Assess { framework, assessor, scope } => {
            let framework_enum = match framework.to_lowercase().as_str() {
                "soc2" => ComplianceFramework::SOC2TypeII,
                "gdpr" => ComplianceFramework::GDPR,
                "hipaa" => ComplianceFramework::HIPAA,
                "pci" => ComplianceFramework::PCI_DSS,
                "iso27001" => ComplianceFramework::ISO27001,
                "nist" => ComplianceFramework::NIST,
                _ => return Err(format!("Unknown framework: {}", framework).into()),
            };

            println!("Performing {} compliance assessment...", framework);
            let assessment = compliance_manager.perform_assessment(framework_enum, assessor, scope).await?;
            
            println!("Assessment completed:");
            println!("  ID: {}", assessment.id);
            println!("  Overall rating: {:?}", assessment.overall_rating);
            println!("  Controls tested: {}", assessment.control_results.len());
            println!("  Findings: {}", assessment.findings.len());
            println!("  Next assessment due: {}", assessment.next_assessment_due.format("%Y-%m-%d"));
        }
        ComplianceCommands::Metrics => {
            let metrics = compliance_manager.get_compliance_metrics();
            println!("Compliance metrics:");
            println!("  Total controls: {}", metrics.total_controls);
            println!("  Implemented controls: {}", metrics.implemented_controls);
            println!("  Implementation percentage: {:.1}%", metrics.implementation_percentage);
            println!("  Open findings: {}", metrics.open_findings);
            println!("  Overdue tests: {}", metrics.overdue_tests);
            if let Some(last_assessment) = metrics.last_assessment_date {
                println!("  Last assessment: {}", last_assessment.format("%Y-%m-%d"));
            }
        }
        ComplianceCommands::Controls { framework } => {
            let controls = if let Some(fw) = framework {
                let framework_enum = match fw.to_lowercase().as_str() {
                    "soc2" => ComplianceFramework::SOC2TypeII,
                    "gdpr" => ComplianceFramework::GDPR,
                    "hipaa" => ComplianceFramework::HIPAA,
                    "pci" => ComplianceFramework::PCI_DSS,
                    "iso27001" => ComplianceFramework::ISO27001,
                    "nist" => ComplianceFramework::NIST,
                    _ => return Err(format!("Unknown framework: {}", fw).into()),
                };
                compliance_manager.get_controls_by_framework(&framework_enum)
            } else {
                compliance_manager.get_controls_by_framework(&ComplianceFramework::SOC2TypeII)
            };

            println!("Compliance controls:");
            for control in controls {
                println!("  {}: {}", control.id, control.title);
                println!("    Status: {:?}", control.implementation_status);
                println!("    Category: {:?}", control.category);
                println!("    Responsible: {}", control.responsible_party);
            }
        }
        ComplianceCommands::Findings => {
            let findings = compliance_manager.get_open_findings();
            println!("Open compliance findings ({}):", findings.len());
            for finding in findings {
                println!("  {} [{}]: {}", finding.id, finding.severity, finding.title);
                println!("    Category: {:?}", finding.category);
                println!("    Target resolution: {}", finding.target_resolution_date.format("%Y-%m-%d"));
                println!("    Responsible: {}", finding.responsible_party);
            }
        }
    }
    Ok(())
}

async fn handle_report_command(
    format: &str,
    output: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Generating comprehensive security report...");
    
    // This would generate a full security report combining all components
    let report = format!("QuicCrunch Security Report - Generated at {}", chrono::Utc::now());
    
    match output {
        Some(path) => {
            std::fs::write(path, report)?;
            println!("Report written to: {}", path);
        }
        None => {
            println!("{}", report);
        }
    }
    
    Ok(())
}