// HTTP transport layer (relay-remote-transport Task 2, design D1)。
// streamable HTTP 是唯一 transport：POST /mcp 分派 handle_request、GET /mcp
// 開 SSE session 事件流推 ImpactAlert 通知、GET /health 為無 auth 探測端點。
// stdio 啟動路徑不存在（spec mcp-server：binary 僅 serve）。
// ponytail: axum handler 簽名是協定形狀；允許標準 clippy 集，維持 HTTP 薄層
// 精簡（與 main.rs 同一政策）。
#![allow(clippy::unused_async)]
#![allow(clippy::needless_pass_by_value)]

use crate::types::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use std::sync::mpsc;
use tokio::sync::oneshot;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

/// 分派通道：axum handler 把 JSON-RPC 請求排入佇列，dispatcher 執行緒
/// （單執行緒 std thread，保留 stdio 時代語意與內嵌 plugin 的 Rc 所有權）
/// 循序處理後以 oneshot 回覆。std channel 的 send 是同步非阻塞，handler
/// 側只需 await oneshot receiver。
pub type DispatchTx = mpsc::Sender<(JsonRpcRequest, oneshot::Sender<JsonRpcResponse>)>;

/// `/mcp` 路由共享狀態：分派通道 + SSE 通知匯流排 + 可選 Bearer token。
#[derive(Clone)]
struct Transport {
    dispatch: DispatchTx,
    notify_tx: tokio::sync::broadcast::Sender<serde_json::Value>,
    /// `GRAPHIFY_MCP_TOKEN`；未設定 = 全放行（內網部署信任邊界由部署層把關）。
    token: Option<String>,
}

/// 組出 streamable HTTP router：`/mcp`（POST 分派 + GET SSE）與
/// `/health`（無 auth）。token 設定時 `/mcp` 兩方法皆須
/// `Authorization: Bearer <token>`；`/health` 永遠無 auth（spec 明定）。
pub fn build_router(
    dispatch: DispatchTx,
    notify_tx: tokio::sync::broadcast::Sender<serde_json::Value>,
    token: Option<String>,
) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/mcp", post(post_mcp).get(get_mcp))
        .with_state(Transport {
            dispatch,
            notify_tx,
            token,
        })
}

/// 在專用執行緒的 tokio runtime 內啟動 streamable HTTP server。
/// dispatcher 留在 main 執行緒（內嵌 plugin 是 `!Send` 的 `Rc`），此處只
/// 持有可跨執行緒的 `DispatchTx` / broadcast `Sender`（relay-remote-transport D1）。
pub async fn serve(
    listen: std::net::SocketAddr,
    dispatch: DispatchTx,
    notify_tx: tokio::sync::broadcast::Sender<serde_json::Value>,
    token: Option<String>,
) -> anyhow::Result<()> {
    let app = build_router(dispatch, notify_tx, token);
    let listener = tokio::net::TcpListener::bind(listen).await?;
    eprintln!(
        "graphify-mcp: streamable HTTP listening on http://{listen} (POST /mcp, GET /mcp SSE, GET /health)"
    );
    axum::serve(listener, app).await?;
    Ok(())
}

/// 無 auth、無 payload 的探測端點（nexus 據此判定 upstream 健康）。
async fn health() -> &'static str {
    "ok"
}

async fn post_mcp(State(t): State<Transport>, headers: HeaderMap, body: String) -> Response {
    if !authorized(t.token.as_deref(), &headers) {
        return unauthorized();
    }
    let Ok(request) = serde_json::from_str::<JsonRpcRequest>(&body) else {
        return rpc_error(None, -32700, "parse error: invalid JSON".to_string());
    };
    let is_notification = request.id.is_none();
    let (resp_tx, resp_rx) = oneshot::channel();
    if t.dispatch.send((request, resp_tx)).is_err() {
        return rpc_error(None, -32603, "dispatcher unavailable".to_string());
    }
    let response = match resp_rx.await {
        Ok(response) => response,
        Err(_) => {
            return rpc_error(None, -32603, "dispatcher dropped the response".to_string());
        }
    };
    if is_notification {
        // MCP streamable HTTP：notification（無 id）不攜帶回應本體，
        // 業務副作用已在 dispatcher 執行完畢。
        StatusCode::ACCEPTED.into_response()
    } else {
        Json(response).into_response()
    }
}

/// MCP streamable HTTP session 事件流：server 端主動推送
/// `notifications/review/impact_alert`（dispatcher 執行緒在回應後推入
/// broadcast；T2.3 語意與舊 stdout 順序一致：先回應後通知）。
async fn get_mcp(State(t): State<Transport>, headers: HeaderMap) -> Response {
    if !authorized(t.token.as_deref(), &headers) {
        return unauthorized();
    }
    let rx = t.notify_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|item| {
        item.ok().and_then(|value| {
            serde_json::to_string(&value).ok().map(|s| {
                Ok::<_, std::convert::Infallible>(Event::default().event("message").data(s))
            })
        })
    });
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Bearer token 驗證：token 未設定 = 全放行；設定時請求須帶
/// `Authorization: Bearer <token>`（soundwave 先例：nexus.yaml
/// `headers.Authorization`）。
fn authorized(token: Option<&str>, headers: &HeaderMap) -> bool {
    let Some(expected) = token else {
        return true;
    };
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == format!("Bearer {expected}"))
}

fn unauthorized() -> Response {
    (StatusCode::UNAUTHORIZED, "unauthorized").into_response()
}

fn rpc_error(id: Option<serde_json::Value>, code: i32, message: String) -> Response {
    Json(JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: None,
        error: Some(JsonRpcError { code, message }),
    })
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use tower::ServiceExt; // Router::oneshot

    type TestParts = (
        Router,
        mpsc::Receiver<(JsonRpcRequest, oneshot::Sender<JsonRpcResponse>)>,
        tokio::sync::broadcast::Sender<serde_json::Value>,
    );

    /// 測試 router：Bearer token = "s3cret"。
    fn test_router() -> TestParts {
        let (tx, rx) = mpsc::channel();
        let (ntx, _nrx) = tokio::sync::broadcast::channel(8);
        let app = build_router(tx, ntx.clone(), Some("s3cret".to_string()));
        (app, rx, ntx)
    }

    fn req(
        method: &str,
        uri: &str,
        body: &str,
    ) -> Result<axum::http::Request<Body>, anyhow::Error> {
        Ok(axum::http::Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))?)
    }

    #[tokio::test]
    async fn health_is_unauthenticated() -> anyhow::Result<()> {
        let (app, _rx, _ntx) = test_router();
        let resp = app.oneshot(req("GET", "/health", "")?).await?;
        assert_eq!(resp.status(), StatusCode::OK);
        Ok(())
    }

    #[tokio::test]
    async fn mcp_requires_bearer() -> anyhow::Result<()> {
        let (app, _rx, _ntx) = test_router();
        let resp = app
            .oneshot(req(
                "POST",
                "/mcp",
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
            )?)
            .await?;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        Ok(())
    }

    #[tokio::test]
    async fn mcp_roundtrip_notification_and_sse_notify() -> anyhow::Result<()> {
        let (tx, rx) = mpsc::channel();
        let (ntx, _nrx) = tokio::sync::broadcast::channel(8);
        let app = build_router(tx, ntx.clone(), None);

        // Fake dispatcher：echo method 驗證 transport 層（業務語意由
        // main.rs 的 handle_request 單元測試覆蓋）。dispatcher 是 std
        // thread，與正式執行緒模型一致。
        std::thread::spawn(move || {
            while let Ok((request, resp_tx)) = rx.recv() {
                let response = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: Some(serde_json::json!({ "echo": request.method })),
                    error: None,
                };
                let _ = resp_tx.send(response);
            }
        });

        // initialize round-trip（真實 HTTP 端到端）。
        let resp = app
            .clone()
            .oneshot(req(
                "POST",
                "/mcp",
                r#"{"jsonrpc":"2.0","id":7,"method":"initialize","params":{}}"#,
            )?)
            .await?;
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024).await?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)?;
        assert_eq!(value["result"]["echo"], "initialize");

        // notification（無 id）→ 202 無本體。
        let resp = app
            .clone()
            .oneshot(req(
                "POST",
                "/mcp",
                r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            )?)
            .await?;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);

        // SSE session 事件流：先開流（handler 訂閱後才回 headers），再推
        // 通知，第一個 data frame 必須是該通知（T2.3）。
        let resp = app.oneshot(req("GET", "/mcp", "")?).await?;
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(ct.starts_with("text/event-stream"), "content-type: {ct}");
        ntx.send(serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/review/impact_alert",
            "params": { "note": "e2e" }
        }))?;
        let mut stream = resp.into_body().into_data_stream();
        let first = tokio::time::timeout(std::time::Duration::from_secs(5), stream.next())
            .await
            .map_err(|_| anyhow::anyhow!("SSE first frame timeout"))?
            .ok_or_else(|| anyhow::anyhow!("SSE stream ended early"))??;
        let text = String::from_utf8_lossy(&first);
        assert!(
            text.contains("notifications/review/impact_alert"),
            "frame: {text}"
        );
        Ok(())
    }
}
