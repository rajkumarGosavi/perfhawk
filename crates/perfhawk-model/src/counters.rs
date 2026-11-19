//! Runtime-neutral counters sampled at a fixed interval during a capture
//! (.NET `System.Runtime` EventCounters, Go `runtime/metrics`).

use serde::{Deserialize, Serialize};

pub const COUNTER_SCHEMA: u32 = 1;
pub const MAX_COUNTER_SERIES: usize = 256;
/// One point per second for the 10 min recording limit, plus slack.
pub const MAX_COUNTER_POINTS: usize = 700;

#[derive(Debug, thiserror::Error)]
pub enum CounterError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Json(String),
}

/// Gauge: a level averaged over the interval. Rate: an increment per interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CounterKind {
    Gauge,
    Rate,
}

/// `t_ns` is nanoseconds since `CounterData::start_unix_ns`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CounterPoint {
    pub t_ns: u64,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CounterSeries {
    /// Runtime name, e.g. "threadpool-queue-length" or "/sched/goroutines:goroutines".
    pub name: String,
    pub display_name: String,
    /// As the runtime reports it ("", "MB", "%", "ms", ...).
    pub unit: String,
    pub kind: CounterKind,
    /// Ordered by `t_ns`.
    pub points: Vec<CounterPoint>,
}

/// A point-in-time runtime signal, e.g. kind "threadpool_starvation".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CounterEvent {
    pub t_ns: u64,
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CounterData {
    pub schema: u32,
    /// Display only ("dotnet", "go").
    pub runtime: String,
    pub start_unix_ns: i64,
    pub duration_ns: u64,
    pub interval_ns: u64,
    pub series: Vec<CounterSeries>,
    pub events: Vec<CounterEvent>,
    pub notes: Vec<String>,
}

fn invalid(msg: impl Into<String>) -> CounterError {
    CounterError::Invalid(msg.into())
}

impl CounterData {
    pub fn validate(&self) -> Result<(), CounterError> {
        if self.schema != COUNTER_SCHEMA {
            return Err(invalid(format!(
                "unsupported counter data schema {} (expected {COUNTER_SCHEMA})",
                self.schema
            )));
        }
        if self.interval_ns == 0 {
            return Err(invalid("counter interval must be greater than zero"));
        }
        if self.series.len() > MAX_COUNTER_SERIES {
            return Err(invalid(format!(
                "too many counter series ({}, max {MAX_COUNTER_SERIES})",
                self.series.len()
            )));
        }
        for s in &self.series {
            if s.points.len() > MAX_COUNTER_POINTS {
                return Err(invalid(format!(
                    "counter '{}' has too many points ({}, max {MAX_COUNTER_POINTS})",
                    s.name,
                    s.points.len()
                )));
            }
            if s.points.windows(2).any(|w| w[1].t_ns <= w[0].t_ns) {
                return Err(invalid(format!("counter '{}' points must be ordered by time", s.name)));
            }
            if s.points.iter().any(|p| !p.value.is_finite()) {
                return Err(invalid(format!("counter '{}' has a non-finite value", s.name)));
            }
        }
        Ok(())
    }

    /// Parses and validates.
    pub fn from_json(bytes: &[u8]) -> Result<CounterData, CounterError> {
        let d: CounterData =
            serde_json::from_slice(bytes).map_err(|e| CounterError::Json(e.to_string()))?;
        d.validate()?;
        Ok(d)
    }

    pub fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("CounterData always serialises")
    }

    pub fn series(&self, name: &str) -> Option<&CounterSeries> {
        self.series.iter().find(|s| s.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> CounterData {
        CounterData {
            schema: COUNTER_SCHEMA,
            runtime: "dotnet".into(),
            start_unix_ns: 1_700_000_000_000_000_000,
            duration_ns: 3_000_000_000,
            interval_ns: 1_000_000_000,
            series: vec![CounterSeries {
                name: "threadpool-queue-length".into(),
                display_name: "ThreadPool Queue Length".into(),
                unit: "".into(),
                kind: CounterKind::Gauge,
                points: vec![
                    CounterPoint { t_ns: 0, value: 0.0 },
                    CounterPoint { t_ns: 1_000_000_000, value: 4.5 },
                ],
            }],
            events: vec![CounterEvent {
                t_ns: 1_000_000_000,
                kind: "threadpool_starvation".into(),
                detail: "worker threads -> 12".into(),
            }],
            notes: vec![],
        }
    }

    #[test]
    fn round_trip() {
        let d = sample();
        assert_eq!(CounterData::from_json(&d.to_json()).unwrap(), d);
        assert!(d.series("threadpool-queue-length").is_some());
        assert!(d.series("nope").is_none());
    }

    #[test]
    fn rejects_schema_2() {
        let mut d = sample();
        d.schema = 2;
        assert!(matches!(CounterData::from_json(&d.to_json()), Err(CounterError::Invalid(_))));
    }

    #[test]
    fn rejects_unordered_points() {
        let mut d = sample();
        d.series[0].points.reverse();
        assert!(matches!(CounterData::from_json(&d.to_json()), Err(CounterError::Invalid(_))));
    }

    #[test]
    fn rejects_non_finite() {
        let mut v: serde_json::Value = serde_json::from_slice(&sample().to_json()).unwrap();
        v["series"][0]["points"][0]["value"] = serde_json::Value::Null;
        let bytes = serde_json::to_vec(&v).unwrap();
        assert!(CounterData::from_json(&bytes).is_err());
        let text = String::from_utf8(sample().to_json()).unwrap().replace("4.5", "1e400");
        assert!(CounterData::from_json(text.as_bytes()).is_err());
    }

    #[test]
    fn rejects_too_many_series() {
        let mut d = sample();
        let s = d.series[0].clone();
        d.series = (0..MAX_COUNTER_SERIES + 1).map(|_| s.clone()).collect();
        assert!(matches!(CounterData::from_json(&d.to_json()), Err(CounterError::Invalid(_))));
    }

    #[test]
    fn rejects_too_many_points() {
        let mut d = sample();
        d.series[0].points = (0..MAX_COUNTER_POINTS as u64 + 1)
            .map(|i| CounterPoint { t_ns: i, value: 1.0 })
            .collect();
        assert!(matches!(CounterData::from_json(&d.to_json()), Err(CounterError::Invalid(_))));
    }

    #[test]
    fn rejects_zero_interval() {
        let mut d = sample();
        d.interval_ns = 0;
        assert!(matches!(CounterData::from_json(&d.to_json()), Err(CounterError::Invalid(_))));
    }

    #[test]
    fn accepts_empty_series() {
        let mut d = sample();
        d.series.clear();
        d.events.clear();
        assert_eq!(CounterData::from_json(&d.to_json()).unwrap(), d);
    }

    #[test]
    fn kind_serialises_lowercase() {
        let s = String::from_utf8(sample().to_json()).unwrap();
        assert!(s.contains("\"kind\":\"gauge\""), "{s}");
    }
}
