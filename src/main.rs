use std::fs;
use std::io::{self};

use anyhow::{Context, Result};
use clap::Parser;
use colored::*;

mod cli;
mod fetch;
mod ical;
mod settings;
mod types;

use cli::Cli;
use fetch::{fetch_schedule, load_class_times};
use ical::{build_events, render_ics};
use settings::{load_settings, resolve_options, save_settings};
use types::WeekSchedule;

fn main() {
    if let Err(e) = run() {
        eprintln!("{}", format!("错误: {:?}", e).red().bold());
    }
    
    // avoid immediate console close
    println!("\n{}", "按 Enter 键退出...".dimmed());
    let mut temp = String::new();
    let _ = io::stdin().read_line(&mut temp);
}

fn run() -> Result<()> {
    println!("{}", "HUST 课表 ICS 生成器 - 获取课表数据并生成 iCalendar 文件".cyan().bold());
    println!("{}", "可导入到 Outlook、Google Calendar、Apple Calendar、手机系统日历等应用".cyan());
    println!("{}", "by Mirpri\n".dimmed());
    
    let cli = Cli::parse();
    let mut settings = load_settings()?;
    let options = resolve_options(cli, &mut settings)?;
    save_settings(&settings)?;

    let class_times = load_class_times(&options.class_times)?;
    let raw_schedule = match &options.input_json {
        Some(path) => fs::read_to_string(path)
            .with_context(|| format!("读取课表 JSON 失败：{}", path.display()))?,
        None => {
            let sc = fetch_schedule(&options, &mut settings)?;
            save_settings(&settings)?; // update new browser paths
            sc
        }
    };

    let weeks: Vec<WeekSchedule> =
        serde_json::from_str(&raw_schedule).context("解析课表 JSON 失败")?;
    let events = build_events(&weeks, &class_times)?;
    let ics = render_ics(&events, &class_times.timezone, options.reminder_minutes);

    fs::write(&options.output, ics)
        .with_context(|| format!("写入输出文件失败：{}", options.output.display()))?;

    println!("{}", format!("✔ 已生成 {} 个日历事件，输出文件：{}", events.len(), options.output.display()).green().bold());
    Ok(())
}
