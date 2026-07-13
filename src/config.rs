use log::{info, warn};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    #[serde(default = "default_api_key")]
    pub api_key: String,
    #[serde(default = "default_api_url")]
    pub api_url: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_top_p")]
    pub top_p: f32,
    #[serde(default = "default_user_name")]
    pub user_name: String,
    #[serde(default = "default_ai_name")]
    pub ai_name: String,
    #[serde(default)]
    pub persona: String,
    #[serde(default)]
    pub tts_api_url: String,
    #[serde(default)]
    pub tts_ref_audio: String,
    #[serde(default)]
    pub tts_prompt_text: String,
    #[serde(default)]
    pub tts_gpt_weights: String,
    #[serde(default)]
    pub tts_sovits_weights: String,
    #[serde(default)]
    pub tts_speed: f32,
    #[serde(default = "default_tts_sample_steps")]
    pub tts_sample_steps: u32,
    #[serde(default)]
    pub tts_auto_play: bool,
    #[serde(default)]
    pub qq_app_id: String,
    #[serde(default)]
    pub qq_app_secret: String,
}

fn default_api_key() -> String {
    std::env::var("DEEPSEEK_API_KEY").unwrap_or_default()
}
fn default_api_url() -> String { "https://api.deepseek.com/v1/chat/completions".into() }
fn default_model() -> String { "deepseek-chat".into() }
fn default_max_tokens() -> u32 { 4096 }
fn default_temperature() -> f32 { 1.0 }
fn default_top_p() -> f32 { 1.0 }
fn default_user_name() -> String { "我".into() }
fn default_ai_name() -> String { "AI".into() }
fn default_tts_sample_steps() -> u32 { 32 }

impl Default for Config {
    fn default() -> Self {
        Self {
            api_key: default_api_key(),
            api_url: default_api_url(),
            model: default_model(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            top_p: default_top_p(),
            user_name: default_user_name(),
            ai_name: default_ai_name(),
            persona: String::new(),
            tts_api_url: "http://127.0.0.1:9880".into(),
            tts_ref_audio: String::new(),
            tts_prompt_text: String::new(),
            tts_gpt_weights: String::new(),
            tts_sovits_weights: String::new(),
            tts_speed: 1.0,
            tts_sample_steps: default_tts_sample_steps(),
            tts_auto_play: false,
            qq_app_id: String::new(),
            qq_app_secret: String::new(),
        }
    }
}

impl Config {
    pub fn load(path: &str) -> Self {
        match std::fs::read_to_string(path) {
            Ok(data) => {
                match serde_json::from_str::<Config>(&data) {
                    Ok(cfg) => {
                        info!("配置已加载: {}", path);
                        cfg
                    }
                    Err(e) => {
                        warn!("解析配置失败: {}，使用默认值", e);
                        let cfg = Config::default();
                        let _ = cfg.save(path);
                        cfg
                    }
                }
            }
            Err(_) => {
                warn!("未找到配置文件 {}，创建默认配置", path);
                let cfg = Config::default();
                let _ = cfg.save(path);
                cfg
            }
        }
    }

    pub fn save(&self, path: &str) -> Result<(), String> {
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| format!("序列化失败: {}", e))?;
        std::fs::write(path, json).map_err(|e| format!("保存失败: {}", e))?;
        info!("配置已保存: {}", path);
        Ok(())
    }
}
