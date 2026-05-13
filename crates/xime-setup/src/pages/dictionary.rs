use gpui::*;
use crate::components::{SettingsPage, SettingsGroup, SettingsItem, SettingsControl};
use crate::state::SettingsState;
use crate::pages::SettingsApp;

fn get_user_data_dir() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    format!("{}/.config/xime/rime", home)
}

pub fn render(settings: Entity<SettingsState>, cx: &mut Context<SettingsApp>) -> AnyElement {
    let colors = cx.read_entity(&settings, |state, _| state.colors());
    let path_str = get_user_data_dir();

    SettingsPage::new("词库管理", colors.clone())
        .group(
            SettingsGroup::new("词库操作", colors.clone())
                .items(vec![
                    SettingsItem::new("导入词库", SettingsControl::button("导入"))
                        .description("从文件导入用户词库"),
                    SettingsItem::new("导出词库", SettingsControl::button("导出"))
                        .description("导出用户词库到文件"),
                    SettingsItem::new("同步词库", SettingsControl::button("同步"))
                        .description("重新编译词库"),
                ])
        )
        .group(
            SettingsGroup::new("词库信息", colors.clone())
                .items(vec![
                    SettingsItem::new("词库路径", SettingsControl::label(path_str))
                        .description("用户词库存储位置"),
                    SettingsItem::new("清空词库", SettingsControl::button("清空"))
                        .description("清空用户词库，恢复默认状态"),
                ])
        )
        .group(
            SettingsGroup::new("添加词条", colors.clone())
                .items(vec![
                    SettingsItem::new("添加词条", SettingsControl::button("添加"))
                        .description("手动添加新的词条"),
                ])
        )
        .into_any_element()
}