use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use time::{OffsetDateTime, UtcOffset};

use crate::events::{Event, EventStore, LatestPageview, Payment};
use crate::providers::Subscription;
use crate::retention::{Mrr, Retention};
use crate::stats::{
    self, Activity, BreakdownRow, Dimension, Filter, GoalCount, GoalRow, GoalStat, Journey,
    Overview, PaymentFilter, PaymentPage, PaymentSummary, Period, ProviderSplit, Report, Step,
    StepCount, Who,
};
use crate::visitors::{Facets, Timeline, VisitorFilter, VisitorPage};

/// Entries kept before the cache starts over.
const CAPACITY: usize = 2000;

type Entry = (u64, Arc<dyn Any + Send + Sync>);

#[derive(Default)]
struct State {
    /// Bumped whenever a site's data changes.
    generations: HashMap<String, u64>,
    entries: HashMap<(String, String), Entry>,
}

/// Read results per site and arguments, dropped the moment that site's data changes.
pub struct Cached<S> {
    inner: S,
    state: Mutex<State>,
}

impl<S: EventStore> Cached<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            state: Mutex::default(),
        }
    }

    fn get<T: Clone + Send + Sync + 'static>(
        &self,
        site_id: &str,
        key: String,
        read: impl FnOnce() -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let entry = (site_id.to_string(), key);
        let generation = {
            let state = self.state.lock().unwrap();
            let generation = state.generations.get(site_id).copied().unwrap_or(0);
            if let Some((g, value)) = state.entries.get(&entry)
                && *g == generation
                && let Some(v) = value.downcast_ref::<T>()
            {
                return Ok(v.clone());
            }
            generation
        };
        let value = read()?;
        let mut state = self.state.lock().unwrap();
        if state.generations.get(site_id).copied().unwrap_or(0) == generation {
            if state.entries.len() >= CAPACITY {
                state.entries.clear();
            }
            state
                .entries
                .insert(entry, (generation, Arc::new(value.clone())));
        }
        Ok(value)
    }

    /// Forgets everything read for `sites`; call after their data is written.
    fn changed<'a>(&self, sites: impl IntoIterator<Item = &'a str>) {
        let mut state = self.state.lock().unwrap();
        for site in sites {
            *state.generations.entry(site.to_string()).or_default() += 1;
            state.entries.retain(|(s, _), _| s != site);
        }
    }
}

impl<S: EventStore> EventStore for Cached<S> {
    fn insert_events(&self, events: &[Event]) -> anyhow::Result<()> {
        let result = self.inner.insert_events(events);
        self.changed(events.iter().map(|e| e.site_id.as_str()));
        result
    }

    fn upsert_payments(&self, payments: &[Payment]) -> anyhow::Result<()> {
        let result = self.inner.upsert_payments(payments);
        self.changed(payments.iter().map(|p| p.site_id.as_str()));
        result
    }

    fn replace_subscriptions(
        &self,
        site_id: &str,
        provider: &str,
        subscriptions: &[Subscription],
    ) -> anyhow::Result<()> {
        let result = self
            .inner
            .replace_subscriptions(site_id, provider, subscriptions);
        self.changed([site_id]);
        result
    }

    fn rebuild_attribution(
        &self,
        site_id: &str,
        model: &str,
        window_days: i64,
    ) -> anyhow::Result<()> {
        let result = self.inner.rebuild_attribution(site_id, model, window_days);
        self.changed([site_id]);
        result
    }

    fn retention(
        &self,
        site_id: &str,
        period: &Period,
        now: OffsetDateTime,
    ) -> anyhow::Result<Retention> {
        self.inner.retention(site_id, period, now)
    }

    fn mrr(
        &self,
        site_id: &str,
        period: &Period,
        now: OffsetDateTime,
    ) -> anyhow::Result<Option<Mrr>> {
        self.inner.mrr(site_id, period, now)
    }

    #[cfg(test)]
    fn count_events(&self, site_id: &str) -> anyhow::Result<u64> {
        self.inner.count_events(site_id)
    }

    fn latest_pageview(&self, site_id: &str) -> anyhow::Result<Option<LatestPageview>> {
        self.get(site_id, "latest_pageview".into(), || {
            self.inner.latest_pageview(site_id)
        })
    }

    fn first_event(&self, site_id: &str) -> anyhow::Result<Option<OffsetDateTime>> {
        self.get(site_id, "first_event".into(), || {
            self.inner.first_event(site_id)
        })
    }

    fn settled(&self, site_id: &str) -> bool {
        self.inner.settled(site_id)
    }

    fn live_visitors(&self, site_id: &str) -> anyhow::Result<u64> {
        self.inner.live_visitors(site_id)
    }

    fn insert_bots(&self, bots: &[(Event, &str)]) -> anyhow::Result<()> {
        self.inner.insert_bots(bots)
    }

    fn bots_filtered(&self, site_id: &str, days: i64) -> anyhow::Result<u64> {
        self.inner.bots_filtered(site_id, days)
    }

    fn utc_offset(&self, tz: &str) -> anyhow::Result<UtcOffset> {
        self.inner.utc_offset(tz)
    }

    fn overview_in(&self, site_id: &str, period: &Period, f: &Filter) -> anyhow::Result<Overview> {
        self.get(site_id, format!("overview {period:?} {f:?}"), || {
            self.inner.overview_in(site_id, period, f)
        })
    }

    #[cfg(test)]
    fn timeseries(&self, site_id: &str, period: &Period) -> anyhow::Result<Vec<stats::DayPoint>> {
        self.inner.timeseries(site_id, period)
    }

    fn breakdown_in(
        &self,
        site_id: &str,
        period: &Period,
        dimension: Dimension,
        f: &Filter,
    ) -> anyhow::Result<Vec<BreakdownRow>> {
        self.get(
            site_id,
            format!("breakdown {period:?} {dimension:?} {f:?}"),
            || self.inner.breakdown_in(site_id, period, dimension, f),
        )
    }

    fn last_pages_in(
        &self,
        site_id: &str,
        period: &Period,
        f: &Filter,
    ) -> anyhow::Result<Vec<BreakdownRow>> {
        self.get(site_id, format!("last_pages {period:?} {f:?}"), || {
            self.inner.last_pages_in(site_id, period, f)
        })
    }

    fn payment_list_in(
        &self,
        site_id: &str,
        period: &Period,
        filter: PaymentFilter,
        limit: u32,
        offset: u32,
        f: &Filter,
    ) -> anyhow::Result<PaymentPage> {
        self.get(
            site_id,
            format!("payment_list {period:?} {filter:?} {limit} {offset} {f:?}"),
            || {
                self.inner
                    .payment_list_in(site_id, period, filter, limit, offset, f)
            },
        )
    }

    fn journey(
        &self,
        site_id: &str,
        provider: &str,
        provider_payment_id: &str,
        window_days: i64,
    ) -> anyhow::Result<Option<Journey>> {
        self.get(
            site_id,
            format!("journey {provider:?} {provider_payment_id:?} {window_days}"),
            || {
                self.inner
                    .journey(site_id, provider, provider_payment_id, window_days)
            },
        )
    }

    fn report_in(&self, site_id: &str, period: &Period, f: &Filter) -> anyhow::Result<Report> {
        self.get(site_id, format!("report {period:?} {f:?}"), || {
            self.inner.report_in(site_id, period, f)
        })
    }

    fn hourly(
        &self,
        site_id: &str,
        period: &Period,
        f: &Filter,
    ) -> anyhow::Result<Vec<stats::DayPoint>> {
        self.get(site_id, format!("hourly {period:?} {f:?}"), || {
            self.inner.hourly(site_id, period, f)
        })
    }

    fn payment_summary(&self, site_id: &str, period: &Period) -> anyhow::Result<PaymentSummary> {
        self.get(site_id, format!("payment_summary {period:?}"), || {
            self.inner.payment_summary(site_id, period)
        })
    }

    fn payment_split(&self, site_id: &str, period: &Period) -> anyhow::Result<Vec<ProviderSplit>> {
        self.get(site_id, format!("payment_split {period:?}"), || {
            self.inner.payment_split(site_id, period)
        })
    }

    fn funnel(
        &self,
        site_id: &str,
        period: &Period,
        steps: &[Step],
        f: &Filter,
    ) -> anyhow::Result<Vec<StepCount>> {
        self.get(
            site_id,
            format!("funnel {period:?} {steps:?} {f:?}"),
            || self.inner.funnel(site_id, period, steps, f),
        )
    }

    fn goal_table(
        &self,
        site_id: &str,
        period: &Period,
        pages: &[String],
        f: &Filter,
    ) -> anyhow::Result<Vec<GoalStat>> {
        self.get(
            site_id,
            format!("goal_table {period:?} {pages:?} {f:?}"),
            || self.inner.goal_table(site_id, period, pages, f),
        )
    }

    fn activity(&self, site_id: &str, tz: &str, who: &[Who]) -> anyhow::Result<Vec<Activity>> {
        self.get(site_id, format!("activity {tz:?} {who:?}"), || {
            self.inner.activity(site_id, tz, who)
        })
    }

    fn goal_counts(&self, site_id: &str, period: &Period) -> anyhow::Result<Vec<GoalCount>> {
        self.get(site_id, format!("goal_counts {period:?}"), || {
            self.inner.goal_counts(site_id, period)
        })
    }

    fn goal_list(
        &self,
        site_id: &str,
        period: &Period,
        goal: &str,
        limit: u32,
        offset: u32,
    ) -> anyhow::Result<(u64, Vec<GoalRow>)> {
        self.get(
            site_id,
            format!("goal_list {period:?} {goal:?} {limit} {offset}"),
            || self.inner.goal_list(site_id, period, goal, limit, offset),
        )
    }

    fn visitor_list(
        &self,
        site_id: &str,
        period: &Period,
        filter: &VisitorFilter,
        limit: u32,
        offset: u32,
    ) -> anyhow::Result<VisitorPage> {
        self.get(
            site_id,
            format!("visitor_list {period:?} {filter:?} {limit} {offset}"),
            || {
                self.inner
                    .visitor_list(site_id, period, filter, limit, offset)
            },
        )
    }

    fn visitor_facets(&self, site_id: &str, period: &Period) -> anyhow::Result<Facets> {
        self.get(site_id, format!("visitor_facets {period:?}"), || {
            self.inner.visitor_facets(site_id, period)
        })
    }

    fn timeline(&self, site_id: &str, visitor_id: &str) -> anyhow::Result<Option<Timeline>> {
        self.get(site_id, format!("timeline {visitor_id:?}"), || {
            self.inner.timeline(site_id, visitor_id)
        })
    }
}

#[cfg(test)]
mod tests {
    use time::macros::{date, datetime};

    use super::*;
    use crate::events::DuckStore;

    fn view(visitor: &str, minute: i64) -> Event {
        Event {
            site_id: "s".into(),
            ts: datetime!(2026-10-01 12:00 UTC) + time::Duration::minutes(minute),
            visitor_id: visitor.into(),
            session_id: format!("{visitor}-s"),
            kind: "pageview".into(),
            name: None,
            path: "/".into(),
            referrer_host: Some("news.ycombinator.com".into()),
            utm_source: None,
            utm_medium: None,
            utm_campaign: None,
            utm_content: None,
            utm_term: None,
            channel: "Social".into(),
            country: Some("DE".into()),
            device: "Desktop".into(),
            browser: "Firefox".into(),
            os: "macOS".into(),
            region: None,
            city: None,
            host: None,
            language: None,
        }
    }

    fn pay(id: &str, visitor: &str) -> Payment {
        Payment {
            site_id: "s".into(),
            provider: "stripe".into(),
            provider_payment_id: id.into(),
            ts: datetime!(2026-10-01 13:00 UTC),
            amount: 1000,
            currency: "usd".into(),
            amount_usd: Some(10.0),
            refunded: 0,
            status: "paid".into(),
            visitor_id: Some(visitor.into()),
            customer_id: Some(format!("cus_{visitor}")),
            is_renewal: false,
        }
    }

    fn period() -> Period {
        Period {
            from: date!(2026 - 09 - 01),
            to: date!(2026 - 10 - 31),
            tz: "UTC".into(),
        }
    }

    #[test]
    fn cached_reads_match_the_store_and_follow_new_data() {
        let dir = tempfile::tempdir().unwrap();
        let cached = Cached::new(DuckStore::open(&dir.path().join("e.duckdb")).unwrap());
        cached.insert_events(&[view("v1", 0)]).unwrap();
        let p = period();
        let first = cached.report("s", &p).unwrap();
        assert_eq!(first, cached.inner.report("s", &p).unwrap());
        assert_eq!(
            cached.report("s", &p).unwrap(),
            first,
            "a repeat read is served"
        );

        cached.insert_events(&[view("v2", 5)]).unwrap();
        assert_eq!(cached.overview("s", &p).unwrap().current.visitors, 2);
        assert_eq!(cached.report("s", &p).unwrap().overview.current.visitors, 2);

        assert_eq!(
            cached
                .payment_list("s", &p, PaymentFilter::All, 10, 0)
                .unwrap()
                .total,
            0
        );
        cached.upsert_payments(&[pay("pi_1", "v1")]).unwrap();
        cached
            .rebuild_attribution("s", "last_non_direct", 90)
            .unwrap();
        let list = cached
            .payment_list("s", &p, PaymentFilter::All, 10, 0)
            .unwrap();
        assert_eq!(list.total, 1);
        assert_eq!(
            list,
            cached
                .inner
                .payment_list("s", &p, PaymentFilter::All, 10, 0)
                .unwrap()
        );
        assert!(
            list.rows[0].touch.is_some(),
            "attribution reaches the cached row"
        );
        assert_eq!(
            cached.timeline("s", "v1").unwrap(),
            cached.inner.timeline("s", "v1").unwrap()
        );
    }

    #[test]
    fn a_write_to_one_site_keeps_the_others() {
        let dir = tempfile::tempdir().unwrap();
        let cached = Cached::new(DuckStore::open(&dir.path().join("e.duckdb")).unwrap());
        let p = period();
        cached.overview("other", &p).unwrap();
        cached.insert_events(&[view("v1", 0)]).unwrap();
        let state = cached.state.lock().unwrap();
        assert!(state.entries.keys().any(|(site, _)| site == "other"));
        assert_eq!(state.generations.get("s"), Some(&1));
    }
}
