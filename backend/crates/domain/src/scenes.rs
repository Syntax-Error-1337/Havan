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

/// A partial change to a scene. Every field is optional because the two callers differ: a rename sends `name`, and an
/// editor that reorders actions sends `actions`. A field that is absent is a field nobody touched.
#[derive(Debug, Clone, Default)]
pub struct SceneChange {
    pub name: Option<String>,
    pub icon: Option<Option<String>>,
    pub actions: Option<Vec<SceneAction>>,
}

impl SceneChange {
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.icon.is_none() && self.actions.is_none()
    }
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

    /// Move a scene one place towards the top (`up`) or the bottom of its home's list.
    ///
    /// The positions are rewritten densely on every move rather than swapped in place: a column that accumulates gaps
    /// eventually has two scenes claiming the same place, and the order then depends on the tie-break.
    fn move_scene(
        &self,
        home_id: Uuid,
        id: Uuid,
        user_id: Uuid,
        up: bool,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    /// Change the name, the icon, or the actions of a scene that already exists.
    ///
    /// A scene could be created and deleted and nothing in between, so a typo in a scene's name meant deleting it and
    /// rebuilding every action in it. `None` leaves a field alone — a PATCH that carried only a name must not empty
    /// the actions, and a customer renaming "Good night" should not lose the six devices behind it.
    fn update(
        &self,
        id: Uuid,
        user_id: Uuid,
        change: SceneChange,
    ) -> impl Future<Output = Result<(), AppError>> + Send;
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
