use std::collections::HashMap;

use anyhow::{Context, Result, anyhow};
use chrono::{Duration, NaiveDateTime, NaiveTime};

use crate::types::{CalendarEvent, Course, WeekSchedule};

pub fn build_events(
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
                    .with_context(|| format!("课程 {} 的 QSJC 不是有效数字", course.course_name))?;
                let end_period: u32 = course
                    .end_period
                    .parse()
                    .with_context(|| format!("课程 {} 的 JSJC 不是有效数字", course.course_name))?;

                let (start_time, _) = class_times.get(&start_period).ok_or_else(|| {
                    anyhow!(
                        "课程 {} 缺少第 {} 节的时间配置",
                        course.course_name,
                        start_period
                    )
                })?;
                let (_, end_time) = class_times.get(&end_period).ok_or_else(|| {
                    anyhow!(
                        "课程 {} 缺少第 {} 节的时间配置",
                        course.course_name,
                        end_period
                    )
                })?;

                let start = NaiveDateTime::new(date, *start_time);
                let end = NaiveDateTime::new(date, *end_time);
                let description = build_description(course);
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

pub fn render_ics(events: &[CalendarEvent], timezone: &str) -> String {
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
        out.push_str(&format!("SUMMARY:{}\r\n", escape_ics_text(&event.summary)));
        if let Some(location) = &event.location {
            if !location.is_empty() {
                out.push_str(&format!("LOCATION:{}\r\n", escape_ics_text(location)));
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

fn build_description(course: &Course) -> String {
    return format!(
        "课堂：{}\n节次：{}-{} 节\n课程编号：{}\n课堂编号：{}",
        course.course_name,
        course.start_period,
        course.end_period,
        course.extra.get("KCBH").and_then(|v| v.as_str()).unwrap_or("未知"),
        course.extra.get("KTBH").and_then(|v| v.as_str()).unwrap_or("未知")
    );
}

fn escape_ics_text(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\r', "")
        .replace('\n', "\\n")
}
