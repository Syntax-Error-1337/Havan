//! Automations: a trigger, some conditions, some actions.
//!
//! The definition is data, and the evaluator is pure — it takes a snapshot of
//! the world and returns a plan. That is the difference between a rule engine
//! you can test in microseconds and one you can only test with a staging house.

use std::sync::Arc;

use kernel::AppError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationDef {
    pub trigger: Trigger,
    #[serde(default)]
    pub conditions: Vec<Condition>,
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Trigger {
    /// Fire every N seconds. Deliberately not cron in v1: an interval is
    /// testable without a timezone database, and covers "every morning" once
    /// the scheduler knows the home's zone.
    Schedule { every_secs: u32 },
    /// Fire when a point crosses a value.
    DeviceState {
        device_id: Uuid,
        code: String,
        op: CompareOp,
        value: serde_json::Value,
    },
    /// Only when someone presses Run.
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Condition {
    DeviceState {
        device_id: Uuid,
        code: String,
        op: CompareOp,
        value: serde_json::Value,
    },
    /// Wall clock, `HH:MM`, in the home's timezone.
    TimeOfDay { after: String, before: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    SetDevice {
        device_id: Uuid,
        code: String,
        value: serde_json::Value,
    },
    Notify {
        title: String,
        body: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareOp {
    Eq,
    Gt,
    Lt,
    Gte,
    Lte,
}

impl CompareOp {
    /// The symbol a person reads, not the variant name.
    pub fn as_str(&self) -> &'static str {
        match self {
            CompareOp::Eq => "is",
            CompareOp::Gt => ">",
            CompareOp::Lt => "<",
            CompareOp::Gte => ">=",
            CompareOp::Lte => "<=",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PointEvent {
    pub device_id: Uuid,
    pub code: String,
    pub value: serde_json::Value,
}

/// What one run did, step by step. This is what a user sees when they ask why
/// the light came on at 3 a.m.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunLog {
    pub steps: Vec<RunStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunStep {
    Trigger { detail: String },
    ConditionSkipped { index: usize, detail: String },
    Action { index: usize, detail: String },
    ActionFailed { index: usize, error: String },
}

impl RunLog {
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }

    pub fn push(&mut self, step: RunStep) {
        // A runaway loop must not grow a log without bound.
        if self.steps.len() < 200 {
            self.steps.push(step);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

impl Default for RunLog {
    fn default() -> Self {
        Self::new()
    }
}

fn render(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => format!("\"{s}\""),
        other => other.to_string(),
    }
}

impl Trigger {
    /// A phrase a person can read in a run log. The Debug form is for panic
    /// messages; this is what the app shows.
    pub fn describe(&self) -> String {
        match self {
            Trigger::Schedule { every_secs } => {
                if *every_secs % 3600 == 0 {
                    format!("every {} hour(s)", every_secs / 3600)
                } else if *every_secs % 60 == 0 {
                    format!("every {} minute(s)", every_secs / 60)
                } else {
                    format!("every {every_secs} seconds")
                }
            }
            Trigger::DeviceState {
                code, op, value, ..
            } => {
                format!("when {code} {} {}", op.as_str(), render(value))
            }
            Trigger::Manual => "when you press run".to_string(),
        }
    }

    /// Does this event fire the trigger?
    pub fn matches(&self, event: &PointEvent) -> bool {
        match self {
            Trigger::DeviceState {
                device_id,
                code,
                op,
                value,
            } => {
                *device_id == event.device_id
                    && code == &event.code
                    && compare(&event.value, op, value)
            }
            Trigger::Schedule { .. } | Trigger::Manual => false,
        }
    }

    pub fn every_secs(&self) -> Option<u32> {
        match self {
            Trigger::Schedule { every_secs } => Some(*every_secs),
            _ => None,
        }
    }
}

impl Condition {
    /// `get` returns the current value of a point, or `None` when the device is
    /// unknown — an unknown point makes the condition false, never true.
    pub fn holds<F>(&self, get: &F, now_minutes: u32) -> bool
    where
        F: Fn(Uuid, &str) -> Option<serde_json::Value>,
    {
        match self {
            Condition::DeviceState {
                device_id,
                code,
                op,
                value,
            } => get(*device_id, code).is_some_and(|current| compare(&current, op, value)),
            Condition::TimeOfDay { after, before } => {
                let (Some(after), Some(before)) = (parse_hhmm(after), parse_hhmm(before)) else {
                    return false;
                };
                if after <= before {
                    (after..before).contains(&now_minutes)
                } else {
                    // A window that wraps midnight, e.g. 22:00–06:00.
                    now_minutes >= after || now_minutes < before
                }
            }
        }
    }
}

/// Values arrive as JSON from devices, from the API and from stored definitions,
/// so comparison is by number-or-bool, not by Rust type.
fn compare(actual: &serde_json::Value, op: &CompareOp, expected: &serde_json::Value) -> bool {
    let (Some(a), Some(b)) = (as_number(actual), as_number(expected)) else {
        // Non-numeric values fall back to equality of their JSON form.
        return matches!(op, CompareOp::Eq) && actual == expected;
    };

    match op {
        CompareOp::Eq => (a - b).abs() < f64::EPSILON,
        CompareOp::Gt => a > b,
        CompareOp::Lt => a < b,
        CompareOp::Gte => a >= b,
        CompareOp::Lte => a <= b,
    }
}

fn as_number(value: &serde_json::Value) -> Option<f64> {
    match value {
        serde_json::Value::Bool(b) => Some(u8::from(*b) as f64),
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn parse_hhmm(text: &str) -> Option<u32> {
    let (h, m) = text.split_once(':')?;
    let h: u32 = h.parse().ok()?;
    let m: u32 = m.parse::<u32>().ok().filter(|m| *m < 60)?;
    (h < 24).then_some(h * 60 + m)
}

/// Validate a definition before it is stored: a bad automation fails at
/// creation, not silently at 3 a.m.
pub fn validate(def: &AutomationDef) -> Result<(), AppError> {
    if def.actions.is_empty() {
        return Err(AppError::validation(
            "an automation needs at least one action",
        ));
    }
    if let Trigger::Schedule { every_secs } = def.trigger
        && !(5..=86_400).contains(&every_secs)
    {
        return Err(AppError::validation(
            "a schedule must fire between every 5 seconds and once a day",
        ));
    }
    if def.actions.len() > 32 {
        return Err(AppError::validation(
            "an automation may have at most 32 actions",
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Scheduling arithmetic
// ---------------------------------------------------------------------------

/// When a schedule should next fire, rounded to the next whole interval from
/// now. A missed window is not replayed: a light that should have blinked at
/// 02:00 should not blink three times at 09:00.
pub fn next_run(every_secs: u32, now: time::OffsetDateTime) -> time::OffsetDateTime {
    now + time::Duration::seconds(i64::from(every_secs.clamp(5, 86_400)))
}

/// One recorded execution: what fired it, whether it finished, and the steps.
#[derive(Debug, Clone, Serialize)]
pub struct RunRecord {
    pub id: Uuid,
    pub trigger_kind: String,
    pub status: String,
    #[serde(with = "time::serde::rfc3339")]
    pub started_at: time::OffsetDateTime,
    pub log: RunLog,
}

#[derive(Debug, Clone, Serialize)]
pub struct Automation {
    pub id: Uuid,
    pub home_id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub definition: AutomationDef,
    #[serde(with = "time::serde::rfc3339::option")]
    pub next_run_at: Option<time::OffsetDateTime>,
}

pub trait AutomationRepo {
    fn create(
        &self,
        home_id: Uuid,
        name: &str,
        def: &AutomationDef,
        next_run_at: Option<time::OffsetDateTime>,
    ) -> impl Future<Output = Result<Automation, AppError>> + Send;

    fn list(&self, home_id: Uuid)
    -> impl Future<Output = Result<Vec<Automation>, AppError>> + Send;

    fn home_of(&self, id: Uuid) -> impl Future<Output = Result<Option<Uuid>, AppError>> + Send;

    fn delete(&self, id: Uuid) -> impl Future<Output = Result<(), AppError>> + Send;

    /// Turn a rule on or off. Re-enabling a scheduled rule arms it immediately:
    /// a rule that is switched on and then does nothing for an interval looks
    /// broken, and users report it as broken.
    fn set_enabled(
        &self,
        id: Uuid,
        enabled: bool,
    ) -> impl Future<Output = Result<Automation, AppError>> + Send;

    fn runs(
        &self,
        automation_id: Uuid,
        limit: i64,
    ) -> impl Future<Output = Result<Vec<RunRecord>, AppError>> + Send;

    /// Lease every due automation to `owner` for `ttl_secs` and return them.
    /// Two workers racing this must never both win a row.
    /// Enabled automations whose trigger watches this point. The JSON is
    /// filtered in SQL so a busy home does not deserialize every rule.
    fn triggered_by(
        &self,
        device_id: Uuid,
        code: &str,
    ) -> impl Future<Output = Result<Vec<Automation>, AppError>> + Send;

    fn lease_due(
        &self,
        owner: &str,
        ttl_secs: i64,
    ) -> impl Future<Output = Result<Vec<Automation>, AppError>> + Send;

    fn reschedule(
        &self,
        id: Uuid,
        next_run_at: Option<time::OffsetDateTime>,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    fn record_run(
        &self,
        automation_id: Uuid,
        trigger_kind: &str,
        log: &RunLog,
        status: &str,
    ) -> impl Future<Output = Result<(), AppError>> + Send;
}

pub struct AutomationService<R> {
    repo: Arc<R>,
}

impl<R: AutomationRepo + Send + Sync> AutomationService<R> {
    pub fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }

    pub async fn create(
        &self,
        home_id: Uuid,
        name: &str,
        def: AutomationDef,
    ) -> Result<Automation, AppError> {
        validate(&def)?;
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 80 {
            return Err(AppError::validation("name must be 1–80 characters"));
        }
        let next = def
            .trigger
            .every_secs()
            .map(|secs| next_run(secs, kernel::now()));

        self.repo.create(home_id, name, &def, next).await
    }

    pub async fn list(&self, home_id: Uuid) -> Result<Vec<Automation>, AppError> {
        self.repo.list(home_id).await
    }

    pub async fn delete(&self, id: Uuid) -> Result<(), AppError> {
        self.repo.delete(id).await
    }

    pub async fn set_enabled(&self, id: Uuid, enabled: bool) -> Result<Automation, AppError> {
        self.repo.set_enabled(id, enabled).await
    }

    pub async fn runs(&self, id: Uuid, limit: i64) -> Result<Vec<RunRecord>, AppError> {
        self.repo.runs(id, limit).await
    }

    pub async fn home_of(&self, id: Uuid) -> Result<Option<Uuid>, AppError> {
        self.repo.home_of(id).await
    }

    /// Run conditions against a snapshot. `get` is supplied by the executor,
    /// which is the only place that touches storage.
    pub fn should_run<F>(def: &AutomationDef, get: &F) -> (bool, RunLog)
    where
        F: Fn(Uuid, &str) -> Option<serde_json::Value>,
    {
        let mut log = RunLog::new();
        let now = kernel::now();
        let now_minutes = (now.hour() as u32) * 60 + now.minute() as u32;

        for (index, condition) in def.conditions.iter().enumerate() {
            if !condition.holds(get, now_minutes) {
                log.push(RunStep::ConditionSkipped {
                    index,
                    detail: format!("{condition:?}"),
                });
                return (false, log);
            }
        }
        (true, log)
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use kernel::ErrorCode;

    fn event(value: serde_json::Value) -> PointEvent {
        PointEvent {
            device_id: Uuid::from_u128(1),
            code: "switch.power".into(),
            value,
        }
    }

    fn state_trigger(op: CompareOp, value: serde_json::Value) -> Trigger {
        Trigger::DeviceState {
            device_id: Uuid::from_u128(1),
            code: "switch.power".into(),
            op,
            value,
        }
    }

    #[test]
    fn a_matching_event_fires_the_trigger() {
        assert!(
            state_trigger(CompareOp::Eq, serde_json::json!(true))
                .matches(&event(serde_json::json!(true)))
        );
        assert!(
            !state_trigger(CompareOp::Eq, serde_json::json!(true))
                .matches(&event(serde_json::json!(false)))
        );
    }

    #[test]
    fn a_different_device_never_fires() {
        let mut e = event(serde_json::json!(true));
        e.device_id = Uuid::from_u128(2);
        assert!(!state_trigger(CompareOp::Eq, serde_json::json!(true)).matches(&e));
    }

    #[test]
    fn numeric_comparisons_work_across_json_types() {
        // A sensor reports 23.5 as a float, the automation says 23 — not equal.
        assert!(
            !state_trigger(CompareOp::Eq, serde_json::json!(23))
                .matches(&event(serde_json::json!(23.5)))
        );
        assert!(
            state_trigger(CompareOp::Gt, serde_json::json!(20))
                .matches(&event(serde_json::json!(23.5)))
        );
        assert!(
            state_trigger(CompareOp::Lte, serde_json::json!(23.5))
                .matches(&event(serde_json::json!(23.5)))
        );
        // A boolean compares as 0/1, so `gt 0` means "on".
        assert!(
            state_trigger(CompareOp::Gt, serde_json::json!(0))
                .matches(&event(serde_json::json!(true)))
        );
    }

    #[test]
    fn text_compares_by_equality_only() {
        let mut e = event(serde_json::json!("on"));
        e.code = "fan.mode".into();
        let t = Trigger::DeviceState {
            device_id: Uuid::from_u128(1),
            code: "fan.mode".into(),
            op: CompareOp::Eq,
            value: serde_json::json!("on"),
        };
        assert!(t.matches(&e));
        assert!(!t.matches(&{
            PointEvent {
                value: serde_json::json!("off"),
                ..e
            }
        }));
    }

    #[test]
    fn an_unknown_point_makes_a_condition_false() {
        let c = Condition::DeviceState {
            device_id: Uuid::from_u128(9),
            code: "switch.power".into(),
            op: CompareOp::Eq,
            value: serde_json::json!(true),
        };
        assert!(!c.holds(&|_, _| None, 0));
    }

    #[test]
    fn a_time_window_that_wraps_midnight_holds_on_both_sides() {
        let night = Condition::TimeOfDay {
            after: "22:00".into(),
            before: "06:00".into(),
        };
        assert!(night.holds(&|_, _| None, 23 * 60)); // 23:00
        assert!(night.holds(&|_, _| None, 2 * 60)); // 02:00
        assert!(!night.holds(&|_, _| None, 12 * 60)); // 12:00

        let day = Condition::TimeOfDay {
            after: "08:00".into(),
            before: "18:00".into(),
        };
        assert!(day.holds(&|_, _| None, 9 * 60));
        assert!(!day.holds(&|_, _| None, 19 * 60));
    }

    #[test]
    fn a_malformed_time_is_false_not_a_crash() {
        let bad = Condition::TimeOfDay {
            after: "25:99".into(),
            before: "nonsense".into(),
        };
        assert!(!bad.holds(&|_, _| None, 600));
    }

    #[test]
    fn validation_rejects_an_automation_with_no_actions() {
        let def = AutomationDef {
            trigger: Trigger::Manual,
            conditions: vec![],
            actions: vec![],
        };
        assert_eq!(
            validate(&def).unwrap_err().code_kind(),
            ErrorCode::ValidationFailed
        );
    }

    #[test]
    fn validation_bounds_a_schedule() {
        for every_secs in [1, 100_000] {
            let def = AutomationDef {
                trigger: Trigger::Schedule { every_secs },
                conditions: vec![],
                actions: vec![Action::Notify {
                    title: "x".into(),
                    body: String::new(),
                }],
            };
            assert!(validate(&def).is_err(), "{every_secs} should be refused");
        }
    }

    #[test]
    fn conditions_gate_the_run_and_log_the_first_failure() {
        let def = AutomationDef {
            trigger: Trigger::Manual,
            conditions: vec![
                Condition::DeviceState {
                    device_id: Uuid::from_u128(1),
                    code: "switch.power".into(),
                    op: CompareOp::Eq,
                    value: serde_json::json!(true),
                },
                Condition::DeviceState {
                    device_id: Uuid::from_u128(2),
                    code: "switch.power".into(),
                    op: CompareOp::Eq,
                    value: serde_json::json!(true),
                },
            ],
            actions: vec![Action::Notify {
                title: "x".into(),
                body: String::new(),
            }],
        };

        let (run, log) = AutomationService::<NoopRepo>::should_run(&def, &|id, _| {
            (id == Uuid::from_u128(1)).then_some(serde_json::json!(true))
        });
        assert!(!run, "the second condition is unmet");
        assert_eq!(log.steps.len(), 1);
    }

    struct NoopRepo;
    impl AutomationRepo for NoopRepo {
        async fn create(
            &self,
            _: Uuid,
            _: &str,
            _: &AutomationDef,
            _: Option<time::OffsetDateTime>,
        ) -> Result<Automation, AppError> {
            unimplemented!()
        }
        async fn list(&self, _: Uuid) -> Result<Vec<Automation>, AppError> {
            unimplemented!()
        }
        async fn home_of(&self, _: Uuid) -> Result<Option<Uuid>, AppError> {
            unimplemented!()
        }
        async fn delete(&self, _: Uuid) -> Result<(), AppError> {
            unimplemented!()
        }
        async fn set_enabled(&self, _: Uuid, _: bool) -> Result<Automation, AppError> {
            unimplemented!()
        }
        async fn runs(&self, _: Uuid, _: i64) -> Result<Vec<RunRecord>, AppError> {
            unimplemented!()
        }
        async fn triggered_by(&self, _: Uuid, _: &str) -> Result<Vec<Automation>, AppError> {
            unimplemented!()
        }
        async fn lease_due(&self, _: &str, _: i64) -> Result<Vec<Automation>, AppError> {
            unimplemented!()
        }
        async fn reschedule(
            &self,
            _: Uuid,
            _: Option<time::OffsetDateTime>,
        ) -> Result<(), AppError> {
            unimplemented!()
        }
        async fn record_run(&self, _: Uuid, _: &str, _: &RunLog, _: &str) -> Result<(), AppError> {
            unimplemented!()
        }
    }
}
