use reqwest::Client;
use rodio::Source;
use std::io::Cursor;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct TtsConfig {
    pub enabled: bool,
    pub api_url: String,
    pub gpt_weights: String,
    pub sovits_weights: String,
    pub ref_audio_path: String,
    pub prompt_text: String,
    pub prompt_lang: String,
    pub text_lang: String,
    pub speed: f32,
    pub sample_steps: u32,
    pub auto_play: bool,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_url: "http://127.0.0.1:9880".into(),
            gpt_weights: String::new(),
            sovits_weights: String::new(),
            ref_audio_path: String::new(),
            prompt_text: String::new(),
            prompt_lang: "zh".into(),
            text_lang: "zh".into(),
            speed: 1.0,
            sample_steps: 32,
            auto_play: false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct TtsState {
    pub connected: bool,
    pub api_url: String,
    pub gpt_weights: String,
    pub sovits_weights: String,
    pub ref_audio_path: String,
    pub prompt_text: String,
    pub speed: f32,
    pub sample_steps: u32,
    pub auto_play: bool,
    pub prompt_lang: String,
    pub text_lang: String,
}

impl TtsState {
    pub fn from_config(config: &crate::config::Config) -> Self {
        Self {
            connected: false,
            api_url: config.tts_api_url.clone(),
            gpt_weights: config.tts_gpt_weights.clone(),
            sovits_weights: config.tts_sovits_weights.clone(),
            ref_audio_path: config.tts_ref_audio.clone(),
            prompt_text: config.tts_prompt_text.clone(),
            speed: config.tts_speed,
            sample_steps: config.tts_sample_steps,
            auto_play: config.tts_auto_play,
            ..Default::default()
        }
    }

    pub fn build_config(&self) -> TtsConfig {
        TtsConfig {
            enabled: self.connected,
            api_url: self.api_url.clone(),
            gpt_weights: self.gpt_weights.clone(),
            sovits_weights: self.sovits_weights.clone(),
            ref_audio_path: self.ref_audio_path.clone(),
            prompt_text: self.prompt_text.clone(),
            prompt_lang: self.prompt_lang.clone(),
            text_lang: self.text_lang.clone(),
            speed: self.speed,
            sample_steps: self.sample_steps,
            auto_play: self.auto_play,
        }
    }
}

pub async fn connect(config: &TtsConfig) -> Result<String, String> {
    let base = config.api_url.trim_end_matches('/');
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("创建客户端失败: {}", e))?;

    let text_lang = if config.text_lang.is_empty() { "zh" } else { &config.text_lang };
    let prompt_lang = if config.prompt_lang.is_empty() { "zh" } else { &config.prompt_lang };
    let ref_audio = config.ref_audio_path.trim_matches('"');
    let prompt_text = config.prompt_text.trim_matches('"');

    let mut url = reqwest::Url::parse(&format!("{}/tts", base))
        .map_err(|e| format!("URL 解析失败: {}", e))?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("text", "测试");
        pairs.append_pair("text_lang", text_lang);
        pairs.append_pair("ref_audio_path", ref_audio);
        pairs.append_pair("prompt_text", prompt_text);
        pairs.append_pair("prompt_lang", prompt_lang);
        pairs.append_pair("media_type", "wav");
        pairs.append_pair("streaming_mode", "false");
    }

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("无法连接到 TTS 服务: {}\n请确认 GPT-SoVITS api_v2.py 已启动", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("TTS 服务返回错误: HTTP 500: {}", body));
    }

    if !config.gpt_weights.is_empty() {
        set_model_weights(&config.api_url, &config.gpt_weights, "").await?;
    }
    if !config.sovits_weights.is_empty() {
        set_model_weights(&config.api_url, "", &config.sovits_weights).await?;
    }

    Ok(format!("TTS 已连接到 {}", config.api_url))
}

pub async fn generate_speech(config: &TtsConfig, text: &str) -> Result<Vec<u8>, String> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("创建HTTP客户端失败: {}", e))?;

    let text_lang = if config.text_lang.is_empty() { "zh" } else { &config.text_lang };
    let prompt_lang = if config.prompt_lang.is_empty() { "zh" } else { &config.prompt_lang };
    let ref_audio = config.ref_audio_path.trim_matches('"');
    let prompt_text = config.prompt_text.trim_matches('"');
    let speed_str = config.speed.to_string();
    let steps_str = config.sample_steps.to_string();

    let mut url = reqwest::Url::parse(&format!("{}/tts", config.api_url.trim_end_matches('/')))
        .map_err(|e| format!("URL 解析失败: {}", e))?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("text", text);
        pairs.append_pair("text_lang", text_lang);
        pairs.append_pair("ref_audio_path", ref_audio);
        pairs.append_pair("prompt_text", prompt_text);
        pairs.append_pair("prompt_lang", prompt_lang);
        pairs.append_pair("speed_factor", &speed_str);
        pairs.append_pair("sample_steps", &steps_str);
        pairs.append_pair("media_type", "wav");
        pairs.append_pair("streaming_mode", "false");
    }

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("TTS请求失败: {}\n请确认API服务已启动", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("TTS API错误: {}", body));
    }

    let bytes = resp.bytes().await.map_err(|e| format!("读取音频数据失败: {}", e))?;
    if bytes.len() < 100 {
        return Err(format!("TTS返回数据过短({}字节)，可能生成失败", bytes.len()));
    }
    Ok(bytes.to_vec())
}

pub fn play_audio(audio_data: &[u8]) -> Result<(), String> {
    let cursor = Cursor::new(audio_data.to_vec());
    let source = rodio::Decoder::new(cursor).map_err(|e| format!("解码音频失败: {}", e))?;
    let duration = source
        .total_duration()
        .unwrap_or(std::time::Duration::from_secs(10));
    let (_stream, stream_handle) =
        rodio::OutputStream::try_default().map_err(|e| format!("打开音频输出失败: {}", e))?;
    stream_handle
        .play_raw(source.convert_samples())
        .map_err(|e| format!("播放失败: {}", e))?;
    std::thread::sleep(duration);
    Ok(())
}

pub async fn set_model_weights(api_url: &str, gpt_path: &str, sovits_path: &str) -> Result<(), String> {
    let base = api_url.trim_end_matches('/');
    let client = Client::new();
    if !gpt_path.is_empty() {
        let mut url = reqwest::Url::parse(&format!("{}/set_gpt_weights", base))
            .map_err(|e| format!("URL 解析失败: {}", e))?;
        url.query_pairs_mut().append_pair("weights_path", gpt_path);
        client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("设置GPT权重失败: {}", e))?;
    }
    if !sovits_path.is_empty() {
        let mut url = reqwest::Url::parse(&format!("{}/set_sovits_weights", base))
            .map_err(|e| format!("URL 解析失败: {}", e))?;
        url.query_pairs_mut().append_pair("weights_path", sovits_path);
        client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("设置SoVITS权重失败: {}", e))?;
    }
    Ok(())
}

pub fn save_audio(audio_data: &[u8], dir: &str) -> Result<PathBuf, String> {
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {}", e))?;
    let path = dir.join(format!(
        "tts_{}.wav",
        std::time::UNIX_EPOCH.elapsed().unwrap_or_default().as_millis()
    ));
    std::fs::write(&path, audio_data).map_err(|e| format!("保存音频失败: {}", e))?;
    Ok(path)
}
