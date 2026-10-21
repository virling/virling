//! The AI card's Answers tab and the drill-down per assistant.

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use sqlx::SqlitePool;
use time::macros::format_description;
use time::{Date, Duration};

use super::cards::ai_company;
use super::{Shell, blocking, group, load, money, plural};
use crate::answers::{self, Assistant, Tally};
use crate::app::AppState;
use crate::auth::Account;
use crate::crawlers::{self, BotHits, Kind, by_name};
use crate::onboarding::render;
use crate::sites::app_path;
use crate::stats::{BreakdownRow, Dimension, Period};

/// Days since 1970 of a date, the unit answers are stored by.
pub fn day(d: Date) -> i64 {
    i64::from(d.to_julian_day()) - 2_440_588
}

/// What the Answers tab and the drill-down read for one range, all from SQLite.
pub struct Inputs {
    pub keyed: Vec<Assistant>,
    pub now: Vec<(String, Tally)>,
    pub prev: Vec<(String, Tally)>,
    /// (provider, page, answers citing it)
    pub cited: Vec<(String, String, i64)>,
    /// (answer-time bot, path, hits)
    pub pages: Vec<(String, String, i64)>,
}

impl Inputs {
    /// Local dates `from` to `to`, and the same range as unix times for crawler hits.
    pub async fn load(
        db: &SqlitePool,
        site_id: &str,
        (from, to): (Date, Date),
        (from_ts, to_ts): (i64, i64),
    ) -> sqlx::Result<Self> {
        let (a, b) = (day(from), day(to));
        let len = b - a + 1;
        Ok(Inputs {
            keyed: answers::keyed(db, site_id).await?,
            now: answers::tallies(db, site_id, a, b).await?,
            prev: answers::tallies(db, site_id, a - len, a - 1).await?,
            cited: answers::cited(db, site_id, a, b).await?,
            pages: crawlers::answer_pages(db, site_id, from_ts, to_ts).await?,
        })
    }
}

/// One assistant's numbers in a range.
struct Numbers {
    reads: i64,
    now: Tally,
    prev: Tally,
    cited: Vec<(String, i64)>,
    opened: Vec<(String, i64)>,
    visitors: u64,
    revenue: f64,
}

fn answer_bot_of(bot: &str, a: Assistant) -> bool {
    by_name(bot).is_some_and(|c| c.kind == Kind::Answer && c.company == a.company())
}

fn numbers(a: Assistant, i: &Inputs, bots: &[BotHits], sources: &[BreakdownRow]) -> Numbers {
    let tally = |rows: &[(String, Tally)]| {
        rows.iter()
            .find(|r| r.0 == a.as_str())
            .map(|r| r.1.clone())
            .unwrap_or_default()
    };
    let mut opened: Vec<(String, i64)> = Vec::new();
    for (_, path, hits) in i.pages.iter().filter(|p| answer_bot_of(&p.0, a)) {
        match opened.iter_mut().find(|o| &o.0 == path) {
            Some(o) => o.1 += hits,
            None => opened.push((path.clone(), *hits)),
        }
    }
    opened.sort_by(|x, y| y.1.cmp(&x.1).then(x.0.cmp(&y.0)));
    let referred = sources.iter().filter(|r| {
        r.key
            .as_deref()
            .and_then(|k| k.strip_prefix("AI|"))
            .is_some_and(|h| !h.is_empty() && ai_company(h).0 == a.company())
    });
    Numbers {
        reads: bots
            .iter()
            .filter(|b| answer_bot_of(&b.bot, a))
            .map(|b| b.hits)
            .sum(),
        now: tally(&i.now),
        prev: tally(&i.prev),
        cited: i
            .cited
            .iter()
            .filter(|c| c.0 == a.as_str())
            .map(|c| (c.1.clone(), c.2))
            .collect(),
        opened,
        visitors: referred.clone().map(|r| r.visitors).sum(),
        revenue: referred.map(|r| r.revenue_usd).sum(),
    }
}

fn of(t: &Tally) -> String {
    format!("{} of {}", t.mentioned, t.answers)
}

/// "was 4 of 20" and its tone when the share named moved 5 points or more.
fn was(now: &Tally, prev: &Tally) -> (String, &'static str) {
    if now.answers == 0 || prev.answers == 0 {
        return (String::new(), "");
    }
    let rate = |t: &Tally| t.mentioned as f64 / t.answers as f64;
    let d = rate(now) - rate(prev);
    let tone = if d >= 0.05 {
        "up"
    } else if d <= -0.05 {
        "down"
    } else {
        ""
    };
    (format!("was {}", of(prev)), tone)
}

fn pages_line(n: &Numbers) -> String {
    let (verb, pages) = if n.cited.is_empty() {
        ("Opened", &n.opened)
    } else {
        ("Cites", &n.cited)
    };
    let top: Vec<&str> = pages.iter().take(3).map(|p| p.0.as_str()).collect();
    if top.is_empty() {
        String::new()
    } else {
        format!("{verb} {}", top.join(", "))
    }
}

pub struct AnswerRow {
    pub(super) icon: String,
    pub(super) name: &'static str,
    pub(super) sub: String,
    pub(super) href: String,
    pub(super) reads: String,
    pub(super) reads_title: String,
    /// "7 of 20", or a dash before any answer.
    pub(super) mentioned: String,
    pub(super) was: String,
    pub(super) tone: &'static str,
    pub(super) visitors: String,
    pub(super) revenue: String,
}

/// The AI card's Answers tab: per assistant, answers that used the site, mentions and what it sent.
pub struct AnswersTab {
    pub(super) rows: Vec<AnswerRow>,
    pub(super) keyed: bool,
    pub(super) foot: &'static str,
    pub(super) settings: String,
    pub(super) on: bool,
    /// Links to the Visits and Answers tabs.
    pub(super) tabs: (String, String),
}

pub fn tab(
    i: &Inputs,
    bots: &[BotHits],
    sources: &[BreakdownRow],
    href: impl Fn(Assistant) -> String,
    settings: String,
    on: bool,
) -> AnswersTab {
    let rows = Assistant::ALL
        .into_iter()
        .filter_map(|a| {
            let n = numbers(a, i, bots, sources);
            let keyed = i.keyed.contains(&a);
            if n.reads == 0 && n.now.answers == 0 && n.visitors == 0 && !keyed {
                return None;
            }
            let (was, tone) = was(&n.now, &n.prev);
            Some(AnswerRow {
                icon: crate::icons::source(a.logo(), 16),
                name: a.name(),
                sub: pages_line(&n),
                href: href(a),
                reads: group(n.reads.max(0) as u64),
                reads_title: format!(
                    "Used in {} that opened your pages live",
                    plural(n.reads.max(0) as u64, "answer")
                ),
                mentioned: if n.now.answers > 0 {
                    of(&n.now)
                } else {
                    "\u{2013}".into()
                },
                was,
                tone,
                visitors: group(n.visitors),
                revenue: money(n.revenue),
            })
        })
        .collect();
    let keyed = !i.keyed.is_empty();
    AnswersTab {
        rows,
        keyed,
        foot: if keyed {
            "Used in: answers that opened your pages. Mentioned: answers that name you."
        } else {
            "Add your own API keys to see how often each assistant names you."
        },
        settings,
        on,
        tabs: (String::new(), String::new()),
    }
}

/// One question's answers from an assistant, latest first.
struct QuestionView {
    question: String,
    rate: String,
    named: bool,
    latest: String,
    pieces: Vec<(String, bool)>,
}

struct AnswersView {
    icon: String,
    name: &'static str,
    range: String,
    reads: String,
    mentioned: String,
    rate: String,
    revenue: String,
    visitors: String,
    cited: Vec<(String, String)>,
    opened: Vec<String>,
    keyed: bool,
    provider: &'static str,
    settings: String,
    questions: Vec<QuestionView>,
}

#[derive(Template)]
#[template(path = "_answers.html")]
struct AnswersFragment {
    v: AnswersView,
}

#[derive(Template)]
#[template(path = "answers.html")]
struct AnswersPage {
    shell: Shell,
    title: &'static str,
    back: String,
    v: AnswersView,
}

#[derive(Deserialize)]
pub struct RangeQuery {
    from: Option<String>,
    to: Option<String>,
}

fn ordinal(n: i64) -> String {
    let suffix = match (n % 10, n % 100) {
        (1, 11) | (2, 12) | (3, 13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// GET /app/{domain}/answers/{assistant}: the questions and answers, with mentions marked.
pub async fn page(
    State(state): State<AppState>,
    account: Account,
    Path((domain, provider)): Path<(String, String)>,
    Query(q): Query<RangeQuery>,
    headers: HeaderMap,
) -> Response {
    let ctx = match load(&state, &account, &domain, "dashboard").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let Some(a) = Assistant::parse(&provider) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let ymd = format_description!("[year]-[month]-[day]");
    let date = |s: &Option<String>| s.as_deref().and_then(|s| Date::parse(s, ymd).ok());
    let (from, to) = match (date(&q.from), date(&q.to)) {
        (Some(f), Some(t)) if f <= t && (t - f).whole_days() < 3660 => (f, t),
        _ => (ctx.today - Duration::days(29), ctx.today),
    };
    let at = |d: Date| d.midnight().assume_offset(ctx.offset).unix_timestamp();
    let (from_ts, to_ts) = (at(from), at(to + Duration::days(1)) - 1);
    let site = &ctx.site.id;
    let loaded = async {
        let inputs = Inputs::load(&state.db, site, (from, to), (from_ts, to_ts)).await?;
        let bots = crawlers::hits(&state.db, site, from_ts, to_ts).await?;
        let latest = answers::latest(&state.db, site, a, day(from), day(to)).await?;
        let saved: Option<String> =
            sqlx::query_scalar("SELECT names FROM ai_settings WHERE site_id = ?")
                .bind(site)
                .fetch_optional(&state.db)
                .await?;
        Ok::<_, sqlx::Error>((inputs, bots, latest, saved.unwrap_or_default()))
    };
    let Ok((inputs, bots, latest, saved)) = loaded.await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let store = state.events.clone();
    let period = Period {
        from,
        to,
        tz: ctx.site.timezone.clone(),
    };
    let id = ctx.site.id.clone();
    let sources = match blocking(move || store.breakdown(&id, &period, Dimension::ChannelSource)).await
    {
        Ok(s) => s,
        Err(code) => return code.into_response(),
    };
    let n = numbers(a, &inputs, &bots, &sources);
    let names = answers::names(&saved, &ctx.site.domain);
    let short = format_description!("[month repr:short] [day padding:none]");
    let show = |d: Date| d.format(&short).unwrap_or_default();
    let v = AnswersView {
        icon: crate::icons::source(a.logo(), 20),
        name: a.name(),
        range: format!("{} to {}", show(from), show(to)),
        reads: group(n.reads.max(0) as u64),
        mentioned: if n.now.answers > 0 {
            of(&n.now)
        } else {
            "\u{2013}".into()
        },
        rate: if n.now.answers > 0 {
            format!(
                "{:.0}% of answers",
                n.now.mentioned as f64 / n.now.answers as f64 * 100.0
            )
        } else {
            "No answers yet".into()
        },
        revenue: money(n.revenue),
        visitors: plural(n.visitors, "visitor"),
        cited: n
            .cited
            .iter()
            .take(5)
            .map(|(p, c)| (p.clone(), plural(*c as u64, "answer")))
            .collect(),
        opened: n
            .opened
            .iter()
            .take(8)
            .map(|(p, _)| p.clone())
            .collect(),
        keyed: inputs.keyed.contains(&a),
        provider: a.provider_name(),
        settings: format!("{}#ai-answers", app_path(&ctx.site.domain, "settings")),
        questions: latest
            .iter()
            .map(|l| {
                let when = Date::from_julian_day((l.day + 2_440_588) as i32)
                    .map(show)
                    .unwrap_or_default();
                let mut latest = format!("Latest answer, {when}: ");
                match l.position.filter(|_| l.latest_mentioned) {
                    Some(p) => latest.push_str(&format!(
                        "named {} of {} products",
                        ordinal(p),
                        l.named
                    )),
                    None if l.latest_mentioned => latest.push_str("named"),
                    None => latest.push_str("not named"),
                }
                let cited = l.cited_pages();
                if !cited.is_empty() {
                    latest.push_str(&format!(", cites {}", cited.join(", ")));
                }
                QuestionView {
                    question: l.question.clone(),
                    rate: format!("Named in {} of {}", l.mentioned, l.answers),
                    named: l.mentioned > 0,
                    latest,
                    pieces: answers::highlight(&l.answer, &names),
                }
            })
            .collect(),
    };
    if headers.contains_key("x-drawer") {
        return render(AnswersFragment { v });
    }
    render(AnswersPage {
        title: "AI answers",
        back: format!("{}?ai=answers#ai", app_path(&ctx.site.domain, "")),
        v,
        shell: ctx.shell,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(key: &str, visitors: u64, revenue: f64) -> BreakdownRow {
        BreakdownRow {
            key: Some(key.into()),
            label: key.into(),
            unattributed: false,
            visitors,
            payments: 1,
            customers: 1,
            revenue_usd: revenue,
            revenue_per_visitor: None,
        }
    }

    #[test]
    fn rows_join_live_reads_mentions_and_revenue() {
        let t = |answers, mentioned| Tally { answers, mentioned };
        let inputs = Inputs {
            keyed: vec![Assistant::ChatGpt],
            now: vec![("openai".into(), t(20, 7))],
            prev: vec![("openai".into(), t(20, 4))],
            cited: vec![("openai".into(), "/pricing".into(), 5)],
            pages: vec![
                ("ChatGPT-User".into(), "/compare".into(), 9),
                ("Claude-User".into(), "/docs".into(), 4),
                ("GPTBot".into(), "/".into(), 90),
            ],
        };
        let hit = |bot: &str, hits| BotHits {
            bot: bot.into(),
            hits,
            pages: 1,
            top_page: "/".into(),
            last_seen: 0,
        };
        let bots = [hit("ChatGPT-User", 12), hit("GPTBot", 90), hit("Claude-User", 4)];
        let sources = [row("AI|chatgpt.com", 30, 120.0), row("AI|claude.ai", 5, 0.0)];
        let tab = tab(
            &inputs,
            &bots,
            &sources,
            |a| format!("/a/{}", a.as_str()),
            "/s".into(),
            true,
        );
        let names: Vec<_> = tab.rows.iter().map(|r| r.name).collect();
        assert_eq!(names, ["ChatGPT", "Claude"]);
        let gpt = &tab.rows[0];
        assert_eq!(
            (gpt.reads.as_str(), gpt.mentioned.as_str(), gpt.was.as_str(), gpt.tone),
            ("12", "7 of 20", "was 4 of 20", "up")
        );
        assert_eq!(
            (gpt.visitors.as_str(), gpt.revenue.as_str(), gpt.sub.as_str()),
            ("30", "$120", "Cites /pricing")
        );
        let claude = &tab.rows[1];
        assert_eq!(
            (claude.reads.as_str(), claude.mentioned.as_str(), claude.sub.as_str()),
            ("4", "\u{2013}", "Opened /docs")
        );
        assert!(tab.keyed && gpt.href == "/a/openai");
        let none = super::tab(
            &Inputs {
                keyed: Vec::new(),
                now: Vec::new(),
                prev: Vec::new(),
                cited: Vec::new(),
                pages: Vec::new(),
            },
            &[],
            &[],
            |_| String::new(),
            String::new(),
            false,
        );
        assert!(none.rows.is_empty() && !none.keyed);
        assert_eq!(ordinal(2), "2nd");
        assert_eq!(ordinal(11), "11th");
        assert_eq!(day(Date::from_calendar_date(1970, time::Month::January, 2).unwrap()), 1);
    }
}
