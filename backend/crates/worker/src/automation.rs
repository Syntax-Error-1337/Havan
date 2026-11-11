//! The scheduler: leases a due automation, evaluates it, runs its actions.
//!
//! Everything that touches the world happens here — the matcher itself is pure
//! and lives in `domain::automation`. That split is what lets the rules be
//! tested without a database, a broker or a clock.

use domain::automation::{
    Action, Automation, AutomationRepo, AutomationService, PointEvent, RunLog, RunStep, Trigger,
    next_run,
};
use infra::PgAutomation;
use protocol::envelope::{CommandMsg, CommandOp, DpValue};
use rumqttc::{AsyncClient, QoS};
use uuid::Uuid;

pub struct Runner {
    repo: PgAutomation,
    mqtt: AsyncClient,
    id: String,
    /// Last fire time per automation. Two rules that set each other's trigger
    /// would otherwise ping-pong forever; a short cooldown makes that a
    /// non-event instead of a fleet-wide loop.
    cooling: std::sync::Mutex<std::collections::HashMap<Uuid, std::time::Instant>>,
}

impl Runner {
    pub fn new(repo: PgAutomation, mqtt: AsyncClient, id: String) -> Self {
        Self {
            repo,
            mqtt,
            id,
            cooling: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// A device report arrived: run every rule that watches this point.
    pub async fn on_event(&self, device_id: Uuid, code: &str, value: &serde_json::Value) {
        let event = PointEvent {
            device_id,
            code: code.to_string(),
            value: value.clone(),
        };

        let candidates = match self.repo.triggered_by(device_id, code).await {
            Ok(candidates) => candidates,
            Err(e) => {
                tracing::warn!(error = ?e, "could not look up triggered automations");
                return;
            }
        };

        for automation in candidates {
            // SQL narrowed it to "watches this point"; the matcher decides
            // whether *this value* fires it.
            if !automation.enabled || !automation.definition.trigger.matches(&event) {
                continue;
            }
            if self.is_cooling(automation.id) {
                tracing::debug!(automation = %automation.id, "skipping: fired moments ago");
                continue;
            }

            let (mut log, status) = self.execute(&automation).await;
            // The trigger leads: a log is read top-down, and "why" comes first.
            log.steps.insert(
                0,
                RunStep::Trigger {
                    detail: format!("{code} = {value}"),
                },
            );

            if let Err(e) = self
                .repo
                .record_run(automation.id, "device_state", &log, status)
                .await
            {
                tracing::warn!(automation = %automation.id, error = ?e, "could not record run");
            }
            self.mark_fired(automation.id);
        }
    }

    fn is_cooling(&self, id: Uuid) -> bool {
        let map = self.cooling.lock().expect("cooldown map");
        map.get(&id)
            .is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(2))
    }

    fn mark_fired(&self, id: Uuid) {
        let mut map = self.cooling.lock().expect("cooldown map");
        map.insert(id, std::time::Instant::now());
    }

    /// One pass: lease what is due, run it, reschedule, release.
    pub async fn tick(&self) {
        let due = match self.repo.lease_due(&self.id, 30).await {
            Ok(due) => due,
            Err(e) => {
                tracing::warn!(error = ?e, "could not lease automations");
                return;
            }
        };

        for automation in due {
            let (mut log, status) = self.execute(&automation).await;
            log.steps.insert(
                0,
                RunStep::Trigger {
                    detail: automation.definition.trigger.describe(),
                },
            );

            if let Err(e) = self
                .repo
                .record_run(automation.id, "schedule", &log, status)
                .await
            {
                tracing::warn!(automation = %automation.id, error = ?e, "could not record run");
            }

            let next = match automation.definition.trigger {
                Trigger::Schedule { every_secs } => Some(next_run(every_secs, kernel::now())),
                _ => None,
            };
            if let Err(e) = self.repo.reschedule(automation.id, next).await {
                tracing::warn!(automation = %automation.id, error = ?e, "could not reschedule");
            }
        }
    }

    async fn execute(&self, automation: &Automation) -> (RunLog, &'static str) {
        let mut log = RunLog::new();

        // Fetch every value the conditions need in one go, then evaluate purely.
        let mut wanted: Vec<(Uuid, String)> = Vec::new();
        for condition in &automation.definition.conditions {
            if let domain::automation::Condition::DeviceState {
                device_id, code, ..
            } = condition
            {
                wanted.push((*device_id, code.clone()));
            }
        }

        let values = match self.repo.point_values(&wanted).await {
            Ok(values) => values,
            Err(e) => {
                log.push(RunStep::ActionFailed {
                    index: 0,
                    error: format!("could not read device state: {e:?}"),
                });
                return (log, "failed");
            }
        };

        let (should_run, condition_log) =
            AutomationService::<PgAutomation>::should_run(&automation.definition, &|id, code| {
                values.get(&(id, code.to_string())).cloned()
            });
        log.steps.extend(condition_log.steps);

        if !should_run {
            return (log, "skipped");
        }

        let mut failed = false;
        for (index, action) in automation.definition.actions.iter().enumerate() {
            match self.run_action(automation.home_id, action).await {
                Ok(detail) => log.push(RunStep::Action { index, detail }),
                Err(error) => {
                    failed = true;
                    log.push(RunStep::ActionFailed { index, error });
                }
            }
        }

        (log, if failed { "failed" } else { "completed" })
    }

    async fn run_action(&self, home_id: Uuid, action: &Action) -> Result<String, String> {
        match action {
            Action::Notify { title, body } => {
                self.repo
                    .notify_owner(home_id, title, body)
                    .await
                    .map_err(|e| format!("{e:?}"))?;
                Ok(format!("notified: {title}"))
            }
            Action::SetDevice {
                device_id,
                code,
                value,
            } => {
                let target = self
                    .repo
                    .command_target(*device_id, code)
                    .await
                    .map_err(|e| format!("{e:?}"))?
                    .ok_or_else(|| format!("{code} is not a point on {device_id}"))?;

                let command_id = kernel::new_id();
                self.repo
                    .enqueue_command(
                        &target,
                        command_id,
                        *device_id,
                        serde_json::json!({ "dp": target.dp, "value": value }),
                    )
                    .await
                    .map_err(|e| format!("{e:?}"))?;

                // Publish after the row exists: a device that acks before the
                // command is recorded would settle nothing.
                let msg = CommandMsg {
                    v: protocol::WIRE_VERSION,
                    id: command_id,
                    ts: kernel::now_s(),
                    op: CommandOp::Set,
                    dp: Some(target.dp.clone()),
                    value: Some(to_dp_value(value)),
                    action: None,
                };

                self.mqtt
                    .publish(
                        protocol::topics::cmd(
                            &target.vendor_id.to_string(),
                            &target.home_id.to_string(),
                            &device_id.to_string(),
                        ),
                        QoS::AtLeastOnce,
                        false,
                        serde_json::to_vec(&msg).map_err(|e| e.to_string())?,
                    )
                    .await
                    .map_err(|e| format!("publish: {e}"))?;

                Ok(format!("set {code} = {value}"))
            }
        }
    }
}

fn to_dp_value(value: &serde_json::Value) -> DpValue {
    match value {
        serde_json::Value::Bool(b) => DpValue::Bool(*b),
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(i) => DpValue::Int(i),
            None => DpValue::Float(n.as_f64().unwrap_or_default()),
        },
        serde_json::Value::String(s) => DpValue::Text(s.clone()),
        other => DpValue::Text(other.to_string()),
    }
}
