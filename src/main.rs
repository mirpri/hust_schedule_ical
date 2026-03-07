use std::{
    collections::{BTreeMap, HashMap},
    env,
    fs,
    io,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration as StdDuration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};
use clap::{Parser, ValueEnum};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, COOKIE, HeaderMap, HeaderValue, USER_AGENT};
use rookie::common::enums::Cookie as BrowserCookie;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tungstenite::{Message, connect};

const DEFAULT_URL: &str = "https://hubs.hust.edu.cn/schedule/getStudentScheduleByXqh";
const DEFAULT_COOKIE_DOMAIN: &str = "hubs.hust.edu.cn";

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Cli {
    /// Semester code, e.g. 20252
    #[arg(long, default_value = "20252")]
    xqh: String,

    /// Output .ics file path
    #[arg(long, short, default_value = "schedule.ics")]
    output: PathBuf,

    /// JSON file containing class period start/end times
    #[arg(long, default_value = "class-times.example.json")]
    class_times: PathBuf,

    /// Read schedule JSON from local file instead of fetching from HUST
    #[arg(long)]
    input_json: Option<PathBuf>,

    /// Fetch endpoint base URL
    #[arg(long, default_value = DEFAULT_URL)]
    url: String,

    /// Raw Cookie request header, e.g. "JSESSIONID=...; route=..."
    #[arg(long)]
    cookie_header: Option<String>,

    /// Cookie file path. Supports raw header text or Netscape cookie file format.
    #[arg(long)]
    cookie_file: Option<PathBuf>,

    /// Launch a browser window, let the user login, then fetch cookies through the DevTools protocol.
    #[arg(long)]
    interactive_login: bool,

    /// Browser cookie source
    #[arg(long, value_enum, default_value_t = Browser::Any)]
    browser: Browser,

    /// Cookie domain filter
    #[arg(long, default_value = DEFAULT_COOKIE_DOMAIN)]
    cookie_domain: String,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Browser {
    Any,
    Chrome,
    Edge,
    Firefox,
}

#[derive(Clone, Copy)]
enum ChromiumFamily {
    Chrome,
    Edge,
}

#[derive(Debug, Deserialize)]
struct WeekSchedule {
    #[serde(rename = "MONDAY")]
    monday: Vec<Course>,
    #[serde(rename = "TUESDAY")]
    tuesday: Vec<Course>,
    #[serde(rename = "WEDNESDAY")]
    wednesday: Vec<Course>,
    #[serde(rename = "THURSDAY")]
    thursday: Vec<Course>,
    #[serde(rename = "FRIDAY")]
    friday: Vec<Course>,
    #[serde(rename = "SATURDAY")]
    saturday: Vec<Course>,
    #[serde(rename = "SUNDAY")]
    sunday: Vec<Course>,
    #[serde(rename = "KS")]
    week_start: NaiveDate,
    #[serde(rename = "JS")]
    week_end: NaiveDate,
    #[serde(rename = "ZC")]
    week_index: u32,
}

#[derive(Debug, Deserialize, Clone)]
struct Course {
    #[serde(rename = "KCMC")]
    course_name: String,
    #[serde(rename = "JSMC", default)]
    classroom: Option<String>,
    #[serde(rename = "QSJC")]
    start_period: String,
    #[serde(rename = "JSJC")]
    end_period: String,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
struct ClassTimeFile {
    timezone: Option<String>,
    periods: Vec<ClassPeriod>,
}

#[derive(Debug)]
struct LoadedClassTimes {
    timezone: String,
    periods: HashMap<u32, (NaiveTime, NaiveTime)>,
}

#[derive(Debug, Deserialize)]
struct ClassPeriod {
    index: u32,
    start: String,
    end: String,
}

#[derive(Debug, Serialize)]
struct CalendarEvent {
    uid: String,
    summary: String,
    location: Option<String>,
    description: String,
    start: NaiveDateTime,
    end: NaiveDateTime,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DevtoolsTarget {
    url: String,
    #[serde(default)]
    web_socket_debugger_url: Option<String>,
    #[serde(rename = "type")]
    target_type: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let class_times = load_class_times(&cli.class_times)?;
    let raw_schedule = match &cli.input_json {
        Some(path) => fs::read_to_string(path)
            .with_context(|| format!("failed to read schedule json from {}", path.display()))?,
        None => fetch_schedule(&cli)?,
    };

    let weeks: Vec<WeekSchedule> =
        serde_json::from_str(&raw_schedule).context("failed to parse schedule JSON")?;
    let events = build_events(&weeks, &class_times.periods)?;
    let ics = render_ics(&events, &class_times.timezone);

    fs::write(&cli.output, ics)
        .with_context(|| format!("failed to write {}", cli.output.display()))?;

    println!("wrote {} events to {}", events.len(), cli.output.display());
    Ok(())
}

fn load_class_times(path: &Path) -> Result<LoadedClassTimes> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read class times from {}", path.display()))?;
    let parsed: ClassTimeFile =
        serde_json::from_str(&raw).context("failed to parse class times JSON")?;

    let mut map = HashMap::new();
    for period in parsed.periods {
        let start = NaiveTime::parse_from_str(&period.start, "%H:%M")
            .with_context(|| format!("invalid start time for period {}", period.index))?;
        let end = NaiveTime::parse_from_str(&period.end, "%H:%M")
            .with_context(|| format!("invalid end time for period {}", period.index))?;
        map.insert(period.index, (start, end));
    }

    Ok(LoadedClassTimes {
        timezone: parsed
            .timezone
            .unwrap_or_else(|| "Asia/Shanghai".to_string()),
        periods: map,
    })
}

fn fetch_schedule(cli: &Cli) -> Result<String> {
    let cookie_header = resolve_cookie_header(cli)?;

    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36"),
    );
    headers.insert(ACCEPT, HeaderValue::from_static("application/json,text/plain,*/*"));
    headers.insert(
        COOKIE,
        HeaderValue::from_str(&cookie_header).context("cookie header contains invalid characters")?,
    );

    let client = Client::builder()
        .default_headers(headers)
        .build()
        .context("failed to build HTTP client")?;

    let response = client
        .get(&cli.url)
        .query(&[("XQH", cli.xqh.as_str())])
        .send()
        .context("failed to fetch schedule JSON")?;

    let final_url = response.url().to_string();
    let status = response.status();
    let body = response.text().context("failed to read response body")?;

    if !status.is_success() {
        bail!("request failed with status {status}: {body}");
    }

    let trimmed = body.trim_start();
    if trimmed.starts_with("<!DOCTYPE html")
        || trimmed.starts_with("<html")
        || final_url.contains("/login")
    {
        bail!(
            "request looks unauthenticated; final URL: {final_url}. Please ensure browser login is still valid."
        );
    }

    Ok(body)
}

fn resolve_cookie_header(cli: &Cli) -> Result<String> {
    if let Some(header) = &cli.cookie_header {
        let trimmed = header.trim();
        if trimmed.is_empty() {
            bail!("--cookie-header was provided but empty");
        }
        return Ok(trimmed.to_string());
    }

    if let Some(path) = &cli.cookie_file {
        return load_cookie_header_from_file(path);
    }

    if cli.interactive_login {
        return login_and_get_cookie_header(cli);
    }

    let cookies = load_browser_cookies(cli.browser, &cli.cookie_domain)?;
    if cookies.is_empty() {
        bail!(
            "no browser cookies found for domain {}. Try --browser edge/firefox, or pass --cookie-header / --cookie-file.",
            cli.cookie_domain
        );
    }

    Ok(cookies
        .into_iter()
        .map(|cookie| format!("{}={}", cookie.name, cookie.value))
        .collect::<Vec<_>>()
        .join("; "))
}

fn login_and_get_cookie_header(cli: &Cli) -> Result<String> {
    let browser = match cli.browser {
        Browser::Any => Browser::Chrome,
        Browser::Chrome | Browser::Edge => cli.browser,
        Browser::Firefox => bail!("--interactive-login currently supports Chrome or Edge only"),
    };

    let port = pick_debug_port()?;
    let profile_dir = temp_browser_profile_dir(browser)?;
    fs::create_dir_all(&profile_dir)
        .with_context(|| format!("failed to create profile dir {}", profile_dir.display()))?;

    let login_url = format!("{}?XQH={}", cli.url, cli.xqh);
    let mut child = launch_debug_browser(browser, port, &profile_dir, &login_url)?;

    wait_for_devtools(port)?;
    println!("A browser window has been opened. Complete login there, then press Enter here.");
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .context("failed to read confirmation from stdin")?;

    let header = fetch_cookie_header_from_devtools(port, &cli.cookie_domain)?;

    let _ = child.kill();
    let _ = child.wait();

    Ok(header)
}

fn pick_debug_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .context("failed to allocate a debug port")?;
    let port = listener
        .local_addr()
        .context("failed to read debug port")?
        .port();
    drop(listener);
    Ok(port)
}

fn temp_browser_profile_dir(browser: Browser) -> Result<PathBuf> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_millis();
    let browser_name = match browser {
        Browser::Chrome => "chrome",
        Browser::Edge => "edge",
        _ => "browser",
    };
    Ok(env::temp_dir().join(format!("hust_schedule_ical_{browser_name}_{millis}")))
}

fn launch_debug_browser(browser: Browser, port: u16, profile_dir: &Path, url: &str) -> Result<Child> {
    let exe = find_browser_executable(browser)?;
    Command::new(&exe)
        .arg(format!("--remote-debugging-port={port}"))
        .arg(format!("--user-data-dir={}", profile_dir.display()))
        .arg("--new-window")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to launch browser {}", exe.display()))
}

fn find_browser_executable(browser: Browser) -> Result<PathBuf> {
    let local_appdata = env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let program_files = env::var_os("ProgramFiles").map(PathBuf::from);
    let program_files_x86 = env::var_os("ProgramFiles(x86)").map(PathBuf::from);

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
        _ => Vec::new(),
    }
    .into_iter()
    .flatten()
    .collect();

    candidates
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| anyhow!("failed to find browser executable"))
}

fn wait_for_devtools(port: u16) -> Result<()> {
    let client = Client::builder()
        .build()
        .context("failed to build HTTP client for DevTools")?;

    for _ in 0..50 {
        let response = client
            .get(format!("http://127.0.0.1:{port}/json/version"))
            .send();
        if response.is_ok() {
            return Ok(());
        }
        thread::sleep(StdDuration::from_millis(200));
    }

    bail!("browser DevTools endpoint did not become ready")
}

fn fetch_cookie_header_from_devtools(port: u16, domain: &str) -> Result<String> {
    let client = Client::builder()
        .build()
        .context("failed to build HTTP client for DevTools")?;

    let targets: Vec<DevtoolsTarget> = client
        .get(format!("http://127.0.0.1:{port}/json/list"))
        .send()
        .context("failed to query DevTools targets")?
        .json()
        .context("failed to parse DevTools target list")?;

    let target = targets
        .into_iter()
        .find(|target| {
            target.target_type == "page"
                && (target.url.contains("hust.edu.cn") || target.url == "about:blank")
                && target.web_socket_debugger_url.is_some()
        })
        .ok_or_else(|| anyhow!("failed to find a browser page target for HUST login"))?;

    let ws_url = target
        .web_socket_debugger_url
        .ok_or_else(|| anyhow!("missing websocket debugger url"))?;
    let (mut socket, _) = connect(ws_url.as_str())
        .context("failed to connect to browser DevTools websocket")?;

    socket
        .send(Message::Text(
            json!({"id": 1, "method": "Network.getCookies", "params": {"urls": ["https://hubs.hust.edu.cn/"]}})
                .to_string(),
        ))
        .context("failed to request cookies from DevTools")?;

    let domain_filters = expanded_cookie_domains(domain);
    for _ in 0..20 {
        let message = socket.read().context("failed to read DevTools response")?;
        let Message::Text(text) = message else {
            continue;
        };
        let payload: Value = serde_json::from_str(&text).context("invalid DevTools JSON response")?;
        if payload.get("id").and_then(Value::as_i64) != Some(1) {
            continue;
        }

        let cookies = payload["result"]["cookies"]
            .as_array()
            .ok_or_else(|| anyhow!("DevTools response did not contain cookies"))?;
        let pairs: Vec<String> = cookies
            .iter()
            .filter_map(|cookie| {
                let domain = cookie.get("domain")?.as_str()?.trim_start_matches('.').to_ascii_lowercase();
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
            bail!("DevTools login flow succeeded, but no HUST cookies were captured");
        }

        return Ok(pairs.join("; "));
    }

    bail!("timed out waiting for cookies from DevTools")
}

fn load_cookie_header_from_file(path: &Path) -> Result<String> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read cookie file {}", path.display()))?;
    let trimmed = raw.trim();

    if trimmed.is_empty() {
        bail!("cookie file {} is empty", path.display());
    }

    if !trimmed.contains('\n') && trimmed.contains('=') {
        return Ok(trimmed.trim_start_matches("Cookie:").trim().to_string());
    }

    let mut pairs = Vec::new();
    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if !line.contains('\t') {
            continue;
        }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 7 {
            pairs.push(format!("{}={}", parts[5], parts[6]));
        }
    }

    if pairs.is_empty() {
        bail!(
            "unsupported cookie file format in {}. Provide a raw cookie header or Netscape cookie file.",
            path.display()
        );
    }

    Ok(pairs.join("; "))
}

fn load_browser_cookies(browser: Browser, domain: &str) -> Result<Vec<BrowserCookie>> {
    let domain_filters = expanded_cookie_domains(domain);
    let domains = Some(domain_filters.clone());

    let direct = match browser {
        Browser::Any => rookie::chrome(domains.clone())
            .or_else(|_| rookie::edge(domains.clone()))
            .or_else(|_| rookie::firefox(domains.clone())),
        Browser::Chrome => rookie::chrome(domains.clone()),
        Browser::Edge => rookie::edge(domains.clone()),
        Browser::Firefox => rookie::firefox(domains.clone()),
    };

    let all = match direct {
        Ok(cookies) if !cookies.is_empty() => cookies,
        _ => match browser {
            Browser::Any => try_chromium_explicit(ChromiumFamily::Chrome, &domain_filters)?
                .or_else(|| try_chromium_explicit(ChromiumFamily::Edge, &domain_filters).ok().flatten())
                .or_else(|| rookie::firefox(domains).ok())
                .ok_or_else(|| anyhow!("failed to read browser cookies: no cookies found from Chrome, Edge, or Firefox"))?,
            Browser::Chrome => try_chromium_explicit(ChromiumFamily::Chrome, &domain_filters)?
                .ok_or_else(|| anyhow!("failed to read browser cookies: no Chrome cookies found"))?,
            Browser::Edge => try_chromium_explicit(ChromiumFamily::Edge, &domain_filters)?
                .ok_or_else(|| anyhow!("failed to read browser cookies: no Edge cookies found"))?,
            Browser::Firefox => rookie::firefox(domains)
                .map_err(|error| anyhow!("failed to read browser cookies: {error}"))?,
        },
    };

    let mut dedup = BTreeMap::new();
    for cookie in all {
        if !cookie_matches_any_domain(&cookie, &domain_filters) {
            continue;
        }
        dedup.insert(cookie.name.clone(), cookie);
    }
    Ok(dedup.into_values().collect())
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

fn try_chromium_explicit(
    family: ChromiumFamily,
    domain_filters: &[String],
) -> Result<Option<Vec<BrowserCookie>>> {
    let candidates = detect_chromium_paths(family)?;
    if candidates.is_empty() {
        return Ok(None);
    }

    let mut best: Option<Vec<BrowserCookie>> = None;

    for (cookies_path, key_path) in candidates {
        let cookies_path_string = cookies_path.to_string_lossy().into_owned();
        let key_path_string = key_path.to_string_lossy().into_owned();

        let cookies = match rookie::any_browser(&cookies_path_string, None, Some(&key_path_string)) {
            Ok(cookies) if !cookies.is_empty() => cookies,
            _ => continue,
        };

        let cookies: Vec<BrowserCookie> = cookies
            .into_iter()
            .filter(|cookie| cookie_matches_any_domain(cookie, domain_filters))
            .collect();
        if cookies.is_empty() {
            continue;
        }

        let has_session = cookies.iter().any(|cookie| {
            cookie.name.eq_ignore_ascii_case("JSESSIONID")
                || cookie.name.contains("Serverpool")
                || cookie.name.contains("BIGip")
        });

        if has_session {
            return Ok(Some(cookies));
        }

        if best.is_none() {
            best = Some(cookies);
        }
    }

    Ok(best)
}

fn cookie_matches_any_domain(cookie: &BrowserCookie, domain_filters: &[String]) -> bool {
    let cookie_domain = cookie.domain.trim_start_matches('.').to_ascii_lowercase();
    domain_filters.iter().any(|domain| {
        let domain = domain.to_ascii_lowercase();
        cookie_domain == domain || cookie_domain.ends_with(&format!(".{domain}"))
    })
}

fn detect_chromium_paths(family: ChromiumFamily) -> Result<Vec<(PathBuf, PathBuf)>> {
    let local_appdata = env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("LOCALAPPDATA is not set"))?;

    let user_data = match family {
        ChromiumFamily::Chrome => local_appdata.join("Google").join("Chrome").join("User Data"),
        ChromiumFamily::Edge => local_appdata.join("Microsoft").join("Edge").join("User Data"),
    };

    let key_path = user_data.join("Local State");
    if !key_path.exists() {
        return Ok(Vec::new());
    }

    let mut candidates = Vec::new();
    let mut profiles = vec![user_data.join("Default")];

    if let Ok(entries) = fs::read_dir(&user_data) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("Profile ") {
                profiles.push(path);
            }
        }
    }

    for profile in profiles {
        let cookie_path = profile.join("Network").join("Cookies");
        if cookie_path.exists() {
            candidates.push((cookie_path, key_path.clone()));
        }

        let cookie_path = profile.join("Cookies");
        if cookie_path.exists() {
            candidates.push((cookie_path, key_path.clone()));
        }
    }

    Ok(candidates)
}

fn build_events(
    weeks: &[WeekSchedule],
    class_times: &HashMap<u32, (NaiveTime, NaiveTime)>,
) -> Result<Vec<CalendarEvent>> {
    let mut events = Vec::new();

    for week in weeks {
        let day_sets = [
            ("MONDAY", 0_i64, &week.monday),
            ("TUESDAY", 1, &week.tuesday),
            ("WEDNESDAY", 2, &week.wednesday),
            ("THURSDAY", 3, &week.thursday),
            ("FRIDAY", 4, &week.friday),
            ("SATURDAY", 5, &week.saturday),
            ("SUNDAY", 6, &week.sunday),
        ];

        for (day_name, offset, courses) in day_sets {
            let date = week.week_start + Duration::days(offset);
            for course in courses {
                let start_period: u32 = course
                    .start_period
                    .parse()
                    .with_context(|| format!("invalid QSJC for {}", course.course_name))?;
                let end_period: u32 = course
                    .end_period
                    .parse()
                    .with_context(|| format!("invalid JSJC for {}", course.course_name))?;

                let (start_time, _) = class_times.get(&start_period).ok_or_else(|| {
                    anyhow!(
                        "missing class time config for start period {} ({})",
                        start_period,
                        course.course_name
                    )
                })?;
                let (_, end_time) = class_times.get(&end_period).ok_or_else(|| {
                    anyhow!(
                        "missing class time config for end period {} ({})",
                        end_period,
                        course.course_name
                    )
                })?;

                let start = NaiveDateTime::new(date, *start_time);
                let end = NaiveDateTime::new(date, *end_time);
                let description = build_description(week, day_name, course);
                let uid = format!(
                    "{}-{}-{}-{}@hust-schedule-ical",
                    week.week_index, day_name, start_period, course.course_name
                );

                events.push(CalendarEvent {
                    uid,
                    summary: course.course_name.clone(),
                    location: course.classroom.clone(),
                    description,
                    start,
                    end,
                });
            }
        }
    }

    events.sort_by_key(|event| event.start);
    Ok(events)
}

fn build_description(week: &WeekSchedule, day_name: &str, course: &Course) -> String {
    let mut lines = vec![
        format!("Week: {}", week.week_index),
        format!("Date range: {} to {}", week.week_start, week.week_end),
        format!("Day: {day_name}"),
    ];

    for (key, value) in &course.extra {
        if key == "KCMC" || key == "JSMC" {
            continue;
        }

        let rendered = match value {
            Value::String(text) => text.clone(),
            _ => value.to_string(),
        };
        lines.push(format!("{key}: {rendered}"));
    }

    lines.join("\n")
}

fn render_ics(events: &[CalendarEvent], timezone: &str) -> String {
    let mut out = String::new();
    out.push_str("BEGIN:VCALENDAR\r\n");
    out.push_str("VERSION:2.0\r\n");
    out.push_str("PRODID:-//Codex//HUST Schedule iCal//EN\r\n");
    out.push_str("CALSCALE:GREGORIAN\r\n");
    out.push_str("METHOD:PUBLISH\r\n");
    out.push_str(&format!("X-WR-TIMEZONE:{}\r\n", escape_ics_text(timezone)));

    for event in events {
        out.push_str("BEGIN:VEVENT\r\n");
        out.push_str(&format!("UID:{}\r\n", escape_ics_text(&event.uid)));
        out.push_str(&format!(
            "DTSTAMP:{}\r\n",
            chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
        ));
        out.push_str(&format!(
            "DTSTART;TZID={}:{}\r\n",
            escape_ics_text(timezone),
            event.start.format("%Y%m%dT%H%M%S")
        ));
        out.push_str(&format!(
            "DTEND;TZID={}:{}\r\n",
            escape_ics_text(timezone),
            event.end.format("%Y%m%dT%H%M%S")
        ));
        out.push_str(&format!(
            "SUMMARY:{}\r\n",
            escape_ics_text(&event.summary)
        ));
        if let Some(location) = &event.location {
            if !location.is_empty() {
                out.push_str(&format!(
                    "LOCATION:{}\r\n",
                    escape_ics_text(location)
                ));
            }
        }
        out.push_str(&format!(
            "DESCRIPTION:{}\r\n",
            escape_ics_text(&event.description)
        ));
        out.push_str("END:VEVENT\r\n");
    }

    out.push_str("END:VCALENDAR\r\n");
    out
}

fn escape_ics_text(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\r', "")
        .replace('\n', "\\n")
}
