//! Configuration type definitions for Mol-HEP-Lab.
//!
//! All structs are direct ports of the Python dataclasses in
//! `backend/agent/researchclaw/config.py` with brand renames applied:
//! researchclaw → researchmol, metaclaw → metamol, config.arc.yaml → config.mol.yaml.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Automation level for the project workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectMode {
    /// Documentation-first, highest human oversight.
    #[default]
    DocsFirst,
    /// Semi-automated with approval gates.
    SemiAuto,
    /// Fully automated end-to-end.
    FullAuto,
}

/// Knowledge-base storage backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum KbBackend {
    #[default]
    Markdown,
    Obsidian,
}

/// Experiment execution environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentMode {
    #[default]
    Simulated,
    Sandbox,
    Docker,
    SshRemote,
    ColabDrive,
}

/// Direction for the primary optimization metric.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MetricDirection {
    #[default]
    Minimize,
    Maximize,
}

// ---------------------------------------------------------------------------
// Top-level config
// ---------------------------------------------------------------------------

/// Top-level Mol-HEP-Lab configuration (was `RCConfig` in Python).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MolConfig {
    pub project: ProjectConfig,
    pub research: ResearchConfig,
    pub runtime: RuntimeConfig,
    pub notifications: NotificationsConfig,
    pub knowledge_base: KnowledgeBaseConfig,
    #[serde(default, rename = "openmol_bridge")]
    pub openmol_bridge: OpenMolBridgeConfig,
    pub llm: LlmConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub experiment: ExperimentConfig,
    #[serde(default)]
    pub export: ExportConfig,
    #[serde(default)]
    pub prompts: PromptsConfig,
    #[serde(default)]
    pub web_search: WebSearchConfig,
    #[serde(default, rename = "metamol_bridge")]
    pub metamol_bridge: MetaMolBridgeConfig,
}

// ---------------------------------------------------------------------------
// Sub-configs
// ---------------------------------------------------------------------------

/// Project identity and automation level.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectConfig {
    pub name: String,
    #[serde(default)]
    pub mode: ProjectMode,
}

/// Research topic and paper-sourcing parameters.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResearchConfig {
    pub topic: String,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub daily_paper_count: u32,
    #[serde(default)]
    pub quality_threshold: f64,
    #[serde(default = "defaults::bool_true")]
    pub graceful_degradation: bool,
    #[serde(default)]
    pub reference_papers: Vec<String>,
}

/// Runtime scheduling and parallelism knobs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeConfig {
    pub timezone: String,
    #[serde(default = "defaults::one_u32")]
    pub max_parallel_tasks: u32,
    #[serde(default = "defaults::twelve_u32")]
    pub approval_timeout_hours: u32,
    #[serde(default)]
    pub retry_limit: u32,
}

/// Notification channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotificationsConfig {
    pub channel: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub on_stage_start: bool,
    #[serde(default)]
    pub on_stage_fail: bool,
    #[serde(default = "defaults::bool_true")]
    pub on_gate_required: bool,
}

/// Knowledge base storage location and backend.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KnowledgeBaseConfig {
    #[serde(default)]
    pub backend: KbBackend,
    pub root: String,
    #[serde(default)]
    pub obsidian_vault: String,
}

/// OpenMol bridge integration flags (was `OpenClawBridgeConfig`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpenMolBridgeConfig {
    #[serde(default)]
    pub use_cron: bool,
    #[serde(default)]
    pub use_message: bool,
    #[serde(default)]
    pub use_memory: bool,
    #[serde(default)]
    pub use_sessions_spawn: bool,
    #[serde(default)]
    pub use_web_fetch: bool,
    #[serde(default)]
    pub use_browser: bool,
}

/// ACP (Agent Client Protocol) settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpConfig {
    #[serde(default = "defaults::acp_agent")]
    pub agent: String,
    #[serde(default = "defaults::dot")]
    pub cwd: String,
    #[serde(default)]
    pub acpx_command: String,
    #[serde(default = "defaults::session_name")]
    pub session_name: String,
    #[serde(default = "defaults::six_hundred_u32")]
    pub timeout_sec: u32,
}

impl Default for AcpConfig {
    fn default() -> Self {
        Self {
            agent: defaults::acp_agent(),
            cwd: defaults::dot(),
            acpx_command: String::new(),
            session_name: defaults::session_name(),
            timeout_sec: defaults::six_hundred_u32(),
        }
    }
}

/// LLM provider and model configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmConfig {
    pub provider: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key_env: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub primary_model: String,
    #[serde(default)]
    pub coding_model: String,
    #[serde(default)]
    pub image_model: String,
    #[serde(default)]
    pub fallback_models: Vec<String>,
    #[serde(default)]
    pub s2_api_key: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub timeout_sec: u32,
    #[serde(default)]
    pub max_retries: u32,
    #[serde(default)]
    pub acp: AcpConfig,
}

/// Human-in-the-loop and log-redaction security settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default = "defaults::hitl_stages")]
    pub hitl_required_stages: Vec<u8>,
    #[serde(default)]
    pub allow_publish_without_approval: bool,
    #[serde(default = "defaults::bool_true")]
    pub redact_sensitive_logs: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            hitl_required_stages: defaults::hitl_stages(),
            allow_publish_without_approval: false,
            redact_sensitive_logs: true,
        }
    }
}

/// In-process Python sandbox configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    #[serde(default = "defaults::python_path")]
    pub python_path: String,
    #[serde(default)]
    pub gpu_required: bool,
    #[serde(default = "defaults::network_full")]
    pub network_policy: String,
    #[serde(default = "defaults::allowed_imports")]
    pub allowed_imports: Vec<String>,
    #[serde(default = "defaults::four_k_u32")]
    pub max_memory_mb: u32,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            python_path: defaults::python_path(),
            gpu_required: false,
            network_policy: defaults::network_full(),
            allowed_imports: defaults::allowed_imports(),
            max_memory_mb: defaults::four_k_u32(),
        }
    }
}

/// SSH remote execution configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshRemoteConfig {
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub user: String,
    #[serde(default = "defaults::ssh_port")]
    pub port: u16,
    #[serde(default)]
    pub key_path: String,
    #[serde(default)]
    pub gpu_ids: Vec<u32>,
    #[serde(default = "defaults::auto_str")]
    pub accelerator_type: String,
    #[serde(default = "defaults::remote_workdir")]
    pub remote_workdir: String,
    #[serde(default = "defaults::python3")]
    pub remote_python: String,
    #[serde(default)]
    pub setup_commands: Vec<String>,
    #[serde(default)]
    pub use_docker: bool,
    #[serde(default = "defaults::researchmol_experiment_image")]
    pub docker_image: String,
    #[serde(default = "defaults::none_str")]
    pub docker_network_policy: String,
    #[serde(default = "defaults::eight_k_u32")]
    pub docker_memory_limit_mb: u32,
    #[serde(default = "defaults::two_k_u32")]
    pub docker_shm_size_mb: u32,
    #[serde(default = "defaults::six_hundred_u32")]
    pub timeout_sec: u32,
    #[serde(default = "defaults::three_hundred_u32")]
    pub scp_timeout_sec: u32,
    #[serde(default = "defaults::three_hundred_u32")]
    pub setup_timeout_sec: u32,
}

impl Default for SshRemoteConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            user: String::new(),
            port: defaults::ssh_port(),
            key_path: String::new(),
            gpu_ids: Vec::new(),
            accelerator_type: defaults::auto_str(),
            remote_workdir: defaults::remote_workdir(),
            remote_python: defaults::python3(),
            setup_commands: Vec::new(),
            use_docker: false,
            docker_image: defaults::researchmol_experiment_image(),
            docker_network_policy: defaults::none_str(),
            docker_memory_limit_mb: defaults::eight_k_u32(),
            docker_shm_size_mb: defaults::two_k_u32(),
            timeout_sec: defaults::six_hundred_u32(),
            scp_timeout_sec: defaults::three_hundred_u32(),
            setup_timeout_sec: defaults::three_hundred_u32(),
        }
    }
}

/// Google Colab / Drive-based async execution configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColabDriveConfig {
    #[serde(default)]
    pub drive_root: String,
    #[serde(default = "defaults::thirty_u32")]
    pub poll_interval_sec: u32,
    #[serde(default = "defaults::thirty_six_hundred_u32")]
    pub timeout_sec: u32,
    #[serde(default)]
    pub setup_script: String,
}

impl Default for ColabDriveConfig {
    fn default() -> Self {
        Self {
            drive_root: String::new(),
            poll_interval_sec: defaults::thirty_u32(),
            timeout_sec: defaults::thirty_six_hundred_u32(),
            setup_script: String::new(),
        }
    }
}

/// Docker-based experiment sandbox configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerSandboxConfig {
    #[serde(default = "defaults::researchmol_experiment_image")]
    pub image: String,
    #[serde(default = "defaults::bool_true")]
    pub gpu_enabled: bool,
    #[serde(default)]
    pub gpu_device_ids: Vec<u32>,
    #[serde(default = "defaults::auto_str")]
    pub accelerator_type: String,
    #[serde(default = "defaults::eight_k_u32")]
    pub memory_limit_mb: u32,
    #[serde(default = "defaults::setup_only")]
    pub network_policy: String,
    #[serde(default)]
    pub pip_pre_install: Vec<String>,
    #[serde(default = "defaults::bool_true")]
    pub auto_install_deps: bool,
    #[serde(default = "defaults::two_k_u32")]
    pub shm_size_mb: u32,
    #[serde(default = "defaults::usr_bin_python3")]
    pub container_python: String,
    #[serde(default)]
    pub keep_containers: bool,
}

impl Default for DockerSandboxConfig {
    fn default() -> Self {
        Self {
            image: defaults::researchmol_experiment_image(),
            gpu_enabled: true,
            gpu_device_ids: Vec::new(),
            accelerator_type: defaults::auto_str(),
            memory_limit_mb: defaults::eight_k_u32(),
            network_policy: defaults::setup_only(),
            pip_pre_install: Vec::new(),
            auto_install_deps: true,
            shm_size_mb: defaults::two_k_u32(),
            container_python: defaults::usr_bin_python3(),
            keep_containers: false,
        }
    }
}

/// Multi-phase code generation agent configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeAgentConfig {
    #[serde(default = "defaults::bool_true")]
    pub enabled: bool,
    #[serde(default = "defaults::bool_true")]
    pub architecture_planning: bool,
    #[serde(default = "defaults::bool_true")]
    pub sequential_generation: bool,
    #[serde(default = "defaults::bool_true")]
    pub hard_validation: bool,
    #[serde(default = "defaults::two_u32")]
    pub hard_validation_max_repairs: u32,
    #[serde(default = "defaults::three_u32")]
    pub exec_fix_max_iterations: u32,
    #[serde(default = "defaults::sixty_u32")]
    pub exec_fix_timeout_sec: u32,
    #[serde(default)]
    pub tree_search_enabled: bool,
    #[serde(default = "defaults::three_u32")]
    pub tree_search_candidates: u32,
    #[serde(default = "defaults::two_u32")]
    pub tree_search_max_depth: u32,
    #[serde(default = "defaults::one_twenty_u32")]
    pub tree_search_eval_timeout_sec: u32,
    #[serde(default = "defaults::two_u32")]
    pub review_max_rounds: u32,
}

impl Default for CodeAgentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            architecture_planning: true,
            sequential_generation: true,
            hard_validation: true,
            hard_validation_max_repairs: defaults::two_u32(),
            exec_fix_max_iterations: defaults::three_u32(),
            exec_fix_timeout_sec: defaults::sixty_u32(),
            tree_search_enabled: false,
            tree_search_candidates: defaults::three_u32(),
            tree_search_max_depth: defaults::two_u32(),
            tree_search_eval_timeout_sec: defaults::one_twenty_u32(),
            review_max_rounds: defaults::two_u32(),
        }
    }
}

/// Beast-mode external AI coding agent (OpenCode/Aider) configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeConfig {
    #[serde(default = "defaults::bool_true")]
    pub enabled: bool,
    #[serde(default = "defaults::bool_true")]
    pub auto: bool,
    #[serde(default = "defaults::complexity_threshold")]
    pub complexity_threshold: f64,
    #[serde(default)]
    pub model: String,
    #[serde(default = "defaults::six_hundred_u32")]
    pub timeout_sec: u32,
    #[serde(default = "defaults::one_u32")]
    pub max_retries: u32,
    #[serde(default = "defaults::bool_true")]
    pub workspace_cleanup: bool,
}

impl Default for OpenCodeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto: true,
            complexity_threshold: defaults::complexity_threshold(),
            model: String::new(),
            timeout_sec: defaults::six_hundred_u32(),
            max_retries: defaults::one_u32(),
            workspace_cleanup: true,
        }
    }
}

/// BenchmarkAgent multi-agent system configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkAgentConfig {
    #[serde(default = "defaults::bool_true")]
    pub enabled: bool,
    // Surveyor
    #[serde(default = "defaults::bool_true")]
    pub enable_hf_search: bool,
    #[serde(default = "defaults::ten_u32")]
    pub max_hf_results: u32,
    #[serde(default = "defaults::bool_true")]
    pub enable_web_search: bool,
    #[serde(default = "defaults::five_u32")]
    pub max_web_results: u32,
    #[serde(default = "defaults::three_u32")]
    pub web_search_min_local: u32,
    // Selector
    #[serde(default = "defaults::two_u32")]
    pub tier_limit: u32,
    #[serde(default = "defaults::one_u32")]
    pub min_benchmarks: u32,
    #[serde(default = "defaults::two_u32")]
    pub min_baselines: u32,
    #[serde(default = "defaults::bool_true")]
    pub prefer_cached: bool,
    // Orchestrator
    #[serde(default = "defaults::two_u32")]
    pub max_iterations: u32,
}

impl Default for BenchmarkAgentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            enable_hf_search: true,
            max_hf_results: defaults::ten_u32(),
            enable_web_search: true,
            max_web_results: defaults::five_u32(),
            web_search_min_local: defaults::three_u32(),
            tier_limit: defaults::two_u32(),
            min_benchmarks: defaults::one_u32(),
            min_baselines: defaults::two_u32(),
            prefer_cached: true,
            max_iterations: defaults::two_u32(),
        }
    }
}

/// FigureAgent multi-agent system configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FigureAgentConfig {
    #[serde(default = "defaults::bool_true")]
    pub enabled: bool,
    // Planner
    #[serde(default = "defaults::three_u32")]
    pub min_figures: u32,
    #[serde(default = "defaults::eight_u32")]
    pub max_figures: u32,
    // Orchestrator
    #[serde(default = "defaults::three_u32")]
    pub max_iterations: u32,
    // Renderer security
    #[serde(default = "defaults::thirty_u32")]
    pub render_timeout_sec: u32,
    /// `None` = auto-detect; `Some(true/false)` = force.
    #[serde(default)]
    pub use_docker: Option<bool>,
    #[serde(default = "defaults::researchmol_experiment_image")]
    pub docker_image: String,
    // Code generation output format
    #[serde(default = "defaults::python_str")]
    pub output_format: String,
    // Gemini / Nano Banana image generation
    #[serde(default)]
    pub gemini_api_key: String,
    #[serde(default = "defaults::gemini_model")]
    pub gemini_model: String,
    #[serde(default = "defaults::bool_true")]
    pub nano_banana_enabled: bool,
    // Critic
    #[serde(default)]
    pub strict_mode: bool,
    // Output
    #[serde(default = "defaults::three_hundred_u32")]
    pub dpi: u32,
}

impl Default for FigureAgentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_figures: defaults::three_u32(),
            max_figures: defaults::eight_u32(),
            max_iterations: defaults::three_u32(),
            render_timeout_sec: defaults::thirty_u32(),
            use_docker: None,
            docker_image: defaults::researchmol_experiment_image(),
            output_format: defaults::python_str(),
            gemini_api_key: String::new(),
            gemini_model: defaults::gemini_model(),
            nano_banana_enabled: true,
            strict_mode: false,
            dpi: defaults::three_hundred_u32(),
        }
    }
}

/// Experiment runner configuration including nested sub-configs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentConfig {
    #[serde(default)]
    pub mode: ExperimentMode,
    #[serde(default = "defaults::thirty_six_hundred_u32")]
    pub time_budget_sec: u32,
    #[serde(default = "defaults::ten_u32")]
    pub max_iterations: u32,
    #[serde(default)]
    pub max_refine_duration_sec: u32,
    #[serde(default = "defaults::primary_metric")]
    pub metric_key: String,
    #[serde(default)]
    pub metric_direction: MetricDirection,
    #[serde(default)]
    pub keep_threshold: f64,
    #[serde(default)]
    pub datasets_dir: String,
    #[serde(default)]
    pub checkpoints_dir: String,
    #[serde(default)]
    pub codebases_dir: String,
    #[serde(default)]
    pub shared_results_dir: String,
    #[serde(default = "defaults::medium_str")]
    pub paper_length: String,
    #[serde(default)]
    pub sandbox: SandboxConfig,
    #[serde(default)]
    pub docker: DockerSandboxConfig,
    #[serde(default)]
    pub ssh_remote: SshRemoteConfig,
    #[serde(default)]
    pub colab_drive: ColabDriveConfig,
    #[serde(default)]
    pub code_agent: CodeAgentConfig,
    #[serde(default)]
    pub opencode: OpenCodeConfig,
    #[serde(default)]
    pub benchmark_agent: BenchmarkAgentConfig,
    #[serde(default)]
    pub figure_agent: FigureAgentConfig,
    #[serde(default = "defaults::three_u32")]
    pub sanity_check_max_iterations: u32,
}

impl Default for ExperimentConfig {
    fn default() -> Self {
        Self {
            mode: ExperimentMode::default(),
            time_budget_sec: defaults::thirty_six_hundred_u32(),
            max_iterations: defaults::ten_u32(),
            max_refine_duration_sec: 0,
            metric_key: defaults::primary_metric(),
            metric_direction: MetricDirection::default(),
            keep_threshold: 0.0,
            datasets_dir: String::new(),
            checkpoints_dir: String::new(),
            codebases_dir: String::new(),
            shared_results_dir: String::new(),
            paper_length: defaults::medium_str(),
            sandbox: SandboxConfig::default(),
            docker: DockerSandboxConfig::default(),
            ssh_remote: SshRemoteConfig::default(),
            colab_drive: ColabDriveConfig::default(),
            code_agent: CodeAgentConfig::default(),
            opencode: OpenCodeConfig::default(),
            benchmark_agent: BenchmarkAgentConfig::default(),
            figure_agent: FigureAgentConfig::default(),
            sanity_check_max_iterations: defaults::three_u32(),
        }
    }
}

/// PRM quality-gate settings for MetaMol bridge (was `MetaClawPRMConfig`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaMolPRMConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub api_base: String,
    #[serde(default)]
    pub api_key_env: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "defaults::prm_model")]
    pub model: String,
    #[serde(default = "defaults::three_u32")]
    pub votes: u32,
    #[serde(default = "defaults::prm_temperature")]
    pub temperature: f64,
    #[serde(default = "defaults::gate_stages")]
    pub gate_stages: Vec<u8>,
}

impl Default for MetaMolPRMConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_base: String::new(),
            api_key_env: String::new(),
            api_key: String::new(),
            model: defaults::prm_model(),
            votes: defaults::three_u32(),
            temperature: defaults::prm_temperature(),
            gate_stages: defaults::gate_stages(),
        }
    }
}

/// Lesson-to-skill conversion settings for MetaMol (was `MetaClawLessonToSkillConfig`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaMolLessonToSkillConfig {
    #[serde(default = "defaults::bool_true")]
    pub enabled: bool,
    #[serde(default = "defaults::warning_str")]
    pub min_severity: String,
    #[serde(default = "defaults::three_u32")]
    pub max_skills_per_run: u32,
}

impl Default for MetaMolLessonToSkillConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_severity: defaults::warning_str(),
            max_skills_per_run: defaults::three_u32(),
        }
    }
}

/// MetaMol integration bridge configuration (was `MetaClawBridgeConfig`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaMolBridgeConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "defaults::metamol_proxy_url")]
    pub proxy_url: String,
    #[serde(default = "defaults::metamol_skills_dir")]
    pub skills_dir: String,
    #[serde(default)]
    pub fallback_url: String,
    #[serde(default)]
    pub fallback_api_key: String,
    #[serde(default)]
    pub prm: MetaMolPRMConfig,
    #[serde(default)]
    pub lesson_to_skill: MetaMolLessonToSkillConfig,
}

impl Default for MetaMolBridgeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            proxy_url: defaults::metamol_proxy_url(),
            skills_dir: defaults::metamol_skills_dir(),
            fallback_url: String::new(),
            fallback_api_key: String::new(),
            prm: MetaMolPRMConfig::default(),
            lesson_to_skill: MetaMolLessonToSkillConfig::default(),
        }
    }
}

/// Web search and crawling configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchConfig {
    #[serde(default = "defaults::bool_true")]
    pub enabled: bool,
    #[serde(default)]
    pub tavily_api_key: String,
    #[serde(default = "defaults::tavily_api_key_env")]
    pub tavily_api_key_env: String,
    #[serde(default = "defaults::bool_true")]
    pub enable_scholar: bool,
    #[serde(default = "defaults::bool_true")]
    pub enable_crawling: bool,
    #[serde(default = "defaults::bool_true")]
    pub enable_pdf_extraction: bool,
    #[serde(default = "defaults::ten_u32")]
    pub max_web_results: u32,
    #[serde(default = "defaults::ten_u32")]
    pub max_scholar_results: u32,
    #[serde(default = "defaults::five_u32")]
    pub max_crawl_urls: u32,
}

impl Default for WebSearchConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            tavily_api_key: String::new(),
            tavily_api_key_env: defaults::tavily_api_key_env(),
            enable_scholar: true,
            enable_crawling: true,
            enable_pdf_extraction: true,
            max_web_results: defaults::ten_u32(),
            max_scholar_results: defaults::ten_u32(),
            max_crawl_urls: defaults::five_u32(),
        }
    }
}

/// Paper export and LaTeX generation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportConfig {
    #[serde(default = "defaults::target_conference")]
    pub target_conference: String,
    #[serde(default = "defaults::authors")]
    pub authors: String,
    #[serde(default = "defaults::bib_file")]
    pub bib_file: String,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            target_conference: defaults::target_conference(),
            authors: defaults::authors(),
            bib_file: defaults::bib_file(),
        }
    }
}

/// Prompt externalization configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PromptsConfig {
    /// Path to a custom prompts YAML file; empty = use built-in defaults.
    #[serde(default)]
    pub custom_file: String,
}

// ---------------------------------------------------------------------------
// Default-value helpers (used by #[serde(default = "...")])
// ---------------------------------------------------------------------------

mod defaults {
    pub fn bool_true() -> bool {
        true
    }
    pub fn one_u32() -> u32 {
        1
    }
    pub fn two_u32() -> u32 {
        2
    }
    pub fn three_u32() -> u32 {
        3
    }
    pub fn five_u32() -> u32 {
        5
    }
    pub fn eight_u32() -> u32 {
        8
    }
    pub fn ten_u32() -> u32 {
        10
    }
    pub fn twelve_u32() -> u32 {
        12
    }
    pub fn thirty_u32() -> u32 {
        30
    }
    pub fn sixty_u32() -> u32 {
        60
    }
    pub fn one_twenty_u32() -> u32 {
        120
    }
    pub fn three_hundred_u32() -> u32 {
        300
    }
    pub fn six_hundred_u32() -> u32 {
        600
    }
    pub fn thirty_six_hundred_u32() -> u32 {
        3600
    }
    pub fn two_k_u32() -> u32 {
        2048
    }
    pub fn four_k_u32() -> u32 {
        4096
    }
    pub fn eight_k_u32() -> u32 {
        8192
    }

    pub fn hitl_stages() -> Vec<u8> {
        vec![5, 9, 20]
    }
    pub fn gate_stages() -> Vec<u8> {
        vec![5, 9, 15, 20]
    }

    pub fn python_path() -> String {
        ".venv/bin/python3".to_owned()
    }
    pub fn network_full() -> String {
        "full".to_owned()
    }
    pub fn allowed_imports() -> Vec<String> {
        ["math", "random", "json", "csv", "numpy", "torch", "sklearn"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    pub fn ssh_port() -> u16 {
        22
    }
    pub fn auto_str() -> String {
        "auto".to_owned()
    }
    pub fn none_str() -> String {
        "none".to_owned()
    }
    pub fn remote_workdir() -> String {
        "/tmp/researchmol_experiments".to_owned()
    }
    pub fn python3() -> String {
        "python3".to_owned()
    }
    pub fn researchmol_experiment_image() -> String {
        "researchmol/experiment:latest".to_owned()
    }
    pub fn setup_only() -> String {
        "setup_only".to_owned()
    }
    pub fn usr_bin_python3() -> String {
        "/usr/bin/python3".to_owned()
    }

    pub fn acp_agent() -> String {
        "claude".to_owned()
    }
    pub fn dot() -> String {
        ".".to_owned()
    }
    pub fn session_name() -> String {
        "researchmol".to_owned()
    }

    pub fn complexity_threshold() -> f64 {
        0.2
    }
    pub fn prm_model() -> String {
        "gpt-5.4".to_owned()
    }
    pub fn prm_temperature() -> f64 {
        0.6
    }
    pub fn metamol_proxy_url() -> String {
        "http://localhost:30000".to_owned()
    }
    pub fn metamol_skills_dir() -> String {
        "~/.metamol/skills".to_owned()
    }
    pub fn warning_str() -> String {
        "warning".to_owned()
    }
    pub fn tavily_api_key_env() -> String {
        "TAVILY_API_KEY".to_owned()
    }
    pub fn target_conference() -> String {
        "neurips_2025".to_owned()
    }
    pub fn authors() -> String {
        "Anonymous".to_owned()
    }
    pub fn bib_file() -> String {
        "references".to_owned()
    }
    pub fn primary_metric() -> String {
        "primary_metric".to_owned()
    }
    pub fn medium_str() -> String {
        "medium".to_owned()
    }
    pub fn python_str() -> String {
        "python".to_owned()
    }
    pub fn gemini_model() -> String {
        "gemini-3-pro-image-preview".to_owned()
    }
}
