use std::{
    fs,
    io,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration as StdDuration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use chrono::NaiveTime;
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, COOKIE, HeaderMap, HeaderValue, USER_AGENT};
use serde_json::{Value, json};
use tungstenite::{Message, connect};

use crate::types::{Browser, ClassTimeFile, DevtoolsTarget, LoadedClassTimes, ResolvedOptions};

pub fn load_class_times(path: &Path) -> Result<LoadedClassTimes> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("读取课程时间文件失败：{}", path.display()))?;
    let parsed: ClassTimeFile =
        serde_json::from_str(&raw).context("解析课程时间 JSON 失败")?;

    let mut map = std::collections::HashMap::new();
    for period in parsed.periods {
        let start = NaiveTime::parse_from_str(&period.start, "%H:%M")
            .with_context(|| format!("第 {} 节开始时间格式不正确", period.index))?;
        let end = NaiveTime::parse_from_str(&period.end, "%H:%M")
            .with_context(|| format!("第 {} 节结束时间格式不正确", period.index))?;
        map.insert(period.index, (start, end));
    }

    Ok(LoadedClassTimes {
        timezone: parsed.timezone.unwrap_or_else(|| "Asia/Shanghai".to_string()),
        periods: map,
    })
}

pub fn fetch_schedule(options: &ResolvedOptions) -> Result<String> {
    let cookie_header = login_and_get_cookie_header(options)?;

    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36"),
    );
    headers.insert(ACCEPT, HeaderValue::from_static("application/json,text/plain,*/*"));
    headers.insert(
        COOKIE,
        HeaderValue::from_str(&cookie_header).context("Cookie 请求头包含非法字符")?,
    );

    let client = Client::builder()
        .default_headers(headers)
        .build()
        .context("创建 HTTP 客户端失败")?;

    let response = client
        .get(&options.url)
        .query(&[("XQH", options.xqh.as_str())])
        .send()
        .context("请求课表接口失败")?;

    let final_url = response.url().to_string();
    let status = response.status();
    let body = response.text().context("读取响应内容失败")?;

    if !status.is_success() {
        bail!("课表接口返回错误状态码 {status}: {body}");
    }

    let trimmed = body.trim_start();
    if trimmed.starts_with("<!DOCTYPE html")
        || trimmed.starts_with("<html")
        || final_url.contains("/login")
    {
        bail!("登录似乎未生效，最终跳转到了：{final_url}");
    }

    Ok(body)
}

fn login_and_get_cookie_header(options: &ResolvedOptions) -> Result<String> {
    let port = pick_debug_port()?;
    let profile_dir = temp_browser_profile_dir(options.browser)?;
    fs::create_dir_all(&profile_dir)
        .with_context(|| format!("创建临时浏览器配置目录失败：{}", profile_dir.display()))?;

    let login_url = format!("{}?XQH={}", options.url, options.xqh);
    let mut child = launch_debug_browser(options.browser, port, &profile_dir, &login_url)?;

    let result = (|| -> Result<String> {
        wait_for_devtools(port)?;
        println!("已打开浏览器窗口，请在其中登录hub系统，登录成功后回到这里按Enter继续。");
        let mut line = String::new();
        io::stdin()
            .read_line(&mut line)
            .context("读取终端确认输入失败")?;
        fetch_cookie_header_from_devtools(port, &options.cookie_domain)
    })();

    let _ = child.kill();
    let _ = child.wait();
    let _ = fs::remove_dir_all(&profile_dir);

    result
}

fn pick_debug_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .context("分配调试端口失败")?;
    let port = listener
        .local_addr()
        .context("读取调试端口失败")?
        .port();
    drop(listener);
    Ok(port)
}

fn temp_browser_profile_dir(browser: Browser) -> Result<PathBuf> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("系统时间异常")?
        .as_millis();
    Ok(std::env::temp_dir().join(format!(
        "hust_schedule_ical_{}_{}",
        browser.as_str(),
        millis
    )))
}

fn launch_debug_browser(browser: Browser, port: u16, profile_dir: &Path, url: &str) -> Result<Child> {
    let exe = find_browser_executable(browser)?;
    Command::new(&exe)
        .arg(format!("--remote-debugging-port={port}"))
        .arg(format!("--user-data-dir={}", profile_dir.display()))
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--new-window")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("启动浏览器失败：{}", exe.display()))
}

fn find_browser_executable(browser: Browser) -> Result<PathBuf> {
    let local_appdata = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let program_files = std::env::var_os("ProgramFiles").map(PathBuf::from);
    let program_files_x86 = std::env::var_os("ProgramFiles(x86)").map(PathBuf::from);

    let candidates: Vec<PathBuf> = match browser {
        Browser::Chrome => vec![
            local_appdata.clone().map(|p| p.join("Google\\Chrome\\Application\\chrome.exe")),
            program_files.clone().map(|p| p.join("Google\\Chrome\\Application\\chrome.exe")),
            program_files_x86.clone().map(|p| p.join("Google\\Chrome\\Application\\chrome.exe")),
        ],
        Browser::Edge => vec![
            local_appdata.clone().map(|p| p.join("Microsoft\\Edge\\Application\\msedge.exe")),
            program_files.clone().map(|p| p.join("Microsoft\\Edge\\Application\\msedge.exe")),
            program_files_x86.clone().map(|p| p.join("Microsoft\\Edge\\Application\\msedge.exe")),
        ],
    }
    .into_iter()
    .flatten()
    .collect();

    candidates
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| anyhow!("未找到浏览器可执行文件"))
}

fn wait_for_devtools(port: u16) -> Result<()> {
    let client = Client::builder()
        .build()
        .context("创建 DevTools HTTP 客户端失败")?;

    for _ in 0..50 {
        let response = client
            .get(format!("http://127.0.0.1:{port}/json/version"))
            .send();
        if response.is_ok() {
            return Ok(());
        }
        thread::sleep(StdDuration::from_millis(200));
    }

    bail!("浏览器调试接口未在预期时间内就绪")
}

fn fetch_cookie_header_from_devtools(port: u16, domain: &str) -> Result<String> {
    let client = Client::builder()
        .build()
        .context("创建 DevTools HTTP 客户端失败")?;

    let targets: Vec<DevtoolsTarget> = client
        .get(format!("http://127.0.0.1:{port}/json/list"))
        .send()
        .context("获取浏览器页面列表失败")?
        .json()
        .context("解析浏览器页面列表失败")?;

    let target = targets
        .into_iter()
        .find(|target| {
            target.target_type == "page"
                && (target.url.contains("hust.edu.cn") || target.url == "about:blank")
                && target.web_socket_debugger_url.is_some()
        })
        .ok_or_else(|| anyhow!("未找到 HUST 登录页面对应的浏览器标签页"))?;

    let ws_url = target
        .web_socket_debugger_url
        .ok_or_else(|| anyhow!("浏览器调试地址缺失"))?;
    let (mut socket, _) = connect(ws_url.as_str())
        .context("连接浏览器调试 WebSocket 失败")?;

    socket
        .send(Message::Text(
            json!({"id": 1, "method": "Network.getCookies", "params": {"urls": ["https://hubs.hust.edu.cn/"]}})
                .to_string(),
        ))
        .context("通过 DevTools 获取 Cookie 失败")?;

    let domain_filters = expanded_cookie_domains(domain);
    for _ in 0..20 {
        let message = socket.read().context("读取 DevTools 返回结果失败")?;
        let Message::Text(text) = message else {
            continue;
        };
        let payload: Value = serde_json::from_str(&text).context("DevTools 返回了无效 JSON")?;
        if payload.get("id").and_then(Value::as_i64) != Some(1) {
            continue;
        }

        let cookies = payload["result"]["cookies"]
            .as_array()
            .ok_or_else(|| anyhow!("DevTools 返回中没有 cookies 字段"))?;
        let pairs: Vec<String> = cookies
            .iter()
            .filter_map(|cookie| {
                let domain = cookie
                    .get("domain")?
                    .as_str()?
                    .trim_start_matches('.')
                    .to_ascii_lowercase();
                if !domain_filters.iter().any(|filter| {
                    let filter = filter.to_ascii_lowercase();
                    domain == filter || domain.ends_with(&format!(".{filter}"))
                }) {
                    return None;
                }
                let name = cookie.get("name")?.as_str()?;
                let value = cookie.get("value")?.as_str()?;
                Some(format!("{name}={value}"))
            })
            .collect();

        if pairs.is_empty() {
            bail!("登录完成后仍未捕获到 HUST Cookie");
        }

        return Ok(pairs.join("; "));
    }

    bail!("等待 DevTools 返回 Cookie 超时")
}

fn expanded_cookie_domains(domain: &str) -> Vec<String> {
    let mut filters = vec![domain.to_string()];
    let parts: Vec<&str> = domain.split('.').collect();
    for i in 1..parts.len().saturating_sub(1) {
        filters.push(parts[i..].join("."));
    }
    if !filters.iter().any(|d| d == "hust.edu.cn") && domain.ends_with("hust.edu.cn") {
        filters.push("hust.edu.cn".to_string());
    }
    filters.sort();
    filters.dedup();
    filters
}
