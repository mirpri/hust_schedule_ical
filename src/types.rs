use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
};

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Browser {
    Chrome,
    Edge,
}

impl Browser {
    pub fn parse(input: &str) -> Option<Self> {
        match input.trim().to_ascii_lowercase().as_str() {
            "chrome" | "c" => Some(Self::Chrome),
            "edge" | "e" => Some(Self::Edge),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chrome => "chrome",
            Self::Edge => "edge",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct WeekSchedule {
    #[serde(rename = "MONDAY")]
    pub monday: Vec<Course>,
    #[serde(rename = "TUESDAY")]
    pub tuesday: Vec<Course>,
    #[serde(rename = "WEDNESDAY")]
    pub wednesday: Vec<Course>,
    #[serde(rename = "THURSDAY")]
    pub thursday: Vec<Course>,
    #[serde(rename = "FRIDAY")]
    pub friday: Vec<Course>,
    #[serde(rename = "SATURDAY")]
    pub saturday: Vec<Course>,
    #[serde(rename = "SUNDAY")]
    pub sunday: Vec<Course>,
    #[serde(rename = "KS")]
    pub week_start: NaiveDate,
    // #[serde(rename = "JS")]
    // pub week_end: NaiveDate,
    #[serde(rename = "ZC")]
    pub week_index: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Course {
    #[serde(rename = "KCMC")]
    pub course_name: String,
    #[serde(rename = "JSMC", default)]
    pub classroom: Option<String>,
    #[serde(rename = "QSJC")]
    pub start_period: String,
    #[serde(rename = "JSJC")]
    pub end_period: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
pub struct ClassTimeFile {
    pub timezone: Option<String>,
    pub periods: Vec<ClassPeriod>,
}

#[derive(Debug)]
pub struct LoadedClassTimes {
    pub timezone: String,
    pub periods: HashMap<u32, (NaiveTime, NaiveTime)>,
}

#[derive(Debug, Deserialize)]
pub struct ClassPeriod {
    pub index: u32,
    pub start: String,
    pub end: String,
}

#[derive(Debug, Serialize)]
pub struct CalendarEvent {
    pub uid: String,
    pub summary: String,
    pub location: Option<String>,
    pub description: String,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevtoolsTarget {
    pub url: String,
    #[serde(default)]
    pub web_socket_debugger_url: Option<String>,
    #[serde(rename = "type")]
    pub target_type: String,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Settings {
    pub xqh: Option<String>,
    pub output: Option<String>,
    pub class_times: Option<String>,
    pub url: Option<String>,
    pub browser: Option<Browser>,
    pub updated_at: Option<String>,
}

#[derive(Debug)]
pub struct ResolvedOptions {
    pub xqh: String,
    pub output: PathBuf,
    pub class_times: PathBuf,
    pub input_json: Option<PathBuf>,
    pub url: String,
    pub browser: Browser,
    pub cookie_domain: String,
}
