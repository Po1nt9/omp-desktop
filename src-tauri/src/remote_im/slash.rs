//! IM slash commands: /p project · /r resume · /help …

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuiltinCommand {
    Help,
    New,
    Whoami,
    Status,
    Stop,
    Project { query: Option<String> },
    Resume { query: Option<String> },
    Unknown { raw: String },
}

pub fn parse_slash(text: &str) -> Option<BuiltinCommand> {
    let t = text.trim();
    if !t.starts_with('/') {
        return None;
    }
    let rest = &t[1..];
    let (head, query) = match rest.find(' ') {
        Some(i) => {
            let q = rest[i + 1..].trim();
            (
                rest[..i].to_ascii_lowercase(),
                if q.is_empty() {
                    None
                } else {
                    Some(q.to_string())
                },
            )
        }
        None => (rest.to_ascii_lowercase(), None),
    };
    Some(match head.as_str() {
        "help" | "h" | "?" => BuiltinCommand::Help,
        "new" | "reset" => BuiltinCommand::New,
        "whoami" | "id" => BuiltinCommand::Whoami,
        "status" => BuiltinCommand::Status,
        "stop" | "cancel" => BuiltinCommand::Stop,
        "p" | "project" => BuiltinCommand::Project { query },
        "r" | "resume" => BuiltinCommand::Resume { query },
        other => BuiltinCommand::Unknown {
            raw: other.to_string(),
        },
    })
}

pub fn help_text(lang: &str) -> String {
    if lang == "en" {
        [
            "**OMP Desktop Remote IM** — local OMP Runtime via IM (Rust)",
            "",
            "Commands:",
            "- `/help` — this message",
            "- `/p` · `/project` — list / bind a trusted project",
            "- `/p <name|n>` — bind by name or number",
            "- `/r` · `/resume` — list / resume a prior session",
            "- `/r <n>` — resume by number",
            "- `/new` — fresh session (keep project)",
            "- `/whoami` — show your sender id",
            "- `/status` — snapshot",
            "- `/stop` — cancel in-flight turn",
            "- `0` — cancel number-pick mode",
        ]
        .join("\n")
    } else {
        [
            "**OMP Desktop Remote IM** — 本地 OMP Runtime 远程 IM 桥（Rust 内置）",
            "",
            "命令：",
            "- `/help` — 显示帮助",
            "- `/p` · `/project` — 列出 / 绑定已信任项目",
            "- `/p <名|序号>` — 按名称或序号绑定",
            "- `/r` · `/resume` — 列出 / 恢复历史会话",
            "- `/r <序号>` — 按序号恢复",
            "- `/new` — 保持项目，开启新会话",
            "- `/whoami` — 查看发送者 id",
            "- `/status` — 状态快照",
            "- `/stop` — 中断当前任务",
            "- `0` — 取消序号选择",
        ]
        .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_project_resume() {
        assert_eq!(
            parse_slash("/p"),
            Some(BuiltinCommand::Project { query: None })
        );
        assert_eq!(
            parse_slash("/p 1"),
            Some(BuiltinCommand::Project {
                query: Some("1".into())
            })
        );
        assert_eq!(
            parse_slash("/r"),
            Some(BuiltinCommand::Resume { query: None })
        );
        assert!(matches!(parse_slash("hi"), None));
    }
}
