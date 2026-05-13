use zbus::Connection;
use zbus::object_server::InterfaceRef;
use crate::{StatusNotifierItem, DBusMenu, InputMode, MenuAction};
use crate::sni::StatusNotifierItemSignals;
use tokio::sync::mpsc::{channel, Receiver};

const SNI_WATCHER_SERVICE: &str = "org.kde.StatusNotifierWatcher";
const SNI_WATCHER_OBJECT: &str = "/StatusNotifierWatcher";
const SNI_WATCHER_INTERFACE: &str = "org.kde.StatusNotifierWatcher";
const SNI_OBJECT: &str = "/StatusNotifierItem";
const MENU_OBJECT: &str = "/MenuBar";

pub struct TrayManager {
    sni_ref: InterfaceRef<StatusNotifierItem>,
}

impl TrayManager {
    pub async fn register(connection: &Connection) -> zbus::Result<(Self, Receiver<()>, Receiver<MenuAction>)> {
        let (toggle_tx, toggle_rx) = channel::<()>(1);
        let (action_tx, action_rx) = channel::<MenuAction>(1);
        
        connection.object_server().at(MENU_OBJECT, DBusMenu::with_action_channel(action_tx)).await?;
        connection.object_server().at(SNI_OBJECT, StatusNotifierItem::with_toggle_channel(toggle_tx)).await?;
        
        let sni_ref = connection.object_server()
            .interface::<_, StatusNotifierItem>(SNI_OBJECT).await?;
        
        connection.call_method(
            Some(SNI_WATCHER_SERVICE),
            SNI_WATCHER_OBJECT,
            Some(SNI_WATCHER_INTERFACE),
            "RegisterStatusNotifierItem",
            &(connection.unique_name().map(|n| n.to_string()).unwrap_or_default()),
        ).await?;
        
        eprintln!("DEBUG: SNI registered successfully (initially hidden)");
        Ok((Self { sni_ref }, toggle_rx, action_rx))
    }
    
    pub async fn set_mode(&self, mode: InputMode) {
        let iface = self.sni_ref.get_mut().await;
        iface.set_mode(mode);
        self.sni_ref.new_icon().await.ok();
        self.sni_ref.new_tool_tip().await.ok();
    }
    
    pub async fn set_visible(&self, visible: bool) {
        let iface = self.sni_ref.get_mut().await;
        let was_visible = iface.is_visible();
        iface.set_visible(visible);
        
        if was_visible != visible {
            let status = if visible { "Active" } else { "Passive" };
            self.sni_ref.new_status(status).await.ok();
            if visible {
                self.sni_ref.new_icon().await.ok();
            }
            eprintln!("DEBUG: Tray visibility changed to {}", status);
        }
    }
    
    pub async fn get_mode(&self) -> InputMode {
        let iface = self.sni_ref.get().await;
        iface.get_mode()
    }
}