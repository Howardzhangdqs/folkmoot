//! 人类渲染：comfy-table 表格 + 时间线（§5.3）

use chrono::{DateTime, Local, Utc};
use comfy_table::{presets::UTF8_FULL, Table};

use folkmoot_common::{ConversationSummary, Message};

pub fn table(headers: &[&str], rows: Vec<Vec<String>>) -> String {
    let mut t = Table::new();
    t.load_preset(UTF8_FULL);
    t.set_header(headers.to_vec());
    for r in rows {
        t.add_row(r);
    }
    t.to_string()
}

pub fn rel_time(dt: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let d = now - *dt;
    if d.num_seconds() < 60 {
        "just now".into()
    } else if d.num_minutes() < 60 {
        format!("{}m ago", d.num_minutes())
    } else if d.num_hours() < 24 {
        format!("{}h ago", d.num_hours())
    } else {
        format!("{}d ago", d.num_days())
    }
}

/// 时间线：`14:02 agent-beta | deploy done`；附件行 `[image chart.png sha256:ab12cd34… id=0198…]`
pub fn timeline_line(m: &Message) -> String {
    let ts: DateTime<Local> = m.created_at.with_timezone(&Local);
    let sender = m.sender_key.clone().unwrap_or_else(|| m.sender_id[..8].to_string());
    let mut lines = Vec::new();
    if let Some(text) = &m.text {
        for (i, l) in text.lines().enumerate() {
            if i == 0 {
                lines.push(format!("{} {:<12} | {}", ts.format("%H:%M"), sender, l));
            } else {
                lines.push(format!("      {:<12} | {}", "", l));
            }
        }
    }
    for a in &m.attachments {
        lines.push(format!(
            "{} {:<12} | [image {} sha256:{}… id={}]",
            ts.format("%H:%M"),
            sender,
            a.file_name,
            &a.sha256[..8],
            a.id
        ));
    }
    if lines.is_empty() {
        lines.push(format!("{} {:<12} | (empty)", ts.format("%H:%M"), sender));
    }
    lines.join("\n")
}

pub fn conv_row(c: &ConversationSummary) -> Vec<String> {
    let kind = match c.kind {
        folkmoot_common::ConversationKind::Dm => "dm".to_string(),
        folkmoot_common::ConversationKind::Group => {
            format!("group: {}", c.title.clone().unwrap_or_default())
        }
    };
    let members = c
        .members
        .iter()
        .map(|m| m.agent_key.clone().unwrap_or_else(|| m.agent_id[..8].into()))
        .collect::<Vec<_>>()
        .join(", ");
    vec![
        c.id[..8].to_string(),
        kind,
        members,
        rel_time(&c.last_message_at),
    ]
}
