use zbus::{interface, object_server::SignalEmitter};
use zbus::zvariant::Value;
use std::collections::HashMap;
use tokio::sync::mpsc::Sender;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    ToggleMode,
    Deploy,
    Exit,
}

pub struct DBusMenu {
    revision: u32,
    action_tx: Option<Sender<MenuAction>>,
}

impl DBusMenu {
    pub fn new() -> Self {
        Self { revision: 0, action_tx: None }
    }
    
    pub fn with_action_channel(action_tx: Sender<MenuAction>) -> Self {
        Self { revision: 0, action_tx: Some(action_tx) }
    }
}

#[interface(name = "com.canonical.dbusmenu")]
impl DBusMenu {
    #[zbus(signal)]
    async fn layout_updated(signal_emitter: &SignalEmitter<'_>, revision: u32, parent: i32) -> zbus::Result<()> {}
    
    #[zbus(signal)]
    async fn items_properties_updated(
        signal_emitter: &SignalEmitter<'_>,
        updated: Vec<(i32, HashMap<String, Value<'static>>)>,
        removed: Vec<(i32, Vec<String>)>,
    ) -> zbus::Result<()> {}
    
    async fn event(&self, id: i32, event_type: &str, _data: Value<'_>, _timestamp: u32) {
        if event_type == "clicked" {
            if let Some(tx) = &self.action_tx {
                let action = match id {
                    1 => MenuAction::ToggleMode,
                    3 => MenuAction::Deploy,
                    4 => MenuAction::Exit,
                    _ => return,
                };
                let _ = tx.send(action).await;
                eprintln!("DEBUG: Menu item {} clicked, action: {:?}", id, action);
            }
        }
    }
    
    fn get_property(&self, _id: i32, _property: &str) -> zbus::fdo::Result<Value<'static>> {
        Err(zbus::fdo::Error::NotSupported("Not implemented".into()))
    }
    
    #[zbus(out_args("revision", "layout"))]
    fn get_layout(&self, parent_id: i32, _recursion_depth: i32, _property_names: Vec<String>) 
        -> zbus::fdo::Result<(u32, (i32, HashMap<String, Value<'static>>, Vec<Value<'static>>))> {
        let layout = if parent_id == 0 {
            let props = HashMap::from([
                ("children-display".to_string(), Value::new("submenu")),
            ]);
            let children: Vec<Value<'static>> = vec![
                Value::new((1, HashMap::from([
                    ("label".to_string(), Value::new("切换中英文")),
                    ("icon-name".to_string(), Value::new("input-keyboard")),
                ]), Vec::<Value<'static>>::new())),
                Value::new((2, HashMap::from([
                    ("type".to_string(), Value::new("separator")),
                ]), Vec::<Value<'static>>::new())),
                Value::new((3, HashMap::from([
                    ("label".to_string(), Value::new("重新部署")),
                    ("icon-name".to_string(), Value::new("view-refresh")),
                ]), Vec::<Value<'static>>::new())),
                Value::new((4, HashMap::from([
                    ("label".to_string(), Value::new("退出")),
                    ("icon-name".to_string(), Value::new("application-exit")),
                ]), Vec::<Value<'static>>::new())),
            ];
            (0, props, children)
        } else {
            (parent_id, HashMap::new(), Vec::new())
        };
        Ok((self.revision, layout))
    }
    
    fn get_group_properties(&self, _ids: Vec<i32>, _property_names: Vec<String>) 
        -> Vec<(i32, HashMap<String, Value<'static>>)> {
        Vec::new()
    }
    
    fn about_to_show(&self, id: i32) -> bool {
        id == 0
    }
    
    #[zbus(property)]
    fn version(&self) -> u32 {
        2
    }
    
    #[zbus(property)]
    fn status(&self) -> &str {
        "normal"
    }
}