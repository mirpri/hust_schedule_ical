use std::{
    fs,
    io,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};
use colored::*;

use crate::{
    cli::Cli,
    types::{Browser, ResolvedOptions, Settings},
};

pub const SETTINGS_PATH: &str = "settings.json";
pub const DEFAULT_COOKIE_DOMAIN: &str = "hubs.hust.edu.cn";

pub fn load_settings() -> Result<Settings> {
    let path = Path::new(SETTINGS_PATH);
    if !path.exists() {
        return Err(anyhow!("缺少 settings.json，请先创建该文件并填写默认值。"));
    }

    let raw = fs::read_to_string(path)
        .with_context(|| format!("读取 {} 失败", path.display()))?;
    let settings: Settings =
        serde_json::from_str(&raw).with_context(|| format!("解析 {} 失败", path.display()))?;
    Ok(settings)
}

pub fn save_settings(settings: &Settings) -> Result<()> {
    let serialized = serde_json::to_string_pretty(settings).context("序列化 settings.json 失败")?;
    fs::write(SETTINGS_PATH, serialized).context("写入 settings.json 失败")?;
    Ok(())
}

pub fn resolve_options(cli: Cli, settings: &mut Settings) -> Result<ResolvedOptions> {
    println!("{} {}", "请输入以下选项".yellow(), "直接 Enter 以使用默认值".italic().dimmed());
    let xqh = match cli.xqh {
        Some(value) => value,
        None => prompt_text("学期（学年+学期）", settings.xqh.as_deref())?,
    };
    let output = match cli.output {
        Some(value) => value,
        None => PathBuf::from(prompt_text("输出 .ics 文件路径", settings.output.as_deref())?),
    };
    let class_times = match cli.class_times {
        Some(value) => value,
        None => PathBuf::from(prompt_text(
            "课程时间 JSON 文件路径",
            settings.class_times.as_deref(),
        )?),
    };
    let url = match cli.url {
        Some(value) => value,
        None => settings
            .url
            .clone()
            .ok_or_else(|| anyhow!("URL 不能为空，且 settings.json 中也没有默认值。"))?,
    };
    let browser = match cli.browser {
        Some(value) => value,
        None => prompt_browser(settings.browser)?,
    };
    let reminder_minutes = match cli.reminder_minutes {
        Some(value) => value,
        None => {
            let default_str = settings.reminder_minutes.unwrap_or(15).to_string();
            let parsed = prompt_text("提前提醒时间(分钟，-1为不提醒，0为当时提醒)", Some(&default_str))?;
            parsed.parse().context("提醒时间必须是有效的数字")?
        }
    };

    settings.xqh = Some(xqh.clone());
    settings.output = Some(output.display().to_string());
    settings.class_times = Some(class_times.display().to_string());
    settings.url = Some(url.clone());
    settings.browser = Some(browser);
    settings.reminder_minutes = Some(reminder_minutes);

    Ok(ResolvedOptions {
        xqh,
        output,
        class_times,
        input_json: cli.input_json,
        url,
        browser,
        cookie_domain: DEFAULT_COOKIE_DOMAIN.to_string(),
        reminder_minutes,
        default_chrome_path: settings.chrome_path.as_ref().map(PathBuf::from),
        default_edge_path: settings.edge_path.as_ref().map(PathBuf::from),
    })
}

fn prompt_text(label: &str, default: Option<&str>) -> Result<String> {
    match default {
        Some(value) => println!("{label} [{}]:", value.purple().bold()),
        None => println!("{label}:"),
    }

    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .context("读取终端输入失败")?;
    let trimmed = line.trim();
    if trimmed.is_empty() {
        if let Some(value) = default {
            return Ok(value.to_string());
        }
        return Err(anyhow!("{label} 不能为空，且 settings.json 中也没有默认值。"));
    }
    Ok(trimmed.to_string())
}

fn prompt_browser(default: Option<Browser>) -> Result<Browser> {
    loop {
        match default {
            Some(browser) => println!("浏览器 [chrome/edge] [{}]:", browser.as_str().purple().bold()),
            None => println!("浏览器 [chrome/edge]:"),
        }

        let mut line = String::new();
        io::stdin()
            .read_line(&mut line)
            .context("读取终端输入失败")?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if let Some(browser) = default {
                return Ok(browser);
            }
            return Err(anyhow!("浏览器不能为空，且 settings.json 中也没有默认值。"));
        }
        if let Some(browser) = Browser::parse(trimmed) {
            return Ok(browser);
        }
        println!("{}", "无效的浏览器选项，请输入 chrome 或 edge。".red().bold());
    }
}
