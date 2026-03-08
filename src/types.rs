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

#[derive(Debug, Deserialize, Clone)]
pub struct SeasonSchedule {
    pub start_date: String,
    pub end_date: String,
    #[serde(alias = "classes", alias = "periods")]
    pub periods: Vec<ClassPeriod>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum PeriodsConfig {
    Seasons(Vec<SeasonSchedule>),
    Flat(Vec<ClassPeriod>),
}

#[derive(Debug, Deserialize)]
pub struct ClassTimeFile {
    pub timezone: Option<String>,
    pub periods: PeriodsConfig,
}

#[derive(Debug, Clone)]
pub struct LoadedSeasonSchedule {
    pub start_mmdd: u32,
    pub end_mmdd: u32,
    pub periods: HashMap<u32, (NaiveTime, NaiveTime)>,
}

#[derive(Debug)]
pub struct LoadedClassTimes {
    pub timezone: String,
    pub schedules: Vec<LoadedSeasonSchedule>,
    pub default_periods: Option<HashMap<u32, (NaiveTime, NaiveTime)>>,
}

impl LoadedClassTimes {
    pub fn get_class_time(&self, date: chrono::NaiveDate, period_index: u32) -> Option<(NaiveTime, NaiveTime)> {
        use chrono::Datelike;
        let current_mmdd = date.month() * 100 + date.day();

        for season in &self.schedules {
            let s = season.start_mmdd;
            let e = season.end_mmdd;
            let in_season = if s <= e {
                current_mmdd >= s && current_mmdd <= e
            } else {
                current_mmdd >= s || current_mmdd <= e
            };

            if in_season {
                if let Some(time) = season.periods.get(&period_index) {
                    return Some(*time);
                }
            }
        }
        
        if let Some(dp) = &self.default_periods {
            if let Some(time) = dp.get(&period_index) {
                return Some(*time);
            }
        }
        
        None
    }
}

#[derive(Debug, Deserialize, Clone)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chrome_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_path: Option<String>,
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
    pub default_chrome_path: Option<PathBuf>,
    pub default_edge_path: Option<PathBuf>,
}
