use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteSound {
    pub id: String,
    pub name: String,
    pub volume: f32,
    pub has_image: bool,
    /// Tile color as `#RRGGBB`.
    pub color: String,
    /// Clip length in seconds once decoded; None until the decode cache has it.
    pub duration_secs: Option<f32>,
    /// Bumped on every successful play so clients can restart the progress sweep.
    pub play_seq: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    State { master_volume: f32, playing: Vec<String>, sounds: Vec<RemoteSound> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    Play { id: String },
    StopAll,
    SetMasterVolume { value: f32 },
    SetSoundVolume { id: String, value: f32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_state_serializes_with_type_tag() {
        let m = ServerMsg::State {
            master_volume: 0.8,
            playing: vec!["u1".into()],
            sounds: vec![RemoteSound { id: "u1".into(), name: "Airhorn".into(), volume: 1.0, has_image: true, color: "#FF595E".into(), duration_secs: Some(2.5), play_seq: 3 }],
        };
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(
            json,
            r##"{"type":"state","master_volume":0.8,"playing":["u1"],"sounds":[{"id":"u1","name":"Airhorn","volume":1.0,"has_image":true,"color":"#FF595E","duration_secs":2.5,"play_seq":3}]}"##
        );
    }

    #[test]
    fn client_messages_parse() {
        assert_eq!(serde_json::from_str::<ClientMsg>(r#"{"type":"play","id":"u1"}"#).unwrap(), ClientMsg::Play { id: "u1".into() });
        assert_eq!(serde_json::from_str::<ClientMsg>(r#"{"type":"stop_all"}"#).unwrap(), ClientMsg::StopAll);
        assert_eq!(serde_json::from_str::<ClientMsg>(r#"{"type":"set_master_volume","value":0.5}"#).unwrap(), ClientMsg::SetMasterVolume { value: 0.5 });
        assert_eq!(
            serde_json::from_str::<ClientMsg>(r#"{"type":"set_sound_volume","id":"u1","value":0.25}"#).unwrap(),
            ClientMsg::SetSoundVolume { id: "u1".into(), value: 0.25 }
        );
        assert!(serde_json::from_str::<ClientMsg>(r#"{"type":"explode"}"#).is_err());
    }
}
