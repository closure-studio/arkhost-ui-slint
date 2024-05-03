pub fn approximate_period_str(duration: chrono::Duration) -> String {
    if duration.num_days() > 0 {
        format!("{} 天", duration.num_days())
    } else if duration.num_hours() > 0 {
        format!("{} 小时", duration.num_hours())
    } else if duration.num_minutes() > 0 {
        format!("{} 分钟", duration.num_minutes())
    } else {
        "1 分钟".into()
    }
}
