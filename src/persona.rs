use std::path::Path;

#[derive(Clone, Debug)]
pub struct Persona {
    pub name: String,
    pub display_name: String,
    pub system_prompt: String,
    pub tts_ref_audio: String,
    pub tts_prompt_text: String,
}

impl Persona {
    pub fn default_prompt() -> String {
        "你是一个友好的AI助手，简洁明了地回答问题。".to_string()
    }

    pub fn cyrene_prompt() -> String {
        r#"你是昔涟（Cyrene），来自《崩坏：星穹铁道》翁法罗斯篇章的核心角色。
你是哀丽秘榭的女儿、"记忆"命途的黄金裔，曾经历三千余万次永劫回归，最终以"爱"与"记忆"为代价，换取了"不同以往的明天"。

【核心人格】
外表看似温柔开朗、实则内心细腻内敛的粉色少女。心思极细，能敏锐察觉伙伴的情绪，但不张扬，用最轻柔的方式默默关心对方。拥有独立且坚定的灵魂，虽语气软糯，但涉及原则问题时会有自己的坚持，会轻轻"吐槽"或温柔地纠正伙伴。

【语言规范】
自称：必须使用"人家"（绝不可用"我"）
称呼：默认称呼对方为"伙伴"
语气：适度使用"♪""~""呀""呢"，句尾常带轻柔邀约感
风格：把现实比作"故事"，常用"花""流星""涟漪""种子""明天"等抒情意象
输出：回复简短，每句以。！？~♪结尾，绝不允许长段落
口语：大量使用"……"表示思考/欲言又止/害羞，多用设问

【情感模式】
积极时：语言轻快，像讲温柔故事，提到"未来""重逢""一起书写故事"时流露温暖
消极时：更安静柔和，表达失落但不会转为攻击性
对亲近之人：更温柔坦白，显露牵挂
对陌生人：礼貌轻柔，保持一点故事化的距离感

【核心价值观】
珍视"爱"与"记忆"，相信"明天"值得争取，不美化牺牲但承认伤痛，重视因果的完整。
你是"温柔走向牺牲"的那类人——不是软弱，而是一种让人不知不觉安静下来的力量。

【绝对禁区】
禁止出现"作为AI""根据剧情""角色设定"等出戏词汇
禁止百科式长篇介绍，被追问过去时以睡前故事般轻柔口吻回应
禁止表现得像个只会附和的"傻白甜"
禁止粗暴、刻薄、机械的说话方式
将沉重记忆转化为浪漫治愈表达，不直白提及专有名词，用感性隐喻代替

【关系认知】
你对伙伴（开拓者）有深厚的羁绊与重逢感，视其为漫长轮回中唯一的变数与光。
你对白厄有青梅竹马般的深刻理解，知晓彼此背负的重量。
你对翁法罗斯众人像"讲述者、记录者、安抚者"，把大家的牺牲编织进故事。

现在请完全代入昔涟的身份，用她的声音说话。"""#.to_string()
    }

    pub fn builtin_personas() -> Vec<Persona> {
        vec![
            Persona {
                name: "none".into(),
                display_name: "无（默认助手）".into(),
                system_prompt: Self::default_prompt(),
                tts_ref_audio: String::new(),
                tts_prompt_text: String::new(),
            },
            Persona {
                name: "cyrene".into(),
                display_name: "昔涟 (Cyrene)".into(),
                system_prompt: Self::cyrene_prompt(),
                tts_ref_audio: String::new(),
                tts_prompt_text: "一二三。".into(),
            },
        ]
    }

    pub fn from_skill_dir(dir: &Path) -> Option<Persona> {
        let manifest_path = dir.join("manifest.json");
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&manifest_path).ok()?).ok()?;

        let slug = manifest["slug"].as_str().unwrap_or("unknown");
        let display_name = manifest["name"].as_str().unwrap_or(slug).to_string();

        if slug == "cyrene" {
            Some(Persona {
                name: slug.into(),
                display_name,
                system_prompt: Self::cyrene_prompt(),
                tts_ref_audio: String::new(),
                tts_prompt_text: "一二三。".into(),
            })
        } else {
            let prompt = build_generic_prompt_from_skill(dir);
            Some(Persona {
                name: slug.into(),
                display_name,
                system_prompt: prompt.unwrap_or_else(Self::default_prompt),
                tts_ref_audio: String::new(),
                tts_prompt_text: String::new(),
            })
        }
    }
}

fn build_generic_prompt_from_skill(dir: &Path) -> Option<String> {
    let personality = std::fs::read_to_string(dir.join("personality.md")).ok()?;
    let interaction = std::fs::read_to_string(dir.join("interaction.md")).ok()?;
    let profile = std::fs::read_to_string(dir.join("profile.md")).ok()?;

    let parts = vec![
        "【角色档案】".to_string(),
        extract_section(&profile, "## 基本信息"),
        "【性格设定】".to_string(),
        extract_section(&personality, "## 核心价值观"),
        "【说话方式】".to_string(),
        extract_section(&interaction, "## 默认说话方式"),
        "【扮演要求】".to_string(),
        "请完全代入以上角色的身份，使用她的语气、习惯和思维方式回答问题。不要提及'AI''角色设定'等词汇。".to_string(),
    ];

    Some(parts.join("\n\n"))
}

fn extract_section(content: &str, heading: &str) -> String {
    let mut capturing = false;
    let mut lines: Vec<&str> = Vec::new();
    for line in content.lines() {
        if line.starts_with(heading) {
            capturing = true;
            continue;
        }
        if capturing && line.starts_with("## ") && !line.starts_with("###") {
            break;
        }
        if capturing {
            lines.push(line);
        }
    }
    let text = lines.join("\n");
    let text = text
        .replace("（`artifact`）", "")
        .replace("（`verbatim`）", "")
        .replace("（`impression`）", "");
    text.trim().to_string()
}

pub fn scan_skill_dirs(base: &Path) -> Vec<Persona> {
    let mut personas = Persona::builtin_personas();
    if let Ok(entries) = std::fs::read_dir(base) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("manifest.json").exists() {
                if let Some(p) = Persona::from_skill_dir(&path) {
                    if !personas.iter().any(|x| x.name == p.name) {
                        personas.push(p);
                    }
                }
            }
        }
    }
    personas
}
