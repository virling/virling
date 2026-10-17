use anyhow::{Context, bail};
use clap::{Args, Subcommand};
use serde_json::Value;

use crate::dashboard::{group, native, plural, usd};

pub const ABOUT: &str = "Read revenue analytics from a Virling server";

pub const LONG_ABOUT: &str = "Read revenue analytics from a Virling server through its read API.

Every command calls GET /api/v1 on the server at --url with the API key from --key.
Create a key in the app under Account settings > API keys. Keys are read-only.

Prints tables for people by default. Add --json to print the API response as JSON,
which is what scripts and AI agents should read.

Revenue is net of refunds, in USD. A payment is credited to one visit by the site's
attribution model; payments with no credited visit show as Unattributed.";

pub const EXAMPLES: &str = "Examples:
  export VIRLING_URL=https://stats.example.com VIRLING_API_KEY=vk_...
  virling query sites
  virling query overview --site example.com --range 30d
  virling query channels --from 2026-09-01 --to 2026-09-30
  virling query referrers --json
  virling query payments --limit 20 --filter attributed
  virling query retention --by referrer --range 90d
  virling query visitor vlx3k2a9b7f1c0d4e8

Run `virling query <COMMAND> --help` for each command's options.";

#[derive(Args)]
pub struct QueryArgs {
    /// Virling server URL
    #[arg(
        long,
        env = "VIRLING_URL",
        default_value = "http://localhost:8080",
        global = true,
        display_order = 100
    )]
    url: String,
    /// API key from Account settings > API keys
    #[arg(
        long,
        env = "VIRLING_API_KEY",
        hide_env_values = true,
        global = true,
        display_order = 101
    )]
    key: Option<String>,
    /// Site domain, such as example.com. Optional when the account has one site.
    #[arg(long, env = "VIRLING_SITE", global = true, display_order = 102)]
    site: Option<String>,
    /// Print the API response as JSON instead of a table
    #[arg(long, global = true, display_order = 103)]
    json: bool,
    #[command(subcommand)]
    command: QueryCommand,
}

#[derive(Args, Clone, Default)]
struct RangeArgs {
    /// Preset range ending today in the site's timezone. Default: 30d, or all for a site tracked 30 days or less.
    #[arg(long, value_parser = ["today", "7d", "30d", "90d", "12m", "all"], conflicts_with_all = ["from", "to"])]
    range: Option<String>,
    /// First local date, YYYY-MM-DD; needs --to
    #[arg(long, requires = "to")]
    from: Option<String>,
    /// Last local date, YYYY-MM-DD, inclusive; needs --from
    #[arg(long, requires = "from")]
    to: Option<String>,
}

#[derive(Args, Clone)]
struct BreakdownArgs {
    #[command(flatten)]
    range: RangeArgs,
    /// Rows to show, most revenue first
    #[arg(long, default_value_t = 20)]
    limit: usize,
}

/// A breakdown command's after-help: what the columns mean, then examples.
macro_rules! breakdown_help {
    ($examples:literal) => {
        concat!(
            "Columns: visitors (visits that started in the range), payments and revenue\n",
            "credited to the row, and revenue per visitor. Sorted by revenue, then visitors.\n",
            "The Unattributed row holds payments with no credited visit.\n\nExamples:\n",
            $examples
        )
    };
}

#[derive(Subcommand)]
enum QueryCommand {
    /// List the sites this key can read
    #[command(after_help = "Examples:\n  virling query sites\n  virling query sites --json")]
    Sites,
    /// Visitors, revenue, conversion and revenue per visitor, with change vs the previous period
    #[command(
        long_about = "Visitors, revenue, payments, conversion and revenue per visitor for the range,
next to the period of the same length just before it, and the change between them.

Conversion is paying customers matched to a tracked visitor divided by visitors.
Revenue per visitor is matched revenue divided by visitors.",
        after_help = "Examples:\n  virling query overview --site example.com\n  virling query overview --range 7d\n  virling query overview --from 2026-09-01 --to 2026-09-30 --json"
    )]
    Overview(RangeArgs),
    /// Revenue by channel: Search, Communities, Social, AI, Email, Paid, Referral, Direct
    #[command(
        after_help = breakdown_help!("  virling query channels\n  virling query channels --range 90d --json")
    )]
    Channels(BreakdownArgs),
    /// Revenue by referring domain, such as chatgpt.com or news.ycombinator.com
    #[command(
        after_help = breakdown_help!("  virling query referrers --limit 10\n  virling query referrers --from 2026-09-01 --to 2026-09-30")
    )]
    Referrers(BreakdownArgs),
    /// Revenue by utm_campaign
    #[command(
        after_help = breakdown_help!("  virling query campaigns --range 30d")
    )]
    Campaigns(BreakdownArgs),
    /// Revenue by the first page of each visit
    #[command(
        after_help = breakdown_help!("  virling query landing-pages --limit 10")
    )]
    LandingPages(BreakdownArgs),
    /// Revenue by every page viewed; a payment counts for each page of its credited visit
    #[command(
        after_help = breakdown_help!("  virling query pages --limit 10")
    )]
    Pages(BreakdownArgs),
    /// Revenue by visitor country (two-letter code)
    #[command(
        after_help = breakdown_help!("  virling query countries")
    )]
    Countries(BreakdownArgs),
    /// Revenue by device: Desktop, Mobile, Tablet
    #[command(
        after_help = breakdown_help!("  virling query devices")
    )]
    Devices(BreakdownArgs),
    /// Revenue by any dimension, including keyword, browser and os
    #[command(
        after_help = breakdown_help!("  virling query breakdown keyword\n  virling query breakdown browser --json")
    )]
    Breakdown {
        #[arg(value_parser = crate::api::DIMENSIONS.map(|d| d.0))]
        dimension: String,
        #[command(flatten)]
        args: BreakdownArgs,
    },
    /// Which channels bring customers who keep paying: LTV, refunds and churn by first channel
    #[command(
        long_about = "Customers whose first payment falls in the range, grouped by the channel,
referrer or campaign credited for that first payment. Renewals keep the first channel.

Still paying: a live subscription per the provider, else a payment not fully refunded
within the last billing interval plus 7 days. Net LTV is every payment so far, net of
refunds. Refund rate is the share of customers with any refund. 30-day churn looks at
customers who first paid 30 to 60 days ago, whatever the range: the share with no
payment and no live subscription in the last 30 days.",
        after_help = "Examples:\n  virling query retention\n  virling query retention --by referrer --range 90d\n  virling query retention --by campaign --json"
    )]
    Retention {
        #[command(flatten)]
        range: RangeArgs,
        /// Group customers by their first payment's channel, referrer or campaign
        #[arg(long, default_value = "channel", value_parser = ["channel", "referrer", "campaign"])]
        by: String,
        /// Rows to show, most customers first
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Payments, newest first, with the visit credited for each
    #[command(
        long_about = "Payments in the range, newest first, with the source and campaign of the visit
credited for each, and the visitor ID to pass to `virling query visitor`.",
        after_help = "Examples:\n  virling query payments --limit 20\n  virling query payments --filter unattributed --range 90d\n  virling query payments --limit 50 --offset 50 --json"
    )]
    Payments {
        #[command(flatten)]
        range: RangeArgs,
        /// Which payments to list
        #[arg(long, default_value = "all", value_parser = ["all", "attributed", "unattributed", "refunded"])]
        filter: String,
        /// Payments per page, up to 200
        #[arg(long, default_value_t = 20)]
        limit: u32,
        /// Payments to skip, for the next page
        #[arg(long, default_value_t = 0)]
        offset: u32,
    },
    /// Everything one visitor did: visits, pages, goals, exits and payments
    #[command(
        long_about = "Every visit of one visitor, oldest first, with the pages they saw, goals they
completed, where they left and the payments they made. Find visitor IDs in the
Visitor column of `virling query payments`.",
        after_help = "Examples:\n  virling query visitor vlx3k2a9b7f1c0d4e8\n  virling query visitor vlx3k2a9b7f1c0d4e8 --json"
    )]
    Visitor {
        /// Visitor ID
        id: String,
    },
}

struct Client {
    http: reqwest::Client,
    base: url::Url,
    key: String,
}

impl Client {
    async fn get(&self, segments: &[&str], query: &[(&str, String)]) -> anyhow::Result<Value> {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("--url must be an http or https URL"))?
            .pop_if_empty()
            .extend(["api", "v1"])
            .extend(segments);
        let res = self
            .http
            .get(url.clone())
            .bearer_auth(&self.key)
            .query(query)
            .send()
            .await
            .with_context(|| format!("could not reach {}", self.base))?;
        let status = res.status();
        let body: Value = res.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            let msg = body["error"].as_str().unwrap_or("unexpected response");
            let hint = match status.as_u16() {
                401 => {
                    "\nCreate a key in Account settings > API keys and pass it with --key or VIRLING_API_KEY."
                }
                404 if body.is_null() => {
                    "\nThis server has no read API. Check --url, and that it runs with --app."
                }
                _ => "",
            };
            bail!("{status}: {msg}{hint}");
        }
        Ok(body)
    }

    /// The --site domain, or the account's only site.
    async fn site(&self, site: Option<String>) -> anyhow::Result<String> {
        if let Some(s) = site {
            return Ok(s);
        }
        let body = self.get(&["sites"], &[]).await?;
        let domains: Vec<&str> = body["sites"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s["domain"].as_str())
            .collect();
        match domains[..] {
            [one] => Ok(one.to_string()),
            [] => bail!("this account has no sites"),
            _ => bail!("pass --site, one of: {}", domains.join(", ")),
        }
    }
}

fn range_query(r: &RangeArgs) -> Vec<(&'static str, String)> {
    let mut q = Vec::new();
    for (k, v) in [("range", &r.range), ("from", &r.from), ("to", &r.to)] {
        if let Some(v) = v {
            q.push((k, v.clone()));
        }
    }
    q
}

/// Runs a query command and returns what to print.
pub async fn run(args: QueryArgs) -> anyhow::Result<String> {
    let key = args.key.clone().filter(|k| !k.trim().is_empty()).context(
        "no API key: create one in Account settings > API keys, then pass --key or set VIRLING_API_KEY",
    )?;
    let base = url::Url::parse(args.url.trim()).context("--url is not a valid URL")?;
    let client = Client {
        http: reqwest::Client::builder()
            .user_agent(concat!("virling-cli/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(60))
            .build()?,
        base,
        key,
    };
    let (body, table): (Value, fn(&Value) -> String) = match args.command {
        QueryCommand::Sites => (client.get(&["sites"], &[]).await?, sites_table),
        QueryCommand::Overview(range) => {
            let site = client.site(args.site).await?;
            (
                client
                    .get(&["sites", &site, "overview"], &range_query(&range))
                    .await?,
                overview_table,
            )
        }
        QueryCommand::Payments {
            range,
            filter,
            limit,
            offset,
        } => {
            let site = client.site(args.site).await?;
            let mut q = range_query(&range);
            q.extend([
                ("filter", filter),
                ("limit", limit.to_string()),
                ("offset", offset.to_string()),
            ]);
            (
                client.get(&["sites", &site, "payments"], &q).await?,
                payments_table,
            )
        }
        QueryCommand::Retention { range, by, limit } => {
            let site = client.site(args.site).await?;
            let body = client
                .get(&["sites", &site, "retention"], &range_query(&range))
                .await?;
            return Ok(if args.json {
                serde_json::to_string_pretty(&body)? + "\n"
            } else {
                retention_table(&body, &by, limit)
            });
        }
        QueryCommand::Visitor { id } => {
            let site = client.site(args.site).await?;
            (
                client.get(&["sites", &site, "visitors", &id], &[]).await?,
                visitor_table,
            )
        }
        cmd => {
            let (dimension, b) = match cmd {
                QueryCommand::Channels(b) => ("channel".to_string(), b),
                QueryCommand::Referrers(b) => ("referrer".to_string(), b),
                QueryCommand::Campaigns(b) => ("campaign".to_string(), b),
                QueryCommand::LandingPages(b) => ("landing_page".to_string(), b),
                QueryCommand::Pages(b) => ("page".to_string(), b),
                QueryCommand::Countries(b) => ("country".to_string(), b),
                QueryCommand::Devices(b) => ("device".to_string(), b),
                QueryCommand::Breakdown { dimension, args } => (dimension, args),
                _ => unreachable!("handled above"),
            };
            let site = client.site(args.site).await?;
            let mut q = range_query(&b.range);
            q.push(("limit", b.limit.to_string()));
            (
                client
                    .get(&["sites", &site, "breakdown", &dimension], &q)
                    .await?,
                breakdown_table,
            )
        }
    };
    Ok(if args.json {
        serde_json::to_string_pretty(&body)? + "\n"
    } else {
        table(&body)
    })
}

/// Columns padded to their widest cell; columns after the first are right-aligned when `right` says so.
fn table(head: &[&str], right: &[bool], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = head.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (i, c) in row.iter().enumerate() {
            widths[i] = widths[i].max(c.chars().count());
        }
    }
    let line = |cells: Vec<&str>| {
        let parts: Vec<String> = cells
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if right.get(i).copied().unwrap_or(false) {
                    format!("{c:>w$}", w = widths[i])
                } else {
                    format!("{c:<w$}", w = widths[i])
                }
            })
            .collect();
        parts.join("  ").trim_end().to_string() + "\n"
    };
    let mut out = line(head.to_vec());
    for row in rows {
        out += &line(row.iter().map(String::as_str).collect());
    }
    out
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "-".into(),
        other => other.to_string(),
    }
}

fn count(v: &Value) -> String {
    v.as_u64().map_or_else(|| "-".into(), group)
}

fn dollars(v: &Value) -> String {
    v.as_f64().map_or_else(|| "-".into(), usd)
}

fn percent(v: &Value) -> String {
    v.as_f64()
        .map_or_else(|| "-".into(), |f| format!("{:.2}%", f * 100.0))
}

fn change(v: &Value) -> String {
    v.as_f64()
        .map_or_else(|| "-".into(), |f| format!("{:+.1}%", f * 100.0))
}

/// "2026-10-15 09:12" from an RFC 3339 UTC time.
fn minute(v: &Value) -> String {
    let s = text(v);
    s.get(..16).map_or(s.clone(), |m| m.replace('T', " "))
}

fn range_line(body: &Value) -> String {
    let r = &body["range"];
    format!(
        "{}  {} to {} ({})\n\n",
        text(&body["site"]),
        text(&r["from"]),
        text(&r["to"]),
        text(&r["timezone"])
    )
}

fn sites_table(body: &Value) -> String {
    let rows: Vec<Vec<String>> = body["sites"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|s| vec![text(&s["domain"]), text(&s["timezone"])])
        .collect();
    if rows.is_empty() {
        return "No sites.\n".into();
    }
    table(&["Site", "Timezone"], &[], &rows)
}

fn overview_table(body: &Value) -> String {
    let (c, p, d) = (&body["current"], &body["previous"], &body["change"]);
    type Fmt = fn(&Value) -> String;
    let metrics: [(&str, &str, Fmt); 5] = [
        ("Visitors", "visitors", count),
        ("Revenue", "revenue_usd", dollars),
        ("Payments", "payments", count),
        ("Conversion", "conversion", percent),
        ("Revenue per visitor", "revenue_per_visitor", dollars),
    ];
    let rows: Vec<Vec<String>> = metrics
        .iter()
        .map(|(label, key, f)| vec![label.to_string(), f(&c[key]), f(&p[key]), change(&d[key])])
        .collect();
    let pr = &body["previous_range"];
    let mut out = range_line(body);
    out += &table(
        &["Metric", "Value", "Previous", "Change"],
        &[false, true, true, true],
        &rows,
    );
    out += &format!(
        "\nPrevious period: {} to {}. {} of {} payments matched a tracked visitor.\n",
        text(&pr["from"]),
        text(&pr["to"]),
        count(&c["matched_payments"]),
        count(&c["payments"])
    );
    out
}

fn breakdown_table(body: &Value) -> String {
    let rows: Vec<Vec<String>> = body["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| {
            vec![
                text(&r["label"]),
                count(&r["visitors"]),
                count(&r["payments"]),
                dollars(&r["revenue_usd"]),
                dollars(&r["revenue_per_visitor"]),
            ]
        })
        .collect();
    let mut out = range_line(body);
    if rows.is_empty() {
        return out + "No data in this range.\n";
    }
    let name = text(&body["dimension"]).replace('_', " ");
    let mut head = name.clone();
    head[..1].make_ascii_uppercase();
    out += &table(
        &[&head, "Visitors", "Payments", "Revenue", "Per visitor"],
        &[false, true, true, true, true],
        &rows,
    );
    let total = body["total"].as_u64().unwrap_or(0);
    if total > rows.len() as u64 {
        out += &format!(
            "\nShowing {} of {total} rows. Use --limit for more.\n",
            rows.len()
        );
    }
    out
}

fn retention_table(body: &Value, by: &str, limit: usize) -> String {
    let key = match by {
        "referrer" => "referrers",
        "campaign" => "campaigns",
        _ => "channels",
    };
    let all = body[key].as_array().map_or(&[][..], |v| v);
    let rows: Vec<Vec<String>> = all
        .iter()
        .take(limit)
        .map(|r| {
            vec![
                text(&r["label"]),
                count(&r["customers"]),
                count(&r["still_paying"]),
                dollars(&r["net_ltv_usd"]),
                dollars(&r["avg_ltv_usd"]),
                percent(&r["refund_rate"]),
                percent(&r["churn_30d"]),
            ]
        })
        .collect();
    let mut out = range_line(body);
    if let Some(h) = body["headline"].as_str() {
        out += &format!("{h}\n\n");
    }
    if rows.is_empty() {
        return out + "No customers acquired in this range.\n";
    }
    let mut head = by.to_string();
    head[..1].make_ascii_uppercase();
    out += &table(
        &[
            &head,
            "Customers",
            "Still paying",
            "Net LTV",
            "Avg LTV",
            "Refund rate",
            "30-day churn",
        ],
        &[false, true, true, true, true, true, true],
        &rows,
    );
    if all.len() > rows.len() {
        out += &format!(
            "\nShowing {} of {} rows. Use --limit for more.\n",
            rows.len(),
            all.len()
        );
    }
    if body["day_one"].as_bool() == Some(true) {
        out += &format!(
            "\nOnly {} of payments so far. LTV needs about {} days to mean much; refunds and churn count now.\n",
            plural(body["days_of_data"].as_u64().unwrap_or(0), "day"),
            crate::retention::LTV_DAYS
        );
    }
    out
}

fn payment_amount(p: &Value) -> String {
    match (p["amount"].as_i64(), p["currency"].as_str()) {
        (Some(a), Some(c)) => native(a, c),
        _ => "-".into(),
    }
}

fn payments_table(body: &Value) -> String {
    let rows: Vec<Vec<String>> = body["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| {
            let t = &p["touch"];
            vec![
                minute(&p["ts"]),
                payment_amount(p),
                dollars(&p["net_usd"]),
                if t.is_null() {
                    "Unattributed".into()
                } else {
                    text(&t["source"])
                },
                text(&t["campaign"]),
                text(&p["status"]),
                text(&p["visitor"]),
            ]
        })
        .collect();
    let mut out = range_line(body);
    if rows.is_empty() {
        return out + "No payments in this range.\n";
    }
    out += &table(
        &[
            "Time (UTC)",
            "Amount",
            "Net USD",
            "Source",
            "Campaign",
            "Status",
            "Visitor",
        ],
        &[false, true, true],
        &rows,
    );
    let (total, offset) = (
        body["total"].as_u64().unwrap_or(0),
        body["offset"].as_u64().unwrap_or(0),
    );
    let last = offset + rows.len() as u64;
    out += &format!("\nPayments {}-{last} of {total}.", offset + 1);
    if last < total {
        out += &format!(" Next page: --offset {last}");
    }
    out + "\n"
}

fn step_line(s: &Value) -> String {
    let at = text(&s["ts"]);
    let at = at.get(11..16).unwrap_or("");
    let what = match s["type"].as_str() {
        Some("page") => {
            let secs = s["secs"]
                .as_i64()
                .map(|n| format!(" ({}m {}s)", n / 60, n % 60))
                .unwrap_or_default();
            format!("page     {}{secs}", text(&s["path"]))
        }
        Some("goal") => format!("goal     {}", text(&s["name"])),
        Some("exit") => match s["link"].as_str() {
            Some(link) => format!("exit     {} -> {link}", text(&s["page"])),
            None => format!("exit     {}", text(&s["page"])),
        },
        Some("payment") => format!("payment  {}", payment_line(s)),
        _ => text(&s["type"]),
    };
    format!("  {at}  {what}\n")
}

fn payment_line(p: &Value) -> String {
    let renewal = if p["is_renewal"].as_bool() == Some(true) {
        ", renewal"
    } else {
        ""
    };
    format!(
        "{} ({}, {}{renewal})",
        payment_amount(p),
        text(&p["provider"]),
        text(&p["status"])
    )
}

fn visitor_table(body: &Value) -> String {
    let mut out = format!(
        "Visitor {} on {} ({}, {}, {})\n\n",
        text(&body["visitor_id"]),
        text(&body["site"]),
        text(&body["country"]),
        text(&body["browser"]),
        text(&body["device"])
    );
    for e in body["entries"].as_array().into_iter().flatten() {
        match e["type"].as_str() {
            Some("visit") => {
                let channel = text(&e["channel"]);
                let source = text(&e["source"]);
                let from = if source == channel {
                    source
                } else {
                    format!("{source} ({channel})")
                };
                let campaign = e["campaign"]
                    .as_str()
                    .map(|c| format!(", campaign {c}"))
                    .unwrap_or_default();
                out += &format!(
                    "{} UTC  Visit from {from}{campaign}, {}\n",
                    minute(&e["start"]),
                    plural(e["pageviews"].as_u64().unwrap_or(0), "page")
                );
                for s in e["steps"].as_array().into_iter().flatten() {
                    out += &step_line(s);
                }
            }
            _ => {
                out += &format!("{} UTC  Payment {}\n", minute(&e["ts"]), payment_line(e));
            }
        }
    }
    let pays = body["payments"].as_array().map_or(&[][..], |v| v);
    let net: f64 = pays.iter().filter_map(|p| p["net_usd"].as_f64()).sum();
    out += &format!(
        "\nPaid {} in {}.\n",
        usd(net),
        plural(pays.len() as u64, "payment")
    );
    out
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use clap::Parser;

    use super::*;
    use crate::api::tests::fixture;
    use crate::{Cli, Command};

    /// Serves the fixture on a local port and returns its URL and key.
    async fn server() -> (crate::api::tests::Fixture, String) {
        let f = fixture().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let app =
            crate::app::router(f.state.clone()).into_make_service_with_connect_info::<SocketAddr>();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (f, url)
    }

    async fn query(url: &str, key: &str, args: &[&str]) -> anyhow::Result<String> {
        let mut argv = vec!["virling", "query", "--url", url, "--key", key];
        argv.extend(args);
        match Cli::try_parse_from(argv)?.command {
            Some(Command::Query(q)) => run(q).await,
            _ => unreachable!(),
        }
    }

    #[tokio::test]
    async fn prints_tables_and_json() {
        let (f, url) = server().await;
        let out = query(&url, &f.key, &["sites"]).await.unwrap();
        assert_eq!(out, "Site         Timezone\nexample.com  UTC\n");

        let out = query(&url, &f.key, &["overview", "--range", "7d"])
            .await
            .unwrap();
        assert!(out.starts_with("example.com  "), "{out}");
        assert!(
            out.contains("Revenue              $49.00     $0.00       -\n"),
            "{out}"
        );
        assert!(
            out.contains("Conversion           50.00%         -       -\n"),
            "{out}"
        );

        let out = query(&url, &f.key, &["channels", "--site", "example.com"])
            .await
            .unwrap();
        assert!(
            out.ends_with("Channel  Visitors  Payments  Revenue  Per visitor\nAI              2         1   $49.00       $24.50\n"),
            "{out}"
        );
        let v: Value = serde_json::from_str(
            &query(&url, &f.key, &["breakdown", "browser", "--json"])
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(v["dimension"], "browser");
        assert_eq!(v["rows"][0]["key"], "Firefox");

        let out = query(&url, &f.key, &["payments", "--limit", "5"])
            .await
            .unwrap();
        assert!(
            out.contains("$49   $49.00  ChatGPT  launch    paid    v1\n"),
            "{out}"
        );
        assert!(out.ends_with("Payments 1-1 of 1.\n"), "{out}");

        let out = query(&url, &f.key, &["retention", "--by", "referrer"])
            .await
            .unwrap();
        assert!(
            out.contains("Referrer  Customers  Still paying  Net LTV  Avg LTV  Refund rate  30-day churn\nChatGPT           1             1   $49.00   $49.00        0.00%             -\n"),
            "{out}"
        );
        assert!(out.ends_with("Only 1 day of payments so far. LTV needs about 60 days to mean much; refunds and churn count now.\n"), "{out}");
        let v: Value =
            serde_json::from_str(&query(&url, &f.key, &["retention", "--json"]).await.unwrap())
                .unwrap();
        assert_eq!(v["channels"][0]["customers"], 1);

        let out = query(&url, &f.key, &["visitor", "v1"]).await.unwrap();
        assert!(
            out.contains("Visit from ChatGPT (AI), campaign launch, 2 pages\n"),
            "{out}"
        );
        assert!(out.contains("  page     / (1m 0s)\n"), "{out}");
        assert!(out.contains("payment  $49 (stripe, paid)\n"), "{out}");
        assert!(out.ends_with("Paid $49.00 in 1 payment.\n"), "{out}");
    }

    #[tokio::test]
    async fn explains_failures() {
        let (f, url) = server().await;
        let err = query(&url, "vk_wrong", &["sites"]).await.unwrap_err();
        assert!(
            err.to_string().contains("Account settings > API keys"),
            "{err}"
        );
        let err = query(&url, &f.key, &["overview", "--site", "other.com"])
            .await
            .unwrap_err();
        assert!(err.to_string().contains("site not found"), "{err}");
        let err = query(&url, &f.key, &["visitor", "nobody"])
            .await
            .unwrap_err();
        assert!(err.to_string().contains("visitor not found"), "{err}");
        assert!(
            query(&url, &f.key, &["overview", "--from", "2026-01-01"])
                .await
                .is_err()
        );
        assert!(query(&url, &f.key, &["breakdown", "planet"]).await.is_err());
        let err = query(&url, " ", &["sites"]).await.unwrap_err();
        assert!(err.to_string().contains("no API key"), "{err}");
    }

    #[test]
    fn every_command_has_examples() {
        use clap::CommandFactory;
        let cli = Cli::command();
        let query = cli.find_subcommand("query").unwrap();
        for sub in query.get_subcommands().filter(|s| s.get_name() != "help") {
            let help = sub
                .get_after_help()
                .map(|h| h.to_string())
                .unwrap_or_default();
            assert!(
                help.contains("virling query"),
                "{} has no examples",
                sub.get_name()
            );
        }
    }
}
