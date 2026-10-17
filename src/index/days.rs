//! Per-day timeseries values kept across appends; a new event drops only the days its session covers.

use super::*;

/// (payments, customers, revenue, converted, matched revenue, revenue by channel) of a day.
pub(super) type DayPays = (u64, u64, f64, u64, f64, Vec<(Option<String>, f64)>);

/// Days kept before the cache starts over.
const DAYS_KEPT: usize = 50_000;

/// One local day's values: its UTC span, distinct visitors and payments.
pub(super) struct Day {
    start: i64,
    end: i64,
    visitors: u64,
    pays: Option<DayPays>,
}

/// Days by filter, time zone and local day label.
pub(super) type Days = Mutex<HashMap<(String, String, i64), Day>>;

impl SiteIndex {
    /// Distinct visitors and payments per label of `pieces`, one cached day at a time.
    pub(super) fn per_day(
        &self,
        pieces: &Pieces,
        scope: &Scope,
        tz: &str,
    ) -> (HashMap<i64, u64>, HashMap<i64, DayPays>) {
        let (mut visitors, mut pays) = (HashMap::new(), HashMap::new());
        let mut i = 0;
        while i < pieces.len() {
            let label = pieces[i].2;
            let mut j = i + 1;
            while j < pieces.len() && pieces[j].2 == label {
                j += 1;
            }
            let day = &pieces[i..j];
            let (start, end) = (day[0].0, day[day.len() - 1].1);
            let key = (scope.1.clone(), tz.to_string(), label);
            let cached = self
                .days
                .lock()
                .unwrap()
                .get(&key)
                .filter(|d| d.start == start && d.end == end)
                .map(|d| (d.visitors, d.pays.clone()));
            let (v, p) = cached.unwrap_or_else(|| {
                let day = day.to_vec();
                let v = self.visitors_per(&day, scope).remove(&label).unwrap_or(0);
                let p = self.payments_per(&day, scope, true).remove(&label);
                let mut days = self.days.lock().unwrap();
                if days.len() >= DAYS_KEPT {
                    days.clear();
                }
                days.insert(
                    key,
                    Day {
                        start,
                        end,
                        visitors: v,
                        pays: p.clone(),
                    },
                );
                (v, p)
            });
            if v > 0 {
                visitors.insert(label, v);
            }
            if let Some(p) = p {
                pays.insert(label, p);
            }
            i = j;
        }
        (visitors, pays)
    }

    /// Drops the days that overlap any of `spans`, each [first, last] of a changed session.
    pub(super) fn days_changed(&mut self, mut spans: Vec<(i64, i64)>) {
        let days = self.days.get_mut().unwrap();
        if days.is_empty() || spans.is_empty() {
            return;
        }
        spans.sort_unstable();
        let mut merged: Vec<(i64, i64)> = Vec::new();
        for (a, b) in spans {
            match merged.last_mut() {
                Some(m) if a <= m.1 => m.1 = m.1.max(b),
                _ => merged.push((a, b)),
            }
        }
        days.retain(|_, d| {
            let i = merged.partition_point(|m| m.1 < d.start);
            merged.get(i).is_none_or(|m| m.0 >= d.end)
        });
    }
}
