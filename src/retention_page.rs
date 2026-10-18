use askama::Template;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use time::OffsetDateTime;

use crate::app::AppState;
use crate::auth::Account;
use crate::dashboard::{
    Opt, RangeArgs, RangeMenu, Shell, blocking, group, link, load_ranged, money, plural,
    provider_name,
};
use crate::icons;
use crate::onboarding::render;
use crate::retention::{By, LTV_DAYS, RetentionRow};
use crate::sites;

#[derive(Deserialize, Default)]
pub struct RetentionQuery {
    range: Option<String>,
    from: Option<String>,
    to: Option<String>,
    compare: Option<String>,
    by: Option<String>,
}

/// A figure in the summary strip; tone is blue for customers, orange for revenue.
pub struct Stat {
    name: &'static str,
    value: String,
    sub: String,
    tone: &'static str,
}

pub struct RowView {
    icon: String,
    label: String,
    unattr: bool,
    acquired: String,
    paying: String,
    /// Bar length against the largest row, and the still-paying share of it, both in percent.
    bar: f64,
    fill: f64,
    net: String,
    avg: String,
    refund: String,
    churn: String,
    churn_title: String,
}

#[derive(Template)]
#[template(path = "retention.html")]
struct RetentionPage {
    shell: Shell,
    title: &'static str,
    range: RangeMenu,
    headline: String,
    based: String,
    day_one: bool,
    day_one_note: String,
    stats: Vec<Stat>,
    tabs: Vec<Opt>,
    head: &'static str,
    rows: Vec<RowView>,
    empty: String,
    no_payments: bool,
    status_note: String,
}

const TABS: [(&str, &str, By); 3] = [
    ("channel", "Channel", By::Channel),
    ("referrer", "Referrer", By::Referrer),
    ("campaign", "Campaign", By::Campaign),
];

fn pct(v: Option<f64>) -> String {
    v.map_or_else(|| "n/a".into(), |f| format!("{:.0}%", f * 100.0))
}

fn row_view(r: &RetentionRow, by: By, max: u64) -> RowView {
    let (icon, label) = match (by, &r.key) {
        _ if r.unattributed => (icons::source("Unattributed", 16), r.label.clone()),
        (By::Channel, _) => (icons::source(&r.label, 16), r.label.clone()),
        (By::Referrer, Some(domain)) => (icons::source(domain, 16), r.label.clone()),
        (By::Referrer, None) => (icons::source("Direct", 16), "No referrer".into()),
        (By::Campaign, Some(_)) => (String::new(), r.label.clone()),
        (By::Campaign, None) => (String::new(), "No campaign".into()),
    };
    RowView {
        icon,
        label,
        unattr: r.unattributed,
        acquired: group(r.customers),
        paying: group(r.still_paying),
        bar: if max > 0 {
            r.customers as f64 / max as f64 * 100.0
        } else {
            0.0
        },
        fill: if r.customers > 0 {
            r.still_paying as f64 / r.customers as f64 * 100.0
        } else {
            0.0
        },
        net: money(r.net_ltv_usd),
        avg: r.avg_ltv_usd.map_or_else(|| "n/a".into(), money),
        refund: pct(r.refund_rate),
        churn: pct(r.churn_30d),
        churn_title: if r.churn_customers > 0 {
            format!(
                "{} of {} who started 30 to 60 days ago stopped paying",
                group(r.churned),
                group(r.churn_customers)
            )
        } else {
            "No customers started 30 to 60 days ago".into()
        },
    }
}

fn stats(t: &RetentionRow, day_one: bool) -> Vec<Stat> {
    let customers = Stat {
        name: "Customers acquired",
        value: group(t.customers),
        sub: format!("{} in first payments", money(t.first_payment_usd)),
        tone: "blue",
    };
    let paying = Stat {
        name: "Still paying",
        value: group(t.still_paying),
        sub: match t.customers {
            0 => "No customers yet".into(),
            n => format!("{:.0}% of them", t.still_paying as f64 / n as f64 * 100.0),
        },
        tone: "blue",
    };
    let ltv = Stat {
        name: "Net LTV",
        value: money(t.net_ltv_usd),
        sub: if day_one {
            format!("Needs about {LTV_DAYS} days of payments")
        } else {
            t.avg_ltv_usd
                .map_or_else(String::new, |a| format!("{} per customer", money(a)))
        },
        tone: "orange",
    };
    let refund = Stat {
        name: "Refund rate",
        value: pct(t.refund_rate),
        sub: format!("{} refunded", money(t.refunded_usd)),
        tone: "",
    };
    let churn = Stat {
        name: "30-day churn",
        value: pct(t.churn_30d),
        sub: if t.churn_customers > 0 {
            format!(
                "Of {} who started 30 to 60 days ago",
                plural(t.churn_customers, "customer")
            )
        } else {
            "No customers started 30 to 60 days ago".into()
        },
        tone: "",
    };
    if day_one {
        vec![refund, churn, customers, paying, ltv]
    } else {
        vec![customers, paying, ltv, refund, churn]
    }
}

pub async fn page(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Query(q): Query<RetentionQuery>,
) -> Response {
    let args = RangeArgs {
        range: q.range.clone(),
        from: q.from.clone(),
        to: q.to.clone(),
        compare: q.compare.clone(),
    };
    let (ctx, picked) = match load_ranged(&state, &account, &domain, "retention", args).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };
    let p = picked.period(&ctx.site.timezone);
    let (tab, head, by) = TABS
        .into_iter()
        .find(|t| Some(t.0) == q.by.as_deref())
        .unwrap_or(TABS[0]);
    let store = state.events.clone();
    let id = ctx.site.id.clone();
    let r = match blocking(move || store.retention(&id, &p, OffsetDateTime::now_utc())).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };
    let base = sites::app_path(&ctx.site.domain, "retention");
    let by_q = if tab == "channel" { "" } else { tab };
    let rq = picked.params();
    let mut params: Vec<(&str, &str)> = rq.iter().map(|(k, v)| (*k, v.as_str())).collect();
    params.push(("by", by_q));
    let rows = match by {
        By::Channel => &r.channels,
        By::Referrer => &r.referrers,
        By::Campaign => &r.campaigns,
    };
    let max = rows.iter().map(|x| x.customers).max().unwrap_or(0);
    let missing: Vec<&str> = r
        .status_sources
        .iter()
        .filter(|s| !s.subscription_status)
        .map(|s| provider_name(&s.provider))
        .collect();
    let t = &r.total;
    render(RetentionPage {
        title: "Retention",
        range: picked.menu(ctx.today, ctx.first_day, &base, &[("by", by_q.to_string())]),
        headline: r.headline.clone().unwrap_or_default(),
        based: format!(
            "{} acquired, {} still paying.",
            plural(t.customers, "customer"),
            group(t.still_paying)
        ),
        day_one: r.day_one && t.customers > 0,
        day_one_note: format!(
            "{} of payments so far. Lifetime value needs about {LTV_DAYS} days to mean much, so refund rate and 30-day churn lead for now.",
            plural(r.days_of_data.max(0) as u64, "day")
        ),
        stats: stats(t, r.day_one),
        tabs: TABS
            .iter()
            .map(|(k, label, _)| Opt {
                label: label.to_string(),
                href: link(
                    &base,
                    &params,
                    &[("by", if *k == "channel" { "" } else { k })],
                ),
                on: *k == tab,
                count: String::new(),
            })
            .collect(),
        head,
        rows: rows.iter().map(|x| row_view(x, by, max)).collect(),
        empty: format!(
            "No customers made their first payment in {}.",
            if picked.key == "custom" {
                picked.label.clone()
            } else {
                picked.label.to_lowercase()
            }
        ),
        no_payments: r.status_sources.is_empty(),
        status_note: if missing.is_empty() {
            String::new()
        } else {
            format!(
                "No subscription status from {}, so those customers count as still paying while their last payment is within its billing interval plus 7 days.",
                missing.join(" or ")
            )
        },
        shell: ctx.shell,
    })
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::api::tests::fixture;
    use crate::auth::{hash_token, random_token};

    #[tokio::test]
    async fn renders_the_tabs_and_day_one_note() {
        let f = fixture().await;
        let session = random_token();
        sqlx::query("INSERT INTO sessions (token_hash, account_id, created_at, expires_at) VALUES (?, ?, 0, ?)")
            .bind(hash_token(&session))
            .bind(&f.account)
            .bind(i64::MAX)
            .execute(&f.state.db)
            .await
            .unwrap();
        let get = |uri: &str| {
            let req = Request::get(uri)
                .header("cookie", format!("virling_session={session}"))
                .body(Body::empty())
                .unwrap();
            let app = crate::app::router(f.state.clone());
            async move {
                let res = app.oneshot(req).await.unwrap();
                let status = res.status();
                let bytes = axum::body::to_bytes(res.into_body(), 1 << 22)
                    .await
                    .unwrap();
                (status, String::from_utf8(bytes.to_vec()).unwrap())
            }
        };
        let (code, html) = get("/app/example.com/retention?by=referrer").await;
        assert_eq!(code, StatusCode::OK);
        assert!(html.contains(r#"class="on" aria-current="true">Referrer</a>"#));
        assert!(html.contains("ChatGPT"));
        assert!(html.contains("1 day of payments so far."));
        assert!(html.contains(r#"aria-current="page">"#) && html.contains("Retention</a>"));
        let (code, _) = get("/app/other.com/retention").await;
        assert_eq!(code, StatusCode::NOT_FOUND);
    }
}
