use crate::auth::bearer_token_from_context;
use crate::clients::hivemind::HivemindClient;
use crate::clients::vexa::VexaClient;
use crate::config::Config;
use crate::error::error_result;
use crate::tools;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, GetPromptRequestParams, GetPromptResult,
    ListPromptsResult, ListToolsResult, PaginatedRequestParams, ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use std::sync::Arc;

#[derive(Clone)]
pub struct KiokuMcpService {
    pub http: reqwest::Client,
    pub config: Arc<Config>,
    pub hivemind: HivemindClient,
    pub vexa: VexaClient,
}

impl ServerHandler for KiokuMcpService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::default()
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, ErrorData>>
           + rmcp::service::MaybeSendFuture
           + '_ {
        async move {
            Ok(ListToolsResult {
                tools: tools::all_tools(),
                meta: None,
                next_cursor: None,
            })
        }
    }

    fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListPromptsResult, ErrorData>>
           + rmcp::service::MaybeSendFuture
           + '_ {
        async move {
            Ok(ListPromptsResult {
                prompts: tools::prompts::all_prompts(),
                next_cursor: None,
                meta: None,
            })
        }
    }

    fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<GetPromptResult, ErrorData>>
           + rmcp::service::MaybeSendFuture
           + '_ {
        async move { tools::prompts::dispatch(request) }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, ErrorData>>
           + rmcp::service::MaybeSendFuture
           + '_ {
        async move {
            let name = request.name.clone();
            let args = request.arguments.as_ref();
            let Some(token) = bearer_token_from_context(&context) else {
                return Ok(error_result(
                    "Missing credentials (send Authorization: Bearer <token>).",
                ));
            };
            tools::dispatch(self, &name, args, &token).await
        }
    }
}
