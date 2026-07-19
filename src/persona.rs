use log::warn;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct Persona {
    pub name: String,
    pub display_name: String,
    pub system_prompt: String,
}

pub fn scan_skill_dirs(base: &Path) -> Vec<Persona> {
    let mut personas: Vec<Persona> = Vec::new();
    let personas_dir = base.join("personas");

    if !personas_dir.is_dir() {
        personas.push(Persona {
            name: "none".into(),
            display_name: "无（默认助手）".into(),
            system_prompt: "你是一个友好的AI助手，简洁明了地回答问题。".into(),
        });
        return personas;
    }

    if let Ok(entries) = std::fs::read_dir(&personas_dir) {
        for entry in entries.flatten() {
            let path = entry.path();

            // 子文件夹格式: personas/<name>/system_prompt.txt
            if path.is_dir() {
                let prompt_path = path.join("system_prompt.txt");
                let prompt = match std::fs::read_to_string(&prompt_path) {
                    Ok(s) => s.trim().to_string(),
                    Err(_) => continue,
                };
                if prompt.is_empty() {
                    warn!("[persona] 角色文件内容为空，已跳过");
                    continue;
                }
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                let display_name = std::fs::read_to_string(path.join("display_name.txt"))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|_| name.clone());
                personas.push(Persona { name, display_name, system_prompt: prompt });
                continue;
            }

            // 直接 txt 格式: personas/<name>.txt
            if let Some(ext) = path.extension() {
                if ext == "txt" {
                    let name = path
                        .file_stem()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let prompt = match std::fs::read_to_string(&path) {
                        Ok(s) => s.trim().to_string(),
                        Err(_) => continue,
                    };
                    if prompt.is_empty() {
                        warn!("[persona] 角色文件内容为空，已跳过");
                        continue;
                    }
                    let display_name = std::fs::read_to_string(
                        personas_dir.join(format!("{}.display_name.txt", name)),
                    )
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|_| name.clone());
                    personas.push(Persona { name, display_name, system_prompt: prompt });
                }
            }
        }
    }

    let has_none = personas.iter().any(|p| p.name == "none");
    if !has_none {
        personas.push(Persona {
            name: "none".into(),
            display_name: "无（默认助手）".into(),
            system_prompt: "你是一个友好的AI助手，简洁明了地回答问题。".into(),
        });
    }

    personas
}
