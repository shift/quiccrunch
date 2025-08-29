//! Compliance framework for SOC2, GDPR, and other regulatory requirements

use crate::{SecurityError, SecurityResult, SecurityEventType};
use crate::logging::SecurityEvent;
use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Compliance frameworks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComplianceFramework {
    SOC2TypeII,
    GDPR,
    HIPAA,
    PCI_DSS,
    ISO27001,
    NIST,
    Custom(String),
}

/// Compliance control categories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ControlCategory {
    AccessControl,
    Authentication,
    Authorization,
    DataProtection,
    Encryption,
    Logging,
    Monitoring,
    IncidentResponse,
    BusinessContinuity,
    RiskManagement,
    VendorManagement,
    PhysicalSecurity,
    NetworkSecurity,
}

/// Compliance control
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceControl {
    pub id: String,
    pub framework: ComplianceFramework,
    pub category: ControlCategory,
    pub title: String,
    pub description: String,
    pub requirements: Vec<String>,
    pub implementation_status: ImplementationStatus,
    pub evidence_required: Vec<String>,
    pub testing_frequency: TestingFrequency,
    pub responsible_party: String,
    pub last_tested: Option<DateTime<Utc>>,
    pub next_test_due: Option<DateTime<Utc>>,
    pub control_effectiveness: ControlEffectiveness,
    pub exceptions: Vec<ControlException>,
}

/// Implementation status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImplementationStatus {
    NotImplemented,
    InProgress,
    Implemented,
    Operating,
    Effective,
}

/// Testing frequency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestingFrequency {
    Continuous,
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    Annually,
    OnDemand,
}

/// Control effectiveness rating
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ControlEffectiveness {
    Ineffective,
    PartiallyEffective,
    Effective,
    NotTested,
}

/// Control exception
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlException {
    pub id: String,
    pub description: String,
    pub justification: String,
    pub risk_assessment: String,
    pub compensating_controls: Vec<String>,
    pub approved_by: String,
    pub approval_date: DateTime<Utc>,
    pub expiration_date: Option<DateTime<Utc>>,
    pub remediation_plan: Option<String>,
}

/// Compliance evidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceEvidence {
    pub id: String,
    pub control_id: String,
    pub evidence_type: EvidenceType,
    pub title: String,
    pub description: String,
    pub file_path: Option<String>,
    pub content: Option<String>,
    pub collected_by: String,
    pub collected_at: DateTime<Utc>,
    pub retention_period: Duration,
    pub verified: bool,
    pub verification_date: Option<DateTime<Utc>>,
    pub verified_by: Option<String>,
}

/// Evidence types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EvidenceType {
    Documentation,
    LogFile,
    Screenshot,
    Configuration,
    TestResult,
    Report,
    Certificate,
    Policy,
    Procedure,
    AuditReport,
    RiskAssessment,
}

/// Compliance assessment result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceAssessment {
    pub id: String,
    pub framework: ComplianceFramework,
    pub assessment_date: DateTime<Utc>,
    pub assessor: String,
    pub scope: String,
    pub control_results: Vec<ControlAssessmentResult>,
    pub overall_rating: ComplianceRating,
    pub findings: Vec<ComplianceFinding>,
    pub recommendations: Vec<String>,
    pub next_assessment_due: DateTime<Utc>,
}

/// Control assessment result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlAssessmentResult {
    pub control_id: String,
    pub rating: ComplianceRating,
    pub test_method: String,
    pub test_results: String,
    pub evidence_reviewed: Vec<String>,
    pub deficiencies: Vec<String>,
    pub recommendations: Vec<String>,
}

/// Compliance rating
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComplianceRating {
    Compliant,
    PartiallyCompliant,
    NonCompliant,
    NotApplicable,
    NotTested,
}

/// Compliance finding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceFinding {
    pub id: String,
    pub severity: FindingSeverity,
    pub category: ControlCategory,
    pub title: String,
    pub description: String,
    pub impact: String,
    pub recommendation: String,
    pub remediation_timeline: String,
    pub responsible_party: String,
    pub status: FindingStatus,
    pub identified_date: DateTime<Utc>,
    pub target_resolution_date: DateTime<Utc>,
    pub actual_resolution_date: Option<DateTime<Utc>>,
}

/// Finding severity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FindingSeverity {
    Critical,
    High,
    Medium,
    Low,
    Informational,
}

/// Finding status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FindingStatus {
    Open,
    InProgress,
    Resolved,
    Accepted,
    Deferred,
}

/// Compliance manager
pub struct ComplianceManager {
    controls: HashMap<String, ComplianceControl>,
    evidence: HashMap<String, ComplianceEvidence>,
    assessments: Vec<ComplianceAssessment>,
    findings: Vec<ComplianceFinding>,
}

impl ComplianceManager {
    pub fn new() -> Self {
        let mut manager = Self {
            controls: HashMap::new(),
            evidence: HashMap::new(),
            assessments: Vec::new(),
            findings: Vec::new(),
        };

        // Initialize default SOC2 controls
        manager.initialize_soc2_controls().expect("Failed to initialize SOC2 controls");

        manager
    }

    /// Initialize SOC2 Type II controls
    fn initialize_soc2_controls(&mut self) -> SecurityResult<()> {
        // Access Control Controls
        self.add_control(ComplianceControl {
            id: "CC6.1".to_string(),
            framework: ComplianceFramework::SOC2TypeII,
            category: ControlCategory::AccessControl,
            title: "Logical and Physical Access Controls".to_string(),
            description: "The entity implements logical and physical access controls to protect against threats from sources outside its system boundaries.".to_string(),
            requirements: vec![
                "Implement user access provisioning process".to_string(),
                "Regular access reviews and certifications".to_string(),
                "Segregation of duties for privileged access".to_string(),
            ],
            implementation_status: ImplementationStatus::Implemented,
            evidence_required: vec![
                "Access control policies".to_string(),
                "User access logs".to_string(),
                "Access review reports".to_string(),
            ],
            testing_frequency: TestingFrequency::Quarterly,
            responsible_party: "Security Team".to_string(),
            last_tested: None,
            next_test_due: Some(Utc::now() + Duration::days(90)),
            control_effectiveness: ControlEffectiveness::NotTested,
            exceptions: Vec::new(),
        })?;

        // Authentication Controls
        self.add_control(ComplianceControl {
            id: "CC6.2".to_string(),
            framework: ComplianceFramework::SOC2TypeII,
            category: ControlCategory::Authentication,
            title: "User Authentication".to_string(),
            description: "Prior to issuing system credentials and granting system access, the entity registers and authorizes new internal and external users.".to_string(),
            requirements: vec![
                "Multi-factor authentication for privileged users".to_string(),
                "Strong password policies".to_string(),
                "Account lockout mechanisms".to_string(),
            ],
            implementation_status: ImplementationStatus::Implemented,
            evidence_required: vec![
                "Authentication policies".to_string(),
                "MFA configuration screenshots".to_string(),
                "Password policy documentation".to_string(),
            ],
            testing_frequency: TestingFrequency::Quarterly,
            responsible_party: "Security Team".to_string(),
            last_tested: None,
            next_test_due: Some(Utc::now() + Duration::days(90)),
            control_effectiveness: ControlEffectiveness::NotTested,
            exceptions: Vec::new(),
        })?;

        // Logging and Monitoring Controls
        self.add_control(ComplianceControl {
            id: "CC7.1".to_string(),
            framework: ComplianceFramework::SOC2TypeII,
            category: ControlCategory::Logging,
            title: "Security Event Logging".to_string(),
            description: "To meet its objectives, the entity uses detection and monitoring procedures to identify security events.".to_string(),
            requirements: vec![
                "Comprehensive security event logging".to_string(),
                "Log retention for compliance periods".to_string(),
                "Automated log analysis and alerting".to_string(),
            ],
            implementation_status: ImplementationStatus::Implemented,
            evidence_required: vec![
                "Log management policy".to_string(),
                "Sample security logs".to_string(),
                "Alert configuration screenshots".to_string(),
            ],
            testing_frequency: TestingFrequency::Monthly,
            responsible_party: "Security Team".to_string(),
            last_tested: None,
            next_test_due: Some(Utc::now() + Duration::days(30)),
            control_effectiveness: ControlEffectiveness::NotTested,
            exceptions: Vec::new(),
        })?;

        // Data Protection Controls
        self.add_control(ComplianceControl {
            id: "CC6.7".to_string(),
            framework: ComplianceFramework::SOC2TypeII,
            category: ControlCategory::DataProtection,
            title: "Data Transmission and Disposal".to_string(),
            description: "The entity restricts the transmission, movement, and removal of information to authorized internal and external users and processes.".to_string(),
            requirements: vec![
                "Encryption in transit (TLS 1.3)".to_string(),
                "Encryption at rest for sensitive data".to_string(),
                "Secure data disposal procedures".to_string(),
            ],
            implementation_status: ImplementationStatus::Implemented,
            evidence_required: vec![
                "Encryption policies".to_string(),
                "TLS configuration documentation".to_string(),
                "Data disposal procedures".to_string(),
            ],
            testing_frequency: TestingFrequency::Quarterly,
            responsible_party: "Security Team".to_string(),
            last_tested: None,
            next_test_due: Some(Utc::now() + Duration::days(90)),
            control_effectiveness: ControlEffectiveness::NotTested,
            exceptions: Vec::new(),
        })?;

        // Incident Response Controls
        self.add_control(ComplianceControl {
            id: "CC7.4".to_string(),
            framework: ComplianceFramework::SOC2TypeII,
            category: ControlCategory::IncidentResponse,
            title: "Incident Response".to_string(),
            description: "The entity responds to identified security events by executing a defined incident response program.".to_string(),
            requirements: vec![
                "Documented incident response procedures".to_string(),
                "Incident response team and escalation".to_string(),
                "Incident tracking and reporting".to_string(),
            ],
            implementation_status: ImplementationStatus::InProgress,
            evidence_required: vec![
                "Incident response plan".to_string(),
                "Incident response training records".to_string(),
                "Sample incident reports".to_string(),
            ],
            testing_frequency: TestingFrequency::Annually,
            responsible_party: "Security Team".to_string(),
            last_tested: None,
            next_test_due: Some(Utc::now() + Duration::days(365)),
            control_effectiveness: ControlEffectiveness::NotTested,
            exceptions: Vec::new(),
        })?;

        Ok(())
    }

    /// Add a compliance control
    pub fn add_control(&mut self, control: ComplianceControl) -> SecurityResult<()> {
        self.controls.insert(control.id.clone(), control);
        Ok(())
    }

    /// Get control by ID
    pub fn get_control(&self, control_id: &str) -> Option<&ComplianceControl> {
        self.controls.get(control_id)
    }

    /// Update control status
    pub fn update_control_status(
        &mut self,
        control_id: &str,
        status: ImplementationStatus,
    ) -> SecurityResult<()> {
        if let Some(control) = self.controls.get_mut(control_id) {
            control.implementation_status = status;
            Ok(())
        } else {
            Err(SecurityError::ComplianceError(format!("Control {} not found", control_id)))
        }
    }

    /// Add compliance evidence
    pub fn add_evidence(&mut self, evidence: ComplianceEvidence) -> SecurityResult<()> {
        self.evidence.insert(evidence.id.clone(), evidence);
        Ok(())
    }

    /// Collect automated evidence from security events
    pub fn collect_automated_evidence(&mut self, events: &[SecurityEvent]) -> SecurityResult<()> {
        // Collect evidence for logging controls
        if !events.is_empty() {
            let evidence = ComplianceEvidence {
                id: format!("LOG_EVIDENCE_{}", Utc::now().timestamp()),
                control_id: "CC7.1".to_string(),
                evidence_type: EvidenceType::LogFile,
                title: "Security Event Logs".to_string(),
                description: format!("Automated collection of {} security events", events.len()),
                file_path: None,
                content: Some(serde_json::to_string_pretty(events)?),
                collected_by: "compliance_manager".to_string(),
                collected_at: Utc::now(),
                retention_period: Duration::days(2555), // 7 years for SOC2
                verified: false,
                verification_date: None,
                verified_by: None,
            };
            self.add_evidence(evidence)?;
        }

        // Collect evidence for authentication controls based on auth events
        let auth_events: Vec<&SecurityEvent> = events.iter()
            .filter(|e| e.event_type == SecurityEventType::Authentication)
            .collect();

        if !auth_events.is_empty() {
            let evidence = ComplianceEvidence {
                id: format!("AUTH_EVIDENCE_{}", Utc::now().timestamp()),
                control_id: "CC6.2".to_string(),
                evidence_type: EvidenceType::LogFile,
                title: "Authentication Event Logs".to_string(),
                description: format!("Authentication events showing control operation: {} events", auth_events.len()),
                file_path: None,
                content: Some(serde_json::to_string_pretty(&auth_events)?),
                collected_by: "compliance_manager".to_string(),
                collected_at: Utc::now(),
                retention_period: Duration::days(2555),
                verified: false,
                verification_date: None,
                verified_by: None,
            };
            self.add_evidence(evidence)?;
        }

        Ok(())
    }

    /// Perform compliance assessment
    pub fn perform_assessment(
        &mut self,
        framework: ComplianceFramework,
        assessor: String,
        scope: String,
    ) -> SecurityResult<ComplianceAssessment> {
        let assessment_id = format!("ASSESS_{}", Utc::now().timestamp());
        let assessment_date = Utc::now();

        let mut control_results = Vec::new();
        let mut findings = Vec::new();
        let mut compliant_count = 0;
        let mut total_controls = 0;

        // Assess each control for the framework
        for (control_id, control) in &self.controls {
            if std::mem::discriminant(&control.framework) == std::mem::discriminant(&framework) {
                total_controls += 1;
                
                let (rating, deficiencies) = self.assess_control(control)?;
                
                if rating == ComplianceRating::Compliant {
                    compliant_count += 1;
                }

                control_results.push(ControlAssessmentResult {
                    control_id: control_id.clone(),
                    rating,
                    test_method: "Automated assessment with manual review".to_string(),
                    test_results: format!("Control {} assessment completed", control_id),
                    evidence_reviewed: self.get_evidence_for_control(control_id),
                    deficiencies: deficiencies.clone(),
                    recommendations: self.generate_control_recommendations(&deficiencies),
                });

                // Create findings for deficiencies
                for deficiency in deficiencies {
                    findings.push(ComplianceFinding {
                        id: format!("FINDING_{}_{}", control_id, Utc::now().timestamp()),
                        severity: FindingSeverity::Medium,
                        category: control.category.clone(),
                        title: format!("Control {} Deficiency", control_id),
                        description: deficiency,
                        impact: "Potential compliance gap".to_string(),
                        recommendation: "Address control deficiency".to_string(),
                        remediation_timeline: "30 days".to_string(),
                        responsible_party: control.responsible_party.clone(),
                        status: FindingStatus::Open,
                        identified_date: assessment_date,
                        target_resolution_date: assessment_date + Duration::days(30),
                        actual_resolution_date: None,
                    });
                }
            }
        }

        // Determine overall rating
        let overall_rating = if compliant_count == total_controls {
            ComplianceRating::Compliant
        } else if compliant_count > total_controls / 2 {
            ComplianceRating::PartiallyCompliant
        } else {
            ComplianceRating::NonCompliant
        };

        let assessment = ComplianceAssessment {
            id: assessment_id,
            framework,
            assessment_date,
            assessor,
            scope,
            control_results,
            overall_rating,
            findings: findings.clone(),
            recommendations: self.generate_assessment_recommendations(&findings),
            next_assessment_due: assessment_date + Duration::days(365),
        };

        // Store findings
        self.findings.extend(findings);

        // Store assessment
        self.assessments.push(assessment.clone());

        Ok(assessment)
    }

    /// Assess individual control
    fn assess_control(&self, control: &ComplianceControl) -> SecurityResult<(ComplianceRating, Vec<String>)> {
        let mut deficiencies = Vec::new();

        // Check implementation status
        match control.implementation_status {
            ImplementationStatus::NotImplemented => {
                deficiencies.push("Control is not implemented".to_string());
            }
            ImplementationStatus::InProgress => {
                deficiencies.push("Control implementation is incomplete".to_string());
            }
            _ => {} // Implemented controls pass this check
        }

        // Check if evidence exists
        let evidence_count = self.evidence.values()
            .filter(|e| e.control_id == control.id)
            .count();

        if evidence_count == 0 {
            deficiencies.push("No evidence collected for control".to_string());
        }

        // Check testing frequency compliance
        if let Some(next_test_due) = control.next_test_due {
            if next_test_due < Utc::now() {
                deficiencies.push("Control testing is overdue".to_string());
            }
        }

        // Determine rating based on deficiencies
        let rating = if deficiencies.is_empty() {
            ComplianceRating::Compliant
        } else if deficiencies.len() <= 2 {
            ComplianceRating::PartiallyCompliant
        } else {
            ComplianceRating::NonCompliant
        };

        Ok((rating, deficiencies))
    }

    /// Get evidence for a control
    fn get_evidence_for_control(&self, control_id: &str) -> Vec<String> {
        self.evidence.values()
            .filter(|e| e.control_id == control_id)
            .map(|e| e.id.clone())
            .collect()
    }

    /// Generate recommendations for control deficiencies
    fn generate_control_recommendations(&self, deficiencies: &[String]) -> Vec<String> {
        let mut recommendations = Vec::new();

        for deficiency in deficiencies {
            if deficiency.contains("not implemented") {
                recommendations.push("Complete control implementation according to documented procedures".to_string());
            } else if deficiency.contains("evidence") {
                recommendations.push("Collect and document required evidence for control operation".to_string());
            } else if deficiency.contains("testing") {
                recommendations.push("Perform control testing according to defined frequency".to_string());
            } else {
                recommendations.push("Address identified control deficiency".to_string());
            }
        }

        recommendations
    }

    /// Generate assessment recommendations
    fn generate_assessment_recommendations(&self, findings: &[ComplianceFinding]) -> Vec<String> {
        let mut recommendations = Vec::new();

        let critical_findings = findings.iter().filter(|f| matches!(f.severity, FindingSeverity::Critical)).count();
        let high_findings = findings.iter().filter(|f| matches!(f.severity, FindingSeverity::High)).count();

        if critical_findings > 0 {
            recommendations.push(format!("Address {} critical findings immediately", critical_findings));
        }

        if high_findings > 0 {
            recommendations.push(format!("Prioritize resolution of {} high-severity findings", high_findings));
        }

        recommendations.push("Implement continuous monitoring for compliance controls".to_string());
        recommendations.push("Schedule regular compliance assessments".to_string());
        recommendations.push("Enhance evidence collection automation".to_string());

        recommendations
    }

    /// Get compliance dashboard metrics
    pub fn get_compliance_metrics(&self) -> ComplianceMetrics {
        let total_controls = self.controls.len();
        let implemented_controls = self.controls.values()
            .filter(|c| matches!(c.implementation_status, ImplementationStatus::Implemented | ImplementationStatus::Operating | ImplementationStatus::Effective))
            .count();

        let open_findings = self.findings.iter()
            .filter(|f| matches!(f.status, FindingStatus::Open | FindingStatus::InProgress))
            .count();

        let overdue_tests = self.controls.values()
            .filter(|c| c.next_test_due.map_or(false, |due| due < Utc::now()))
            .count();

        ComplianceMetrics {
            total_controls,
            implemented_controls,
            implementation_percentage: if total_controls > 0 {
                (implemented_controls as f64 / total_controls as f64) * 100.0
            } else {
                0.0
            },
            open_findings,
            overdue_tests,
            last_assessment_date: self.assessments.last().map(|a| a.assessment_date),
            next_assessment_due: self.assessments.last().map(|a| a.next_assessment_due),
        }
    }

    /// Get controls by framework
    pub fn get_controls_by_framework(&self, framework: &ComplianceFramework) -> Vec<&ComplianceControl> {
        self.controls.values()
            .filter(|c| std::mem::discriminant(&c.framework) == std::mem::discriminant(framework))
            .collect()
    }

    /// Get open findings
    pub fn get_open_findings(&self) -> Vec<&ComplianceFinding> {
        self.findings.iter()
            .filter(|f| matches!(f.status, FindingStatus::Open | FindingStatus::InProgress))
            .collect()
    }

    /// Resolve finding
    pub fn resolve_finding(&mut self, finding_id: &str) -> SecurityResult<()> {
        if let Some(finding) = self.findings.iter_mut().find(|f| f.id == finding_id) {
            finding.status = FindingStatus::Resolved;
            finding.actual_resolution_date = Some(Utc::now());
            Ok(())
        } else {
            Err(SecurityError::ComplianceError(format!("Finding {} not found", finding_id)))
        }
    }

    /// Generate compliance report
    pub fn generate_compliance_report(&self, framework: ComplianceFramework) -> ComplianceReport {
        let controls = self.get_controls_by_framework(&framework);
        let latest_assessment = self.assessments.iter()
            .filter(|a| std::mem::discriminant(&a.framework) == std::mem::discriminant(&framework))
            .last();

        ComplianceReport {
            framework,
            report_date: Utc::now(),
            controls: controls.into_iter().cloned().collect(),
            latest_assessment: latest_assessment.cloned(),
            metrics: self.get_compliance_metrics(),
            summary: "Compliance report generated by QuicCrunch Security Framework".to_string(),
        }
    }
}

/// Compliance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceMetrics {
    pub total_controls: usize,
    pub implemented_controls: usize,
    pub implementation_percentage: f64,
    pub open_findings: usize,
    pub overdue_tests: usize,
    pub last_assessment_date: Option<DateTime<Utc>>,
    pub next_assessment_due: Option<DateTime<Utc>>,
}

/// Compliance report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceReport {
    pub framework: ComplianceFramework,
    pub report_date: DateTime<Utc>,
    pub controls: Vec<ComplianceControl>,
    pub latest_assessment: Option<ComplianceAssessment>,
    pub metrics: ComplianceMetrics,
    pub summary: String,
}