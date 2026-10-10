use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::mpsc::Sender;
use tracing::debug;
use zbus::zvariant::Value;
use zbus::{interface, object_server::SignalEmitter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuAction {
    ToggleMode,
    /// 语音输入：开始/停止听写会话（悬浮层频谱反馈，Ctrl+Alt+V 同效）。
    ToggleSpeech,
    Settings,
    Deploy,
    Exit,
    /// 切换到指定输入方案（托盘菜单动态项）。
    SelectSchema(String),
}

/// 动态方案项的起始 id（固定项占 1..10）。
const SCHEMA_ID_BASE: i32 = 10;

pub struct DBusMenu {
    revision: std::sync::atomic::AtomicU32,
    action_tx: Option<Sender<MenuAction>>,
    /// 可切换方案 (id, 显示名)；空 = 未注入（不渲染该组）。
    schemas: Mutex<Vec<(String, String)>>,
    /// 当前选中方案 id（菜单里打 ✓）。
    current_schema: Mutex<String>,
}

impl DBusMenu {
    pub fn new() -> Self {
        Self {
            revision: std::sync::atomic::AtomicU32::new(0),
            action_tx: None,
            schemas: Mutex::new(Vec::new()),
            current_schema: Mutex::new(String::new()),
        }
    }
}

impl Default for DBusMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl DBusMenu {
    pub fn with_action_channel(action_tx: Sender<MenuAction>) -> Self {
        Self {
            revision: std::sync::atomic::AtomicU32::new(0),
            action_tx: Some(action_tx),
            schemas: Mutex::new(Vec::new()),
            current_schema: Mutex::new(String::new()),
        }
    }

    /// 更新方案菜单（内容变化时 revision 自增，由调用方决定何时发信号刷新）。
    pub fn set_schemas(&self, schemas: Vec<(String, String)>, current: String) {
        let changed = {
            let mut list = self.schemas.lock().unwrap_or_else(|e| e.into_inner());
            let mut cur = self
                .current_schema
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let changed = *list != schemas || *cur != current;
            *list = schemas;
            *cur = current;
            changed
        };
        if changed {
            self.revision
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// 当前布局 revision（发 LayoutUpdated 信号用）。
    pub fn revision(&self) -> u32 {
        self.revision.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[interface(name = "com.canonical.dbusmenu")]
impl DBusMenu {
    #[zbus(signal)]
    pub async fn layout_updated(
        signal_emitter: &SignalEmitter<'_>,
        revision: u32,
        parent: i32,
    ) -> zbus::Result<()> {
    }

    #[zbus(signal)]
    async fn items_properties_updated(
        signal_emitter: &SignalEmitter<'_>,
        updated: Vec<(i32, HashMap<String, Value<'static>>)>,
        removed: Vec<(i32, Vec<String>)>,
    ) -> zbus::Result<()> {
    }

    async fn event(&self, id: i32, event_type: &str, _data: Value<'_>, _timestamp: u32) {
        if event_type == "clicked" {
            let Some(tx) = &self.action_tx else {
                return;
            };
            let action = match id {
                1 => MenuAction::ToggleMode,
                7 => MenuAction::ToggleSpeech,
                3 => MenuAction::Settings,
                4 => MenuAction::Deploy,
                5 => MenuAction::Exit,
                n if n >= SCHEMA_ID_BASE => {
                    let index = (n - SCHEMA_ID_BASE) as usize;
                    let list = self.schemas.lock().unwrap_or_else(|e| e.into_inner());
                    let Some((schema_id, _)) = list.get(index) else {
                        return;
                    };
                    MenuAction::SelectSchema(schema_id.clone())
                }
                _ => return,
            };
            debug!("Menu item {} clicked, action: {:?}", id, action);
            let _ = tx.send(action).await;
        }
    }

    fn get_property(&self, _id: i32, _property: &str) -> zbus::fdo::Result<Value<'static>> {
        Err(zbus::fdo::Error::NotSupported("Not implemented".into()))
    }

    #[allow(clippy::type_complexity)]
    #[zbus(out_args("revision", "layout"))]
    fn get_layout(
        &self,
        parent_id: i32,
        _recursion_depth: i32,
        _property_names: Vec<String>,
    ) -> zbus::fdo::Result<(
        u32,
        (i32, HashMap<String, Value<'static>>, Vec<Value<'static>>),
    )> {
        let layout = if parent_id == 0 {
            let props = HashMap::from([("children-display".to_string(), Value::new("submenu"))]);
            let mut children: Vec<Value<'static>> = vec![
                Value::new((
                    1,
                    HashMap::from([
                        ("label".to_string(), Value::new("切换中英文")),
                        ("icon-name".to_string(), Value::new("input-keyboard")),
                    ]),
                    Vec::<Value<'static>>::new(),
                )),
                Value::new((
                    2,
                    HashMap::from([("type".to_string(), Value::new("separator"))]),
                    Vec::<Value<'static>>::new(),
                )),
            ];
            // 方案切换组（对齐 Android menubar / XimeYao 托盘）：当前项打 ✓。
            {
                let list = self.schemas.lock().unwrap_or_else(|e| e.into_inner());
                let current = self
                    .current_schema
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                for (index, (schema_id, name)) in list.iter().enumerate() {
                    let label = if *schema_id == *current {
                        format!("✓ {name}")
                    } else {
                        name.clone()
                    };
                    children.push(Value::new((
                        SCHEMA_ID_BASE + index as i32,
                        HashMap::from([("label".to_string(), Value::new(label))]),
                        Vec::<Value<'static>>::new(),
                    )));
                }
            }
            children.push(Value::new((
                // 分隔线不参与点击，但 dbusmenu 规范要求同层 id 唯一
                //（客户端按 id 缓存属性/路由属性更新）；第一个分隔线占 2，
                // 6 未被任何动作占用（1/3/4/5/7 = 动作，≥10 = 方案）。
                6,
                HashMap::from([("type".to_string(), Value::new("separator"))]),
                Vec::<Value<'static>>::new(),
            )));
            children.extend([
                Value::new((
                    7,
                    HashMap::from([
                        ("label".to_string(), Value::new("语音输入")),
                        (
                            "icon-name".to_string(),
                            Value::new("audio-input-microphone"),
                        ),
                    ]),
                    Vec::<Value<'static>>::new(),
                )),
                Value::new((
                    // 分隔线占 8（1/3/4/5/7 = 动作，≥10 = 方案，2/6/8 = 分隔线）
                    8,
                    HashMap::from([("type".to_string(), Value::new("separator"))]),
                    Vec::<Value<'static>>::new(),
                )),
                Value::new((
                    3,
                    HashMap::from([
                        ("label".to_string(), Value::new("设置...")),
                        ("icon-name".to_string(), Value::new("preferences-system")),
                    ]),
                    Vec::<Value<'static>>::new(),
                )),
                Value::new((
                    4,
                    HashMap::from([
                        ("label".to_string(), Value::new("重新部署")),
                        ("icon-name".to_string(), Value::new("view-refresh")),
                    ]),
                    Vec::<Value<'static>>::new(),
                )),
                Value::new((
                    5,
                    HashMap::from([
                        ("label".to_string(), Value::new("退出")),
                        ("icon-name".to_string(), Value::new("application-exit")),
                    ]),
                    Vec::<Value<'static>>::new(),
                )),
            ]);
            (0, props, children)
        } else {
            (parent_id, HashMap::new(), Vec::new())
        };
        Ok((
            self.revision.load(std::sync::atomic::Ordering::Relaxed),
            layout,
        ))
    }

    fn get_group_properties(
        &self,
        _ids: Vec<i32>,
        _property_names: Vec<String>,
    ) -> Vec<(i32, HashMap<String, Value<'static>>)> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_action_variants_distinct() {
        // SelectSchema 带字段后枚举不再支持 as-cast，改验判别互异性。
        assert_ne!(
            std::mem::discriminant(&MenuAction::ToggleMode),
            std::mem::discriminant(&MenuAction::Settings)
        );
        assert_ne!(
            std::mem::discriminant(&MenuAction::ToggleSpeech),
            std::mem::discriminant(&MenuAction::ToggleMode)
        );
        assert_ne!(
            std::mem::discriminant(&MenuAction::Deploy),
            std::mem::discriminant(&MenuAction::Exit)
        );
        assert_eq!(
            MenuAction::SelectSchema("wubi86".into()),
            MenuAction::SelectSchema("wubi86".into())
        );
    }

    #[test]
    fn test_menu_action_debug() {
        let action = MenuAction::ToggleMode;
        let debug = format!("{:?}", action);
        assert_eq!(debug, "ToggleMode");
    }

    #[test]
    fn test_menu_action_equality() {
        assert_eq!(MenuAction::ToggleMode, MenuAction::ToggleMode);
        assert_ne!(MenuAction::ToggleMode, MenuAction::Exit);
        assert_ne!(MenuAction::Deploy, MenuAction::Settings);
    }

    #[test]
    fn test_menu_action_clone() {
        let original = MenuAction::Exit;
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    #[test]
    fn test_dbusmenu_new() {
        let menu = DBusMenu::new();
        assert!(menu.action_tx.is_none());
        assert_eq!(menu.revision(), 0);
    }

    #[test]
    fn test_dbusmenu_default() {
        let menu = DBusMenu::default();
        assert!(menu.action_tx.is_none());
        assert_eq!(menu.revision(), 0);
    }
}
