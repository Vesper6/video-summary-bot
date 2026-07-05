//! 应用配置加载。
//!
//! 支持多层配置：
//! 1. 默认值
//! 2. 配置文件（toml / yaml / json）
//! 3. 环境变量
//! 4. CLI 参数

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// 应用配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// VM 默认配置
    #[serde(default)]
    pub vm: VmDefaults,

    /// LLM 配置
    #[serde(default)]
    pub llm: LlmConfig,

    /// 输出配置
    #[serde(default)]
    pub output: OutputConfig,
}

/// VM 默认配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmDefaults {
    /// 默认 vCPU 数
    pub cpus: u8,
    /// 默认内存（MB）
    pub memory_mb: u32,
    /// 默认磁盘（GB）
    pub disk_gb: u32,
    /// 默认内核路径
    pub kernel: Option<PathBuf>,
    /// 默认 initramfs 路径
    pub initramfs: Option<PathBuf>,
    /// 默认 rootfs 路径
    pub rootfs: Option<PathBuf>,
}

impl Default for VmDefaults {
    fn default() -> Self {
        Self {
            cpus: 2,
            memory_mb: 2048,
            disk_gb: 10,
            kernel: None,
            initramfs: None,
            rootfs: None,
        }
    }
}

/// LLM 配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-5".to_string(),
            max_tokens: 4096,
            temperature: 0.3,
        }
    }
}

/// 输出配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    pub dir: PathBuf,
    pub save_raw_data: bool,
    pub generate_timeline_json: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            dir: PathBuf::from("./output"),
            save_raw_data: true,
            generate_timeline_json: true,
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            vm: VmDefaults::default(),
            llm: LlmConfig::default(),
            output: OutputConfig::default(),
        }
    }
}

impl AppConfig {
    /// 加载配置。
    pub fn load() -> Result<Self> {
        let settings = config::Config::builder()
            .add_source(config::Config::try_from(&AppConfig::default()).map_err(|e| {
                Error::Config(format!("default config: {e}"))
            })?)
            // 加载配置文件（如存在）
            .add_source(
                config::File::with_name("config.toml")
                    .required(false),
            )
            .add_source(
                config::File::with_name("config.yaml")
                    .required(false),
            )
            // 环境变量覆盖（前缀 VSB_）
            .add_source(
                config::Environment::with_prefix("VSB")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()
            .map_err(|e| Error::Config(format!("build config: {e}")))?;

        settings
            .try_deserialize()
            .map_err(|e| Error::Config(format!("deserialize: {e}")))
    }
}