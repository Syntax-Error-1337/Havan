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
    /// Fire at sunrise or sunset for the home's location, shifted by `offset_min`
    /// (negative is earlier). A trigger, not a condition: "close the blinds" wants to
    /// happen *at* dusk, not when something else does.
    Solar { event: SolarEvent, offset_min: i32 },
    /// Fire when the home's presence changes to this state.
    Presence { presence: Presence },
    /// Fire when someone calls the automation's webhook. The token is the whole
    /// credential: the endpoint has no user, so the URL *is* the authorisation.
    Webhook { token: String },
    /// Only when someone presses Run.
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolarEvent {
    Sunrise,
    Sunset,
}

impl SolarEvent {
    pub fn label(&self) -> &'static str {
        match self {
            SolarEvent::Sunrise => "sunrise",
            SolarEvent::Sunset => "sunset",
        }
    }

    pub fn is_sunset(&self) -> bool {
        matches!(self, SolarEvent::Sunset)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Presence {
    /// At least one member is at home.
    Home,
    /// Nobody is.
    Away,
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
    /// Is anybody home?
    Present { presence: Presence },
    /// Is it dark outside (or light)? Evaluated against the home's own sunrise and
    /// sunset, so "after sunset" means the same thing in Oslo and in Nairobi.
    Solar { event: SolarEvent, offset_min: i32 },
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
            Trigger::Solar { event, offset_min } => {
                let when = event.label();
                match offset_min.cmp(&0) {
                    std::cmp::Ordering::Equal => format!("at {when}"),
                    std::cmp::Ordering::Less => {
                        format!("{} minutes before {when}", offset_min.abs())
                    }
                    std::cmp::Ordering::Greater => format!("{offset_min} minutes after {when}"),
                }
            }
            Trigger::Presence { presence } => match presence {
                Presence::Home => "when someone arrives home".to_string(),
                Presence::Away => "when everyone leaves".to_string(),
            },
            Trigger::Webhook { .. } => "when the webhook is called".to_string(),
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
            Trigger::Schedule { .. }
            | Trigger::Solar { .. }
            | Trigger::Presence { .. }
            | Trigger::Webhook { .. }
            | Trigger::Manual => false,
        }
    }

    pub fn every_secs(&self) -> Option<u32> {
        match self {
            Trigger::Schedule { every_secs } => Some(*every_secs),
            _ => None,
        }
    }

    pub fn solar(&self) -> Option<(SolarEvent, i32)> {
        match self {
            Trigger::Solar { event, offset_min } => Some((*event, *offset_min)),
            _ => None,
        }
    }

    /// The word a run log records. Kept next to the variant so a new trigger cannot
    /// be added without deciding what its runs are called.
    pub fn kind(&self) -> &'static str {
        match self {
            Trigger::Schedule { .. } => "schedule",
            Trigger::Solar { .. } => "solar",
            Trigger::Presence { .. } => "presence",
            Trigger::Webhook { .. } => "webhook",
            Trigger::DeviceState { .. } => "device_state",
            Trigger::Manual => "manual",
        }
    }
}

/// Everything a condition may look at, gathered once per evaluation.
///
/// Conditions used to take a point lookup and a clock; presence and daylight need
/// the home's people and the home's sky, and a fourth and fifth positional argument
/// would have made every call site unreadable. A snapshot also makes the evaluator
/// honest: if a fact is not in here, no condition can secretly read it from
/// somewhere else.
pub struct Snapshot<'a> {
    /// Current value of a point, or `None` when the device is unknown. The closure is
    /// `Send + Sync` because a snapshot crosses an await inside a spawned task.
    pub get: &'a (dyn Fn(Uuid, &str) -> Option<serde_json::Value> + Send + Sync),
    /// Is at least one member at home?
    pub anyone_home: bool,
    /// Today's arc at the home's coordinates, in UTC minutes. `None` when the home
    /// has no location, or when the sun does not cross the horizon.
    pub solar: Option<crate::solar::Solar>,
    /// Minutes after midnight, UTC.
    pub now_minutes: u32,
}

impl Condition {
    pub fn holds(&self, snap: &Snapshot<'_>) -> bool {
        match self {
            Condition::DeviceState {
                device_id,
                code,
                op,
                value,
            } => (snap.get)(*device_id, code).is_some_and(|current| compare(&current, op, value)),
            Condition::TimeOfDay { after, before } => {
                let (Some(after), Some(before)) = (parse_hhmm(after), parse_hhmm(before)) else {
                    return false;
                };
                if after <= before {
                    (after..before).contains(&snap.now_minutes)
                } else {
                    // A window that wraps midnight, e.g. 22:00–06:00.
                    snap.now_minutes >= after || snap.now_minutes < before
                }
            }
            Condition::Present { presence } => match presence {
                Presence::Home => snap.anyone_home,
                Presence::Away => !snap.anyone_home,
            },
            Condition::Solar { event, offset_min } => {
                // No location, no answer: a missing coordinate must make the rule
                // quietly false, not decide it is dark everywhere.
                let Some(solar) = snap.solar else {
                    return false;
                };
                let now = shifted(snap.now_minutes, *offset_min);
                let daylight = crate::solar::is_daylight(solar, now);
                match event {
                    SolarEvent::Sunrise => daylight,
                    SolarEvent::Sunset => !daylight,
                }
            }
        }
    }
}

/// Move a minute-of-day by an offset, wrapping around the clock.
fn shifted(minutes: u32, offset_min: i32) -> u32 {
    (i64::from(minutes) + i64::from(offset_min)).rem_euclid(1_440) as u32
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
    if let Trigger::Solar { offset_min, .. } = def.trigger
        && !(-720..=720).contains(&offset_min)
    {
        return Err(AppError::validation(
            "a solar offset must be within twelve hours",
        ));
    }
    if let Trigger::Webhook { token } = &def.trigger
        && token.is_empty()
    {
        return Err(AppError::validation("a webhook trigger needs a token"));
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

    /// The home's coordinates, when it has any. Solar rules need them and refuse to
    /// exist without them.
    fn home_location(
        &self,
        home_id: Uuid,
    ) -> impl Future<Output = Result<Option<(f64, f64)>, AppError>> + Send;

    /// Is at least one member at home?
    fn anyone_home(&self, home_id: Uuid) -> impl Future<Output = Result<bool, AppError>> + Send;

    /// Arm a webhook-triggered automation, if `token` is its token. One statement, so
    /// a wrong token cannot be distinguished from a missing automation — and no read
    /// of somebody else's rule happens on the path an anonymous caller can reach.
    fn fire_webhook(
        &self,
        id: Uuid,
        token: &str,
    ) -> impl Future<Output = Result<bool, AppError>> + Send;

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
        mut def: AutomationDef,
    ) -> Result<Automation, AppError> {
        // A webhook token is a credential, so the server makes it. A client-chosen
        // one would be guessable ("hook1") and would be the same across tenants.
        if let Trigger::Webhook { token } = &mut def.trigger
            && token.trim().is_empty()
        {
            *token = kernel::new_id().simple().to_string();
        }

        validate(&def)?;
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 80 {
            return Err(AppError::validation("name must be 1–80 characters"));
        }

        let next = match def.trigger {
            Trigger::Schedule { .. } => def
                .trigger
                .every_secs()
                .map(|secs| next_run(secs, kernel::now())),
            Trigger::Solar { event, offset_min } => {
                let Some((lat, lon)) = self.repo.home_location(home_id).await? else {
                    return Err(AppError::validation(
                        "set the home's location before using sunrise or sunset",
                    ));
                };
                crate::solar::next_occurrence(
                    kernel::now(),
                    lat,
                    lon,
                    event.is_sunset(),
                    offset_min,
                )
            }
            _ => None,
        };

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

    /// Arm a webhook rule if the token matches. `false` covers both "no such
    /// automation" and "wrong token" on purpose: an anonymous caller must not be able
    /// to enumerate rules by watching which one says 404.
    pub async fn fire_webhook(&self, id: Uuid, token: &str) -> Result<bool, AppError> {
        self.repo.fire_webhook(id, token).await
    }

    /// Run conditions against a snapshot. The snapshot is supplied by the executor,
    /// which is the only place that touches storage.
    pub fn should_run(def: &AutomationDef, snap: &Snapshot<'_>) -> (bool, RunLog) {
        let mut log = RunLog::new();

        for (index, condition) in def.conditions.iter().enumerate() {
            if !condition.holds(snap) {
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

    /// A snapshot with no devices, nobody home, and no location — the honest default.
    fn snap(now_minutes: u32) -> Snapshot<'static> {
        static NO_VALUES: fn(Uuid, &str) -> Option<serde_json::Value> = |_, _| None;
        Snapshot {
            get: &NO_VALUES,
            anyone_home: false,
            solar: None,
            now_minutes,
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
    fn a_solar_trigger_reads_as_a_sentence() {
        assert_eq!(
            Trigger::Solar {
                event: SolarEvent::Sunset,
                offset_min: 0
            }
            .describe(),
            "at sunset"
        );
        assert_eq!(
            Trigger::Solar {
                event: SolarEvent::Sunset,
                offset_min: -30
            }
            .describe(),
            "30 minutes before sunset"
        );
        assert_eq!(
            Trigger::Solar {
                event: SolarEvent::Sunrise,
                offset_min: 15
            }
            .describe(),
            "15 minutes after sunrise"
        );
    }

    #[test]
    fn every_trigger_knows_what_its_runs_are_called() {
        // A run log says "webhook", not "schedule" — the log is the audit trail, and
        // an audit trail that misnames the cause is worse than none.
        assert_eq!(
            Trigger::Solar {
                event: SolarEvent::Sunrise,
                offset_min: 0
            }
            .kind(),
            "solar"
        );
        assert_eq!(
            Trigger::Presence {
                presence: Presence::Away
            }
            .kind(),
            "presence"
        );
        assert_eq!(Trigger::Webhook { token: "t".into() }.kind(), "webhook");
    }

    #[test]
    fn presence_says_whether_anybody_is_in() {
        let away = Condition::Present {
            presence: Presence::Away,
        };
        let home = Condition::Present {
            presence: Presence::Home,
        };
        let empty = snap(12 * 60);
        let occupied = Snapshot {
            get: &|_, _| None,
            anyone_home: true,
            solar: None,
            now_minutes: 12 * 60,
        };

        assert!(away.holds(&empty), "nobody home satisfies 'nobody home'");
        assert!(!away.holds(&occupied));
        assert!(home.holds(&occupied));
    }

    #[test]
    fn darkness_follows_the_homes_own_sun_not_the_clock() {
        let after_sunset = Condition::Solar {
            event: SolarEvent::Sunset,
            offset_min: 0,
        };
        // Sunrise 06:00, sunset 18:00 — an arc for one specific place and day.
        let solar = crate::solar::Solar {
            sunrise: 6 * 60,
            sunset: 18 * 60,
        };
        let at = |minutes: u32| Snapshot {
            get: &|_, _| None,
            anyone_home: true,
            solar: Some(solar),
            now_minutes: minutes,
        };

        assert!(
            !after_sunset.holds(&at(12 * 60)),
            "noon is not after sunset"
        );
        assert!(after_sunset.holds(&at(20 * 60)), "evening is");
        assert!(after_sunset.holds(&at(2 * 60)), "so is the small hours");
    }

    #[test]
    fn no_location_means_no_solar_condition_ever_holds() {
        // The dangerous default would be "yes": a rule would fire at all hours in a
        // home the platform cannot place.
        let cond = Condition::Solar {
            event: SolarEvent::Sunset,
            offset_min: 0,
        };
        let no_place = Snapshot {
            get: &|_, _| None,
            anyone_home: true,
            solar: None,
            now_minutes: 23 * 60,
        };
        assert!(!cond.holds(&no_place));
    }

    #[test]
    fn a_webhook_without_a_token_is_refused() {
        let def = AutomationDef {
            trigger: Trigger::Webhook {
                token: String::new(),
            },
            conditions: vec![],
            actions: vec![Action::Notify {
                title: "x".into(),
                body: String::new(),
            }],
        };
        assert!(validate(&def).is_err());
    }

    #[test]
    fn a_solar_offset_beyond_half_a_day_is_refused() {
        let def = |offset_min| AutomationDef {
            trigger: Trigger::Solar {
                event: SolarEvent::Sunset,
                offset_min,
            },
            conditions: vec![],
            actions: vec![Action::Notify {
                title: "x".into(),
                body: String::new(),
            }],
        };
        assert!(validate(&def(-720)).is_ok());
        assert!(validate(&def(721)).is_err());
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
        assert!(!c.holds(&snap(0)));
    }

    #[test]
    fn a_time_window_that_wraps_midnight_holds_on_both_sides() {
        let night = Condition::TimeOfDay {
            after: "22:00".into(),
            before: "06:00".into(),
        };
        assert!(night.holds(&snap(23 * 60))); // 23:00
        assert!(night.holds(&snap(2 * 60))); // 02:00
        assert!(!night.holds(&snap(12 * 60))); // 12:00

        let day = Condition::TimeOfDay {
            after: "08:00".into(),
            before: "18:00".into(),
        };
        assert!(day.holds(&snap(9 * 60)));
        assert!(!day.holds(&snap(19 * 60)));
    }

    #[test]
    fn a_malformed_time_is_false_not_a_crash() {
        let bad = Condition::TimeOfDay {
            after: "25:99".into(),
            before: "nonsense".into(),
        };
        assert!(!bad.holds(&snap(600)));
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

        let values =
            |id: Uuid, _: &str| (id == Uuid::from_u128(1)).then_some(serde_json::json!(true));
        let snapshot = Snapshot {
            get: &values,
            anyone_home: true,
            solar: None,
            now_minutes: 12 * 60,
        };
        let (run, log) = AutomationService::<NoopRepo>::should_run(&def, &snapshot);
        assert!(!run, "the second condition is unmet");
        assert_eq!(log.steps.len(), 1);
    }

    struct NoopRepo;
    impl AutomationRepo for NoopRepo {
        async fn home_location(&self, _: Uuid) -> Result<Option<(f64, f64)>, AppError> {
            Ok(None)
        }

        async fn anyone_home(&self, _: Uuid) -> Result<bool, AppError> {
            Ok(true)
        }

        async fn fire_webhook(&self, _: Uuid, _: &str) -> Result<bool, AppError> {
            Ok(false)
        }

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
