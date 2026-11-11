//! The scheduler: leases a due automation, evaluates it, runs its actions.
//!
//! Everything that touches the world happens here — the matcher itself is pure
//! and lives in `domain::automation`. That split is what lets the rules be
//! tested without a database, a broker or a clock.

use domain::automation::{
    Action, Automation, AutomationRepo, AutomationService, RunLog, RunStep, Trigger, next_run,
};
use infra::PgAutomation;
use protocol::envelope::{CommandMsg, CommandOp, DpValue};
use rumqttc::{AsyncClient, QoS};
use uuid::Uuid;

pub struct Runner {
    repo: PgAutomation,
    mqtt: AsyncClient,
    id: String,
}

impl Runner {
    pub fn new(repo: PgAutomation, mqtt: AsyncClient, id: String) -> Self {
        Self { repo, mqtt, id }
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
            log.push(RunStep::Trigger {
                detail: format!("{:?}", automation.definition.trigger),
            });

            if let Err(e) = self.repo.record_run(automation.id, &log, status).await {
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
