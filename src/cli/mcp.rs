use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rmcp::{
    ErrorData as McpError, Json, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        AnnotateAble, GetPromptRequestParams, GetPromptResult, ListPromptsResult,
        ListResourceTemplatesResult, ListResourcesResult, PaginatedRequestParams, Prompt,
        PromptArgument, PromptMessage, PromptMessageRole, RawResource, RawResourceTemplate,
        ReadResourceRequestParams, ReadResourceResult, ResourceContents, ServerCapabilities,
        ServerInfo,
    },
    schemars, tool, tool_handler, tool_router,
};
use serde_json::{Value, json};

use crate::{self as ng, cli};

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
struct CheckLimitsRequest {
    #[serde(default)]
    account: Option<String>,
    #[serde(default)]
    no_cache: bool,
}

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
struct InvalidateCacheRequest {
    #[serde(default)]
    account: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
struct ToolJsonObject {
    #[serde(flatten)]
    data: HashMap<String, Value>,
}

impl ToolJsonObject {
    fn from_value(value: Value) -> Result<Self, String> {
        let data = value
            .as_object()
            .cloned()
            .ok_or_else(|| "expected JSON object".to_string())?
            .into_iter()
            .collect();
        Ok(Self { data })
    }
}

#[derive(Debug, Clone)]
pub struct VimitMcpServer {
    shared: Arc<Mutex<SharedState>>,
    tool_router: ToolRouter<Self>,
}

#[derive(Debug)]
struct SharedState {
    account_names: Vec<String>,
    current_account_idx: usize,
}

impl VimitMcpServer {
    pub fn new(account_names: Vec<String>) -> Self {
        Self {
            shared: Arc::new(Mutex::new(SharedState {
                account_names,
                current_account_idx: 0,
            })),
            tool_router: Self::tool_router(),
        }
    }

    fn selected_account_name(&self) -> Option<String> {
        let state = self.shared.lock().unwrap();
        state.account_names.get(state.current_account_idx).cloned()
    }

    fn switch_account_name(&self) -> Option<String> {
        let mut state = self.shared.lock().unwrap();
        if state.account_names.is_empty() {
            return None;
        }
        state.current_account_idx = (state.current_account_idx + 1) % state.account_names.len();
        state.account_names.get(state.current_account_idx).cloned()
    }

    fn runtime_args(&self, no_cache: bool) -> cli::args::Args {
        cli::args::Args {
            api_base: None,
            api_key_env: cli::constants::DEFAULT_API_KEY_ENV.to_string(),
            env_file: None,
            demo: false,
            mock: None,
            output: cli::args::OutputMode::Json,
            monitor: false,
            preset: cli::args::Preset::Full,
            theme: cli::theme::Theme::Btop,
            with_abtop: false,
            notify: false,
            watch: 0,
            fail_on: cli::args::FailOn::Never,
            warning_threshold: cli::constants::DEFAULT_WARNING_THRESHOLD,
            danger_threshold: cli::constants::DEFAULT_DANGER_THRESHOLD,
            window_thresholds: HashMap::new(),
            account: self.selected_account_name(),
            list_accounts: false,
            doctor: false,
            init: false,
            mcp: true,
            vpn: false,
            auto_failover: true,
            no_cache,
            trend: false,
            trend_days: 30,
            update: false,
            update_check: false,
            help: false,
            version: false,
            config: None,
            daily_limit: None,
            overlay: false,
        }
    }

    fn load_runtime(
        &self,
        requested_account: Option<&str>,
        no_cache: bool,
    ) -> Result<RuntimeContext, String> {
        let args = self.runtime_args(no_cache);
        let config = cli::config::Config::load(None)?;
        let accounts = cli::accounts::AccountsConfig::load()?;

        let selected_name = requested_account
            .map(ToOwned::to_owned)
            .or_else(|| self.selected_account_name());
        let selected_account = match selected_name.as_deref() {
            Some(name) => Some(accounts.resolve(name)?),
            None => None,
        };

        let api_base = selected_account
            .as_ref()
            .and_then(|account| account.api_base.clone())
            .or_else(|| config.api_base.clone());
        let api_key_env = selected_account
            .as_ref()
            .and_then(|account| account.api_key_env.clone())
            .unwrap_or_else(|| args.api_key_env.clone());

        let mut runtime = ng::RuntimeConfig::from_dotenv(api_base, &api_key_env, None)?;
        runtime.auto_failover = config.auto_failover.unwrap_or(true);

        Ok(RuntimeContext {
            args,
            runtime,
            account_name: selected_name,
        })
    }

    fn status_json(
        &self,
        requested_account: Option<&str>,
        no_cache: bool,
    ) -> Result<Value, String> {
        let ctx = self.load_runtime(requested_account, no_cache)?;
        let http = ng::HttpClient::new(ng::USER_AGENT)?;
        let cache = cli::cache::CacheStore::open()?;
        let mut router = ng::Router::new(
            ctx.runtime.api_base.clone(),
            ng::api_fallbacks_for(&ctx.runtime.api_base, ctx.runtime.auto_failover),
        );
        let mut daily_file = crate::cli::daily::DailyFile::load();
        let snapshot = cli::monitor::collect_status(
            &ctx.args,
            &ctx.runtime,
            &http,
            cache.as_ref(),
            Some(&mut router),
            &mut daily_file,
        )?;
        let mut status = ng::summary_to_json_with_stale(
            &snapshot.windows,
            snapshot.abtop.as_ref(),
            snapshot.daily.as_ref(),
            snapshot.stale,
            snapshot.latency_ms,
            &snapshot.api_endpoint,
        );
        if let Some(map) = status.as_object_mut() {
            map.insert(
                "account".to_string(),
                ctx.account_name
                    .map(Value::String)
                    .unwrap_or(Value::String("default".to_string())),
            );
        }
        Ok(status)
    }

    fn cache_json(&self, requested_account: Option<&str>) -> Result<Value, String> {
        let ctx = self.load_runtime(requested_account, false)?;
        let cache = cli::cache::CacheStore::open()?;
        let entries = match cache {
            Some(store) => store.entries()?,
            None => Vec::new(),
        };
        let entries: Vec<Value> = entries
            .into_iter()
            .filter(|entry| entry.api_base == ctx.runtime.api_base)
            .map(|entry| {
                json!({
                    "cache_key": entry.cache_key,
                    "api_base": entry.api_base,
                    "cached_at": entry.cached_at,
                    "age_secs": entry.age_secs,
                    "payload": entry.payload,
                })
            })
            .collect();
        Ok(json!({
            "account": ctx.account_name.unwrap_or_else(|| "default".to_string()),
            "api_base": ctx.runtime.api_base,
            "entries": entries,
        }))
    }

    fn trends_json(&self, days: u64) -> Result<Value, String> {
        let store = cli::trends::TrendStore::open()?;
        let trends = match store {
            Some(store) => store.query_trends(days)?,
            None => Vec::new(),
        };
        let trends: Vec<Value> = trends
            .into_iter()
            .map(|day| {
                json!({
                    "date": day.date.to_string(),
                    "windows": day.windows.into_iter().map(|window| {
                        json!({
                            "key": window.key,
                            "samples": window.samples,
                            "peak_max": window.peak_max,
                            "peak_avg": window.peak_avg,
                            "credits_avg_used": window.credits_avg_used,
                            "credits_avg_limit": window.credits_avg_limit,
                            "requests_avg_used": window.requests_avg_used,
                            "requests_avg_limit": window.requests_avg_limit,
                        })
                    }).collect::<Vec<_>>()
                })
            })
            .collect();
        Ok(json!({
            "days": days,
            "trends": trends,
        }))
    }
}

impl Default for VimitMcpServer {
    fn default() -> Self {
        let accounts = cli::accounts::AccountsConfig::load()
            .map(|accounts| accounts.list_names())
            .unwrap_or_default();
        Self::new(accounts)
    }
}

struct RuntimeContext {
    args: cli::args::Args,
    runtime: ng::RuntimeConfig,
    account_name: Option<String>,
}

#[tool_router(router = tool_router)]
impl VimitMcpServer {
    #[tool(description = "Run a status check and return JSON matching the CLI JSON output")]
    async fn check_limits(
        &self,
        Parameters(request): Parameters<CheckLimitsRequest>,
    ) -> Result<Json<ToolJsonObject>, String> {
        self.status_json(request.account.as_deref(), request.no_cache)
            .and_then(ToolJsonObject::from_value)
            .map(Json)
    }

    #[tool(description = "Invalidate the cache entry for the selected or requested account")]
    async fn invalidate_cache(
        &self,
        Parameters(request): Parameters<InvalidateCacheRequest>,
    ) -> Result<Json<ToolJsonObject>, String> {
        let ctx = self.load_runtime(request.account.as_deref(), false)?;
        let cache = cli::cache::CacheStore::open()?;
        if let Some(store) = cache {
            store.remove(&ctx.runtime.api_key, &ctx.runtime.api_base)?;
        }
        ToolJsonObject::from_value(json!({
            "ok": true,
            "account": ctx.account_name.unwrap_or_else(|| "default".to_string()),
            "api_base": ctx.runtime.api_base,
        }))
        .map(Json)
    }

    #[tool(
        description = "Rotate to the next configured account and return the selected account name"
    )]
    async fn switch_account(&self) -> Result<Json<ToolJsonObject>, String> {
        ToolJsonObject::from_value(json!({
            "account": self.switch_account_name().unwrap_or_else(|| "default".to_string()),
        }))
        .map(Json)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for VimitMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
        )
        .with_instructions(
            "vimit MCP exposes current quota status, cache contents, trend history, and account helpers.",
        )
    }

    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourcesResult, McpError>> + '_ {
        std::future::ready({
            Ok(ListResourcesResult {
                resources: vec![
                    RawResource::new("vimit://status/current", "Current quota status")
                        .with_description("Current quota status snapshot in vimit JSON form")
                        .no_annotation(),
                    RawResource::new("vimit://cache/current", "Current cache entries")
                        .with_description("Cached API payloads for the active account")
                        .no_annotation(),
                    RawResource::new("vimit://trends/30d", "30 day trends")
                        .with_description("Historical trend samples for the last 30 days")
                        .no_annotation(),
                ],
                ..Default::default()
            })
        })
    }

    fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourceTemplatesResult, McpError>> + '_ {
        std::future::ready({
            Ok(ListResourceTemplatesResult {
                resource_templates: vec![
                    RawResourceTemplate::new("vimit://trends/{days}", "Trend history")
                        .with_description(
                            "Historical trend samples for the requested number of days",
                        )
                        .no_annotation(),
                ],
                ..Default::default()
            })
        })
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ReadResourceResult, McpError>> + '_ {
        std::future::ready((|| {
            let value = if request.uri == "vimit://status/current" {
                self.status_json(None, false)
            } else if request.uri == "vimit://cache/current" {
                self.cache_json(None)
            } else if let Some(days) = request.uri.strip_prefix("vimit://trends/") {
                days.parse::<u64>()
                    .map_err(|error| format!("invalid trend day value '{days}': {error}"))
                    .and_then(|days| self.trends_json(days))
            } else {
                Err(format!("unknown resource: {}", request.uri))
            }
            .map_err(|error| McpError::resource_not_found(error, None))?;

            let text = serde_json::to_string_pretty(&value)
                .map_err(|error| McpError::internal_error(error.to_string(), None))?;
            Ok(ReadResourceResult::new(vec![ResourceContents::text(
                text,
                request.uri,
            )]))
        })())
    }

    fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListPromptsResult, McpError>> + '_ {
        std::future::ready({
            Ok(ListPromptsResult {
                prompts: vec![Prompt::new(
                    "usage_summary",
                    Some("Summarize current vimit quota state for review or planning context."),
                    Some(vec![
                        PromptArgument::new("focus")
                            .with_description("Optional focus such as review, planning, or release")
                            .with_required(false),
                    ]),
                )],
                ..Default::default()
            })
        })
    }

    fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<GetPromptResult, McpError>> + '_ {
        std::future::ready((|| {
            if request.name != "usage_summary" {
                return Err(McpError::invalid_params(
                    format!("unknown prompt: {}", request.name),
                    None,
                ));
            }

            let focus = request
                .arguments
                .as_ref()
                .and_then(|args| args.get("focus"))
                .and_then(Value::as_str)
                .unwrap_or("general");
            let snapshot = self
                .status_json(None, false)
                .map_err(|error| McpError::internal_error(error, None))?;
            let snapshot = serde_json::to_string_pretty(&snapshot)
                .map_err(|error| McpError::internal_error(error.to_string(), None))?;

            Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                PromptMessageRole::User,
                format!(
                    "Use this vimit quota snapshot to produce a concise {focus} summary.\n\n{snapshot}"
                ),
            )]))
        })())
    }
}

pub fn run_mcp() -> Result<i32, String> {
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|error| format!("cannot initialize tokio runtime for MCP: {error}"))?;
    runtime.block_on(async move {
        let accounts = cli::accounts::AccountsConfig::load()?;
        let server = VimitMcpServer::new(accounts.list_names());
        server
            .serve((tokio::io::stdin(), tokio::io::stdout()))
            .await
            .map_err(|error| error.to_string())?
            .waiting()
            .await
            .map_err(|error| error.to_string())?;
        Ok(0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switch_account_cycles_through_configured_accounts() {
        let server = VimitMcpServer::new(vec!["alpha".into(), "beta".into(), "gamma".into()]);
        assert_eq!(server.selected_account_name().as_deref(), Some("alpha"));
        assert_eq!(server.switch_account_name().as_deref(), Some("beta"));
        assert_eq!(server.switch_account_name().as_deref(), Some("gamma"));
        assert_eq!(server.switch_account_name().as_deref(), Some("alpha"));
    }

    #[test]
    fn runtime_args_enable_mcp_mode() {
        let server = VimitMcpServer::new(Vec::new());
        let args = server.runtime_args(true);
        assert!(args.mcp);
        assert!(args.no_cache);
        assert!(matches!(args.output, cli::args::OutputMode::Json));
    }
}
