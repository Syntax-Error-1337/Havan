//! Scenes: a named bundle of actions a person presses.
//!
//! A scene has no trigger and no conditions. That is not a limitation, it is the
//! distinction from an automation: "good night" is a button, and giving it a
//! trigger would mean two engines that overlap.

use std::future::Future;

use kernel::AppError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SceneAction {
    /// Set one datapoint on one device.
    SetDevice {
        device_id: Uuid,
        code: String,
        value: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct Scene {
    pub id: Uuid,
    pub home_id: Uuid,
    pub name: String,
    pub icon: Option<String>,
    pub actions: Vec<SceneAction>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
}

/// What happened when someone pressed the button: per action, nothing invented.
#[derive(Debug, Clone, Serialize)]
pub struct Activation {
    pub scene_id: Uuid,
    pub dispatched: usize,
    /// Actions that failed before dispatch, with the reason. A scene is a burst
    /// of commands; one bad device must not stop the rest, but it must be
    /// visible rather than swallowed.
    pub failed: Vec<ActionFailure>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionFailure {
    pub index: usize,
    pub device_id: Uuid,
    pub code: String,
    pub message: String,
}

pub trait SceneRepo {
    fn create(
        &self,
        home_id: Uuid,
        user_id: Uuid,
        scene: &Scene,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    fn list(
        &self,
        home_id: Uuid,
        user_id: Uuid,
    ) -> impl Future<Output = Result<Vec<Scene>, AppError>> + Send;

    fn get(
        &self,
        id: Uuid,
        user_id: Uuid,
    ) -> impl Future<Output = Result<Option<Scene>, AppError>> + Send;

    fn delete(&self, id: Uuid, user_id: Uuid) -> impl Future<Output = Result<(), AppError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_round_trip_as_tagged_json() {
        let action = SceneAction::SetDevice {
            device_id: Uuid::nil(),
            code: "switch.power".into(),
            value: serde_json::json!(true),
        };

        let json = serde_json::to_value(&action).expect("serialises");
        // The wire form has to be readable by the app and by a person debugging.
        assert_eq!(json["type"], "set_device");
        assert_eq!(json["code"], "switch.power");

        let back: SceneAction = serde_json::from_value(json).expect("deserialises");
        assert_eq!(back, action);
    }

    #[test]
    fn an_unknown_action_type_is_refused() {
        // Otherwise a scene saved by a newer app silently runs nothing.
        let json = serde_json::json!({ "type": "launch_rocket", "device_id": Uuid::nil() });
        assert!(serde_json::from_value::<SceneAction>(json).is_err());
    }
}
