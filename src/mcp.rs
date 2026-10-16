use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::{Value, json};

use crate::api::{
    self, ApiError, BreakdownQuery, DIMENSIONS, KeyAccount, MAX_PAYMENTS, MAX_ROWS, PaymentsQuery,
    RangeQuery,
};
use crate::app::AppState;

/// Newest first; the first is answered when a client asks for one we do not know.
const VERSIONS: [&str; 3] = ["2025-11-25", "2025-06-18", "2025-03-26"];

const INSTRUCTIONS: &str = "Virling links website visits to payments, so every report shows revenue, not only traffic. \
Start with list_sites. Use get_overview for totals and change, get_breakdown for revenue by channel, referrer, campaign, page, country or device, \
get_retention for which channels bring customers who keep paying, list_payments for payments with the visit credited for each, and get_visitor_timeline with a payment's visitor to see what that person did before paying. \
Money is USD net of refunds. Ranges are local dates in the site's timezone: a preset range or from and to.";

/// Answers MCP JSON-RPC over Streamable HTTP with plain JSON responses; no sessions or server-sent streams.
pub async fn post(
    State(state): State<AppState>,
    key: KeyAccount,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(v) = headers
        .get("mcp-protocol-version")
        .and_then(|v| v.to_str().ok())
        && !VERSIONS.contains(&v)
        && v != "2024-11-05"
    {
        return (
            StatusCode::BAD_REQUEST,
            format!("unsupported MCP-Protocol-Version {v}"),
        )
            .into_response();
    }
    let msg: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return Json(error(Value::Null, -32700, "parse error")).into_response(),
    };
    let replies: Vec<Value> = match msg {
        Value::Array(batch) => {
            let mut out = Vec::new();
            for m in batch {
                out.extend(handle(&state, &key.id, m).await);
            }
            if out.is_empty() {
                return StatusCode::ACCEPTED.into_response();
            }
            return Json(Value::Array(out)).into_response();
        }
        m => handle(&state, &key.id, m).await.into_iter().collect(),
    };
    match replies.into_iter().next() {
        Some(r) => Json(r).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// One message's reply; None for notifications and responses.
async fn handle(state: &AppState, account: &str, msg: Value) -> Option<Value> {
    let Some(method) = msg["method"].as_str() else {
        let response =
            msg.get("id").is_some() && (msg.get("result").is_some() || msg.get("error").is_some());
        let id = msg.get("id").cloned().unwrap_or(Value::Null);
        return (!response).then(|| error(id, -32600, "invalid request"));
    };
    let id = msg.get("id")?.clone();
    let params = &msg["params"];
    let result = match method {
        "initialize" => {
            let asked = params["protocolVersion"].as_str().unwrap_or_default();
            let version = VERSIONS
                .iter()
                .find(|v| **v == asked)
                .unwrap_or(&VERSIONS[0]);
            json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "virling", "title": "Virling", "version": env!("CARGO_PKG_VERSION") },
                "instructions": INSTRUCTIONS,
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools() }),
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or_default();
            let args = match &params["arguments"] {
                Value::Null => json!({}),
                a => a.clone(),
            };
            match call(state, account, name, &args).await {
                Some(Ok(v)) => json!({
                    "content": [{ "type": "text", "text": v.to_string() }],
                    "structuredContent": v,
                    "isError": false,
                }),
                Some(Err(ApiError(_, msg))) => json!({
                    "content": [{ "type": "text", "text": msg }],
                    "isError": true,
                }),
                None => return Some(error(id, -32602, &format!("unknown tool {name:?}"))),
            }
        }
        _ => return Some(error(id, -32601, &format!("method not found: {method}"))),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

fn to_value<T: Serialize>(v: T) -> Result<Value, ApiError> {
    serde_json::to_value(v)
        .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "try again".into()))
}

fn args<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T, ApiError> {
    serde_json::from_value(v.clone())
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("invalid arguments: {e}")))
}

/// The `site` argument, or the account's only site.
async fn site(state: &AppState, account: &str, a: &Value) -> Result<String, ApiError> {
    if let Some(s) = a["site"].as_str() {
        return Ok(s.to_string());
    }
    let sites = api::sites_for(state, account).await?;
    match &sites[..] {
        [one] => Ok(one.domain.clone()),
        [] => Err(ApiError(
            StatusCode::NOT_FOUND,
            "this account has no sites".into(),
        )),
        _ => {
            let names: Vec<&str> = sites.iter().map(|s| s.domain.as_str()).collect();
            Err(ApiError(
                StatusCode::BAD_REQUEST,
                format!("pass site, one of: {}", names.join(", ")),
            ))
        }
    }
}

async fn call(
    state: &AppState,
    account: &str,
    name: &str,
    a: &Value,
) -> Option<Result<Value, ApiError>> {
    if !tools().iter().any(|t| t["name"] == name) {
        return None;
    }
    Some(run(state, account, name, a).await)
}

async fn run(state: &AppState, account: &str, name: &str, a: &Value) -> Result<Value, ApiError> {
    if name == "list_sites" {
        return to_value(json!({ "sites": api::sites_for(state, account).await? }));
    }
    let site = site(state, account, a).await?;
    match name {
        "get_overview" => {
            let q: RangeQuery = args(a)?;
            to_value(api::overview_for(state, account, &site, &q).await?)
        }
        "get_breakdown" => {
            let dimension = a["dimension"].as_str().unwrap_or_default();
            let q: BreakdownQuery = args(a)?;
            to_value(api::breakdown_for(state, account, &site, dimension, &q).await?)
        }
        "get_retention" => {
            let q: RangeQuery = args(a)?;
            to_value(api::retention_for(state, account, &site, &q).await?)
        }
        "list_payments" => {
            let q: PaymentsQuery = args(a)?;
            to_value(api::payments_for(state, account, &site, &q).await?)
        }
        _ => {
            let visitor = a["visitor_id"].as_str().ok_or_else(|| {
                ApiError(StatusCode::BAD_REQUEST, "visitor_id is required".into())
            })?;
            to_value(api::visitor_for(state, account, &site, visitor).await?)
        }
    }
}

fn tools() -> Vec<Value> {
    let site = json!({
        "type": "string",
        "description": "Site domain from list_sites, such as example.com. Optional when the account has one site."
    });
    let range = json!({
        "site": site,
        "range": {
            "type": "string",
            "enum": ["today", "7d", "30d", "90d", "12m", "all"],
            "description": "Preset range ending today in the site's timezone. Default 30d, or all for a site tracked 30 days or less. Ignored when from and to are given."
        },
        "from": { "type": "string", "format": "date", "description": "First local date, YYYY-MM-DD. Give with to. Use from and to for calendar months such as last month." },
        "to": { "type": "string", "format": "date", "description": "Last local date, YYYY-MM-DD, inclusive. Give with from." }
    });
    let with = |extra: Value, required: &[&str]| {
        let mut props = range.clone();
        props
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        json!({ "type": "object", "properties": props, "required": required })
    };
    let dims: Vec<&str> = DIMENSIONS.iter().map(|d| d.0).collect();
    let dim_help: Vec<String> = DIMENSIONS
        .iter()
        .map(|d| format!("{}: {}", d.0, d.2))
        .collect();
    let read_only =
        json!({ "readOnlyHint": true, "destructiveHint": false, "openWorldHint": false });
    vec![
        json!({
            "name": "list_sites",
            "title": "List sites",
            "description": "List the sites this API key can read, with their timezones. Call this first to get the site domain for the other tools.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": read_only,
        }),
        json!({
            "name": "get_overview",
            "title": "Get overview",
            "description": "Totals for a date range and for the period of the same length before it: visitors, sessions, payments, customers, revenue_usd (net of refunds), conversion (matched paying customers / visitors) and revenue_per_visitor, plus change as a fraction (0.25 = +25%, null when there is nothing to compare).",
            "inputSchema": with(json!({}), &[]),
            "annotations": read_only,
        }),
        json!({
            "name": "get_breakdown",
            "title": "Get revenue breakdown",
            "description": format!("Visitors, payments, customers, revenue_usd and revenue_per_visitor per value of one dimension, most revenue first. Each payment is credited to one visit by the site's attribution model; payments with no credited visit are in the row with unattributed: true. Dimensions: {}.", dim_help.join("; ")),
            "inputSchema": with(json!({
                "dimension": { "type": "string", "enum": dims, "description": "What to group by." },
                "limit": { "type": "integer", "minimum": 1, "maximum": MAX_ROWS, "description": "Rows to return, default 100." }
            }), &["dimension"]),
            "annotations": read_only,
        }),
        json!({
            "name": "get_retention",
            "title": "Get retention by channel",
            "description": "Which channels bring customers who stay. Customers are grouped by the channel, referrer and campaign credited for their first payment; renewals keep that first channel. The range picks customers whose first payment falls in it. Per row: customers, still_paying (a live subscription per the provider, else a payment not fully refunded within the last billing interval plus 7 days), first_payment_usd, refunded_usd, net_ltv_usd (all their payments so far, net of refunds), avg_ltv_usd, refund_rate (share of customers with any refund), churn_30d (of customers who first paid 30 to 60 days ago, whatever the range, the share with no payment and no live subscription in the last 30 days), share_customers and share_still_paying. day_one is true under 60 days of payments, when LTV is not meaningful yet. headline is the most striking statement, null when data is thin.",
            "inputSchema": with(json!({}), &[]),
            "annotations": read_only,
        }),
        json!({
            "name": "list_payments",
            "title": "List payments",
            "description": "Payments in a date range, newest first. Each row has amount (minor units) and currency, net_usd, status, touch (the credited visit: source such as ChatGPT, channel, referrer_host, campaign, landing_page, country, device; null when unattributed), first_touch (the payer's first visit) and visitor (pass it to get_visitor_timeline). total is the count across pages.",
            "inputSchema": with(json!({
                "filter": { "type": "string", "enum": ["all", "attributed", "unattributed", "refunded"], "description": "Which payments, default all." },
                "limit": { "type": "integer", "minimum": 1, "maximum": MAX_PAYMENTS, "description": "Payments per page, default 20." },
                "offset": { "type": "integer", "minimum": 0, "description": "Payments to skip, for the next page." }
            }), &[]),
            "annotations": read_only,
        }),
        json!({
            "name": "get_visitor_timeline",
            "title": "Get visitor timeline",
            "description": "Everything one visitor did, oldest first. entries are visits (source, channel, campaign, pageviews and steps: page with secs on page, goal, exit with the link clicked out, payment) or standalone payments such as renewals. Use the visitor field of a payment from list_payments.",
            "inputSchema": {
                "type": "object",
                "properties": { "site": site, "visitor_id": { "type": "string", "description": "Visitor ID from a payment's visitor field." } },
                "required": ["visitor_id"]
            },
            "annotations": read_only,
        }),
    ]
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    use super::*;
    use crate::api::tests::fixture;
    use crate::app::router;

    async fn rpc(state: &AppState, key: Option<&str>, body: Value) -> (StatusCode, Value) {
        let mut req = Request::post("/mcp")
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream");
        if let Some(k) = key {
            req = req.header("authorization", format!("Bearer {k}"));
        }
        let res = router(state.clone())
            .oneshot(req.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    fn call(id: u64, name: &str, arguments: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": arguments } })
    }

    #[tokio::test]
    async fn initializes_and_lists_tools() {
        let f = fixture().await;
        let (code, _) = rpc(&f.state, None, json!({})).await;
        assert_eq!(code, StatusCode::UNAUTHORIZED);
        let init = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "t", "version": "1" } } });
        let (code, r) = rpc(&f.state, Some(&f.key), init).await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(r["id"], 1);
        assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(r["result"]["serverInfo"]["name"], "virling");
        assert!(r["result"]["capabilities"]["tools"].is_object());
        let (_, r) = rpc(&f.state, Some(&f.key), json!({ "jsonrpc": "2.0", "id": 2, "method": "initialize", "params": { "protocolVersion": "1999-01-01" } })).await;
        assert_eq!(r["result"]["protocolVersion"], VERSIONS[0]);

        let (code, _) = rpc(
            &f.state,
            Some(&f.key),
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        )
        .await;
        assert_eq!(code, StatusCode::ACCEPTED);

        let (_, r) = rpc(
            &f.state,
            Some(&f.key),
            json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list" }),
        )
        .await;
        let tools = r["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(
            names,
            [
                "list_sites",
                "get_overview",
                "get_breakdown",
                "get_retention",
                "list_payments",
                "get_visitor_timeline"
            ]
        );
        for t in tools {
            assert_eq!(t["inputSchema"]["type"], "object");
            assert_eq!(t["annotations"]["readOnlyHint"], true);
            assert!(t["description"].as_str().unwrap().len() > 40);
        }
        assert_eq!(tools[2]["inputSchema"]["required"], json!(["dimension"]));
        assert_eq!(
            tools[2]["inputSchema"]["properties"]["dimension"]["enum"][0],
            "channel"
        );

        let (_, r) = rpc(
            &f.state,
            Some(&f.key),
            json!({ "jsonrpc": "2.0", "id": 4, "method": "resources/list" }),
        )
        .await;
        assert_eq!(r["error"]["code"], -32601);
        let (_, r) = rpc(
            &f.state,
            Some(&f.key),
            json!({ "jsonrpc": "2.0", "id": 5, "method": "ping" }),
        )
        .await;
        assert_eq!(r["result"], json!({}));
    }

    #[tokio::test]
    async fn calls_tools() {
        let f = fixture().await;
        let (_, r) = rpc(&f.state, Some(&f.key), call(1, "list_sites", json!({}))).await;
        assert_eq!(
            r["result"]["structuredContent"]["sites"][0]["domain"],
            "example.com"
        );

        let (_, r) = rpc(
            &f.state,
            Some(&f.key),
            call(2, "get_overview", json!({ "range": "7d" })),
        )
        .await;
        let res = &r["result"];
        assert_eq!(res["isError"], false);
        assert_eq!(res["structuredContent"]["current"]["revenue_usd"], 49.0);
        let text: Value =
            serde_json::from_str(res["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(text, res["structuredContent"]);

        let (_, r) = rpc(
            &f.state,
            Some(&f.key),
            call(
                3,
                "get_breakdown",
                json!({ "site": "example.com", "dimension": "referrer", "limit": 5 }),
            ),
        )
        .await;
        assert_eq!(
            r["result"]["structuredContent"]["rows"][0]["key"],
            "chatgpt.com"
        );

        let (_, r) = rpc(
            &f.state,
            Some(&f.key),
            call(6, "get_retention", json!({ "range": "30d" })),
        )
        .await;
        let res = &r["result"]["structuredContent"];
        assert_eq!(res["total"]["customers"], 1);
        assert_eq!(res["channels"][0]["key"], "AI");
        assert_eq!(res["referrers"][0]["label"], "ChatGPT");

        let (_, r) = rpc(
            &f.state,
            Some(&f.key),
            call(
                4,
                "list_payments",
                json!({ "filter": "attributed", "limit": 1 }),
            ),
        )
        .await;
        let p = &r["result"]["structuredContent"]["rows"][0];
        assert_eq!(p["touch"]["source"], "ChatGPT");

        let (_, r) = rpc(
            &f.state,
            Some(&f.key),
            call(
                5,
                "get_visitor_timeline",
                json!({ "visitor_id": p["visitor"] }),
            ),
        )
        .await;
        assert_eq!(
            r["result"]["structuredContent"]["entries"][0]["type"],
            "visit"
        );
    }

    #[tokio::test]
    async fn tool_errors_are_results() {
        let f = fixture().await;
        let (_, r) = rpc(
            &f.state,
            Some(&f.key),
            call(1, "get_overview", json!({ "site": "other.com" })),
        )
        .await;
        assert_eq!(r["result"]["isError"], true);
        assert_eq!(r["result"]["content"][0]["text"], "site not found");
        let (_, r) = rpc(
            &f.state,
            Some(&f.key),
            call(2, "get_breakdown", json!({ "dimension": "planet" })),
        )
        .await;
        assert_eq!(r["result"]["isError"], true);
        let (_, r) = rpc(
            &f.state,
            Some(&f.key),
            call(3, "list_payments", json!({ "limit": "many" })),
        )
        .await;
        assert!(
            r["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .starts_with("invalid arguments")
        );
        let (_, r) = rpc(&f.state, Some(&f.key), call(4, "drop_tables", json!({}))).await;
        assert_eq!(r["error"]["code"], -32602);
        let (code, r) = rpc(&f.state, Some(&f.key), json!("not json-rpc")).await;
        assert_eq!(
            (code, r["error"]["code"].as_i64()),
            (StatusCode::OK, Some(-32600))
        );
        let res = router(f.state.clone())
            .oneshot(
                Request::get("/mcp")
                    .header("authorization", format!("Bearer {}", f.key))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::METHOD_NOT_ALLOWED);
    }
}
