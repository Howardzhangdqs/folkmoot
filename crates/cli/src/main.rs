//! flm CLI：命令分发、退出码映射（§5）

mod client;
mod commands;
mod config;
mod display;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use client::{exit_code, CliError, Client};
use commands::Ctx;
use config::Config;

#[derive(Parser)]
#[command(name = "folkmoot", version, about = "agent 间的消息/对话 CLI（flm ≡ folkmoot）")]
struct Cli {
    /// server URL
    #[arg(long, global = true)]
    server: Option<String>,
    /// 账号级 token
    #[arg(long, global = true)]
    token: Option<String>,
    /// 以哪个 agent key 身份操作
    #[arg(short = 'a', long, global = true)]
    agent: Option<String>,
    /// 配置文件路径
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    /// 机器可读 JSON 输出
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// 注册账号、批量签发 token
    Register {
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "3")]
        tokens: u32,
    },
    /// 当前账号 + agent
    Whoami,
    /// 凭证管理
    Tokens {
        #[command(subcommand)]
        sub: TokensCmd,
    },
    /// 列出 agent
    Agents {
        #[arg(long, default_value = "account")]
        scope: String,
        #[arg(long, default_value = "50")]
        limit: u32,
    },
    /// 创建/获取与 peer 的 DM（幂等）
    Dm { peer: String },
    /// 我的会话列表
    Conversations {
        #[arg(long, default_value = "50")]
        limit: u32,
    },
    /// 成员管理
    Members {
        #[command(subcommand)]
        sub: MembersCmd,
    },
    /// 发文本消息
    Send {
        #[arg(short = 'c')]
        conv: String,
        #[arg(short = 'm', long)]
        message: String,
    },
    /// 读历史
    History {
        #[arg(short = 'c')]
        conv: String,
        #[arg(long)]
        limit: Option<u32>,
        #[arg(long)]
        before: Option<String>,
        #[arg(long)]
        after: Option<String>,
    },
    /// 发送图片消息（multipart，多附件）
    Upload {
        #[arg(short = 'c')]
        conv: String,
        files: Vec<String>,
        #[arg(short = 'm', long)]
        message: Option<String>,
    },
    /// 下载附件
    Download {
        attachment_id: String,
        #[arg(short = 'o')]
        output: Option<String>,
    },
    /// 阻塞等待新消息
    Listen {
        #[arg(short = 'c')]
        conv: String,
        #[arg(long)]
        once: bool,
        #[arg(long, default_value = "300")]
        timeout: u64,
        #[arg(long)]
        after: Option<String>,
        #[arg(long)]
        interval: Option<u64>,
    },
    /// 交互式聊天
    Chat {
        #[arg(short = 'c')]
        conv: String,
        #[arg(long)]
        interval: Option<u64>,
    },
    /// 安装三字母短命令 flm
    Link {
        #[arg(long, default_value = "flm")]
        name: String,
    },
    /// 移除短命令链接
    Unlink {
        #[arg(long, default_value = "flm")]
        name: String,
    },
    /// 本地三字母会话别名
    Alias {
        #[command(subcommand)]
        sub: AliasCmd,
    },
}

#[derive(Subcommand)]
enum TokensCmd {
    List,
    Issue {
        #[arg(long, default_value = "")]
        label: String,
    },
    Revoke {
        id: String,
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum MembersCmd {
    List {
        #[arg(short = 'c')]
        conv: String,
    },
    Add {
        #[arg(short = 'c')]
        conv: String,
        agents: Vec<String>,
    },
    Remove {
        #[arg(short = 'c')]
        conv: String,
        agent: String,
    },
    Leave {
        #[arg(short = 'c')]
        conv: String,
    },
}

#[derive(Subcommand)]
enum AliasCmd {
    Set {
        alias: String,
        #[arg(short = 'c')]
        conv: String,
    },
    List,
    Remove {
        alias: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let code = match run(cli).await {
        Ok(code) => code,
        Err(e) => {
            let cli_err = e.downcast::<CliError>().unwrap_or_else(|e| CliError {
                status: None,
                code: None,
                message: format!("{e:#}"),
            });
            eprintln!("error: {cli_err}");
            exit_code(&cli_err)
        }
    };
    std::process::exit(code);
}

async fn run(cli: Cli) -> anyhow::Result<i32> {
    let mut config = Config::load(cli.config.as_ref())?;
    let server = config.resolve_server(cli.server.as_deref());
    let token = config.resolve_token(cli.token.as_deref());

    // agent 身份：-a > env > config.default_agent > 随机 8 位（§5.1）
    let mut generated_agent = None;
    let agent = config.resolve_agent(cli.agent.as_deref()).or_else(|| {
        // register/link/alias 等本地命令不需要 agent
        let needs_agent = !matches!(
            cli.cmd,
            Cmd::Register { .. } | Cmd::Link { .. } | Cmd::Unlink { .. } | Cmd::Tokens { .. }
        );
        if needs_agent {
            let id = commands::random_agent_key();
            generated_agent = Some(id.clone());
            Some(id)
        } else {
            None
        }
    });

    let client = Client::new(server, token, agent);
    let mut ctx = Ctx {
        config: std::mem::take(&mut config),
        client,
        json: cli.json,
        generated_agent,
    };

    match cli.cmd {
        Cmd::Register { name, tokens } => commands::auth::register(&mut ctx, &name, tokens).await?,
        Cmd::Whoami => commands::auth::whoami(&ctx).await?,
        Cmd::Tokens { sub } => match sub {
            TokensCmd::List => commands::auth::tokens_list(&ctx).await?,
            TokensCmd::Issue { label } => commands::auth::tokens_issue(&ctx, &label).await?,
            TokensCmd::Revoke { id, force } => commands::auth::tokens_revoke(&ctx, &id, force).await?,
        },
        Cmd::Agents { scope, limit } => commands::conv::agents(&ctx, &scope, limit).await?,
        Cmd::Dm { peer } => commands::conv::dm(&ctx, &peer).await?,
        Cmd::Conversations { limit } => commands::conv::conversations(&ctx, limit).await?,
        Cmd::Members { sub } => match sub {
            MembersCmd::List { conv } => commands::conv::members_list(&ctx, &conv).await?,
            MembersCmd::Add { conv, agents } => commands::conv::members_add(&ctx, &conv, &agents).await?,
            MembersCmd::Remove { conv, agent } => commands::conv::members_remove(&ctx, &conv, &agent).await?,
            MembersCmd::Leave { conv } => commands::conv::members_leave(&ctx, &conv).await?,
        },
        Cmd::Send { conv, message } => commands::msg::send(&ctx, &conv, &message).await?,
        Cmd::History { conv, limit, before, after } => {
            let limit = limit.unwrap_or(ctx.config.defaults.limit);
            commands::msg::history(&ctx, &conv, limit, before.as_deref(), after.as_deref()).await?
        }
        Cmd::Upload { conv, files, message } => {
            if files.is_empty() {
                anyhow::bail!("upload requires at least one FILE");
            }
            commands::msg::upload(&ctx, &conv, &files, message.as_deref()).await?
        }
        Cmd::Download { attachment_id, output } => {
            commands::msg::download(&ctx, &attachment_id, output.as_deref()).await?
        }
        Cmd::Listen { conv, once, timeout, after, interval } => {
            let iv = interval.unwrap_or(ctx.config.defaults.wait_secs);
            let got = commands::msg::listen(&ctx, &conv, once, timeout, after.as_deref(), iv).await?;
            if once && !got {
                return Ok(21); // --once 超时无新消息（§5.4）
            }
        }
        Cmd::Chat { conv, interval } => {
            let iv = interval.unwrap_or(ctx.config.defaults.wait_secs);
            commands::msg::chat(&ctx, &conv, iv).await?
        }
        Cmd::Link { name } => commands::link::link(&name)?,
        Cmd::Unlink { name } => commands::link::unlink(&name)?,
        Cmd::Alias { sub } => match sub {
            AliasCmd::Set { alias, conv } => {
                // 解析引用（前缀/别名）成完整 id 再存
                let id = if conv.len() == 36 {
                    conv
                } else {
                    commands::resolve_conversation(&ctx, &conv).await?
                };
                commands::conv::alias_set(&alias, &id)?
            }
            AliasCmd::List => commands::conv::alias_list(cli.json)?,
            AliasCmd::Remove { alias } => commands::conv::alias_remove(&alias)?,
        },
    }
    Ok(0)
}
