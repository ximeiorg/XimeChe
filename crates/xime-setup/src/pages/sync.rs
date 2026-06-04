use crate::components::{SettingsItem, TextInput};
use crate::pages::SettingsApp;
use crate::state::SettingsState;
use crate::theme::ThemeColors;
use crate::webdav::{WebdavConfig, WebdavSync};
use gpui::{prelude::FluentBuilder, *};

pub fn render(
    settings: Entity<SettingsState>,
    url_input: Entity<TextInput>,
    username_input: Entity<TextInput>,
    password_input: Entity<TextInput>,
    remote_dir_input: Entity<TextInput>,
    cx: &mut Context<SettingsApp>,
) -> AnyElement {
    let colors = cx.read_entity(&settings, |state, _| state.colors());
    let sync_state = cx.read_entity(&settings, |state, _| state.sync.clone());

    div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .p(px(16.0))
        .w_full()
        .child(
            div()
                .text_size(px(20.0))
                .font_weight(FontWeight::BOLD)
                .text_color(colors.foreground)
                .child("WebDAV 同步"),
        )
        .child(render_config_section(
            &colors,
            &url_input,
            &username_input,
            &password_input,
            &remote_dir_input,
            settings.clone(),
            cx,
        ))
        .child(render_actions_section(
            &colors,
            &sync_state,
            url_input,
            username_input,
            password_input,
            remote_dir_input,
            settings.clone(),
            cx,
        ))
        .when_some(sync_state.status_message.clone(), |this, msg| {
            let status_color = match sync_state.status {
                crate::state::SyncStatus::Success => colors.primary,
                crate::state::SyncStatus::Error => colors.error,
                crate::state::SyncStatus::Idle => colors.foreground_muted,
            };
            this.child(
                div()
                    .mt(px(8.0))
                    .px(px(20.0))
                    .py(px(12.0))
                    .rounded(px(12.0))
                    .bg(colors.surface)
                    .border_1()
                    .border_color(colors.border)
                    .text_size(px(14.0))
                    .text_color(status_color)
                    .child(msg),
            )
        })
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn render_config_section(
    colors: &ThemeColors,
    url_input: &Entity<TextInput>,
    username_input: &Entity<TextInput>,
    password_input: &Entity<TextInput>,
    remote_dir_input: &Entity<TextInput>,
    settings: Entity<SettingsState>,
    _cx: &mut Context<SettingsApp>,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .py(px(16.0))
        .px(px(16.0))
        .rounded(px(12.0))
        .bg(colors.surface)
        .border_1()
        .border_color(colors.border)
        .child(
            div()
                .text_size(px(16.0))
                .font_weight(FontWeight::BOLD)
                .text_color(colors.foreground)
                .child("服务器配置"),
        )
        .child(SettingsItem::render_custom(
            colors,
            "服务器地址",
            Some("WebDAV 服务器地址，例如 https://example.com/remote.php/dav/files/user/xime/"),
            div().child(url_input.clone()),
        ))
        .child(SettingsItem::render_custom(
            colors,
            "用户名",
            Some("WebDAV 登录用户名"),
            div().child(username_input.clone()),
        ))
        .child(SettingsItem::render_custom(
            colors,
            "密码",
            Some("WebDAV 登录密码（保存在 ~/.config/xime/webdav.yaml，权限 600）"),
            div().child(password_input.clone()),
        ))
        .child(SettingsItem::render_custom(
            colors,
            "远程目录",
            Some("服务器上的目录名，默认为 xime"),
            div().child(remote_dir_input.clone()),
        ))
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .mt(px(8.0))
                .child(
                    div()
                        .id("test-webdav-btn")
                        .py(px(8.0))
                        .px(px(20.0))
                        .rounded(px(10.0))
                        .bg(colors.surface_variant)
                        .text_color(colors.foreground)
                        .text_size(px(14.0))
                        .cursor_pointer()
                        .hover(|style| style.bg(colors.border_variant))
                        .border_1()
                        .border_color(colors.border_variant)
                        .child("测试连接")
                        .on_click({
                            let url_input = url_input.clone();
                            let username_input = username_input.clone();
                            let password_input = password_input.clone();
                            let remote_dir_input = remote_dir_input.clone();
                            let settings = settings.clone();
                            move |_event, _window, cx| {
                                let url =
                                    cx.read_entity(&url_input, |i, _| i.content().to_string());
                                let username =
                                    cx.read_entity(&username_input, |i, _| i.content().to_string());
                                let password =
                                    cx.read_entity(&password_input, |i, _| i.content().to_string());
                                let remote_dir = cx
                                    .read_entity(&remote_dir_input, |i, _| i.content().to_string());
                                let config = WebdavConfig {
                                    url,
                                    username,
                                    password,
                                    remote_dir,
                                };

                                cx.update_entity(&settings, |s, cx| {
                                    s.sync.status = crate::state::SyncStatus::Idle;
                                    s.sync.status_message = Some("正在测试连接...".to_string());
                                    cx.notify();
                                });

                                let result = WebdavSync::test_connection(&config);
                                send_notification(&result);

                                cx.update_entity(&settings, |s, cx| {
                                    match &result {
                                        Ok(msg) => {
                                            s.sync.status = crate::state::SyncStatus::Success;
                                            s.sync.status_message = Some(msg.clone());
                                        }
                                        Err(e) => {
                                            s.sync.status = crate::state::SyncStatus::Error;
                                            s.sync.status_message = Some(e.clone());
                                        }
                                    }
                                    cx.notify();
                                });
                            }
                        }),
                )
                .child(
                    div()
                        .id("save-webdav-config-btn")
                        .py(px(8.0))
                        .px(px(20.0))
                        .rounded(px(10.0))
                        .bg(colors.primary)
                        .text_color(colors.on_primary)
                        .text_size(px(14.0))
                        .cursor_pointer()
                        .hover(|style| style.bg(colors.primary_hover))
                        .child("保存配置")
                        .on_click({
                            let url_input = url_input.clone();
                            let username_input = username_input.clone();
                            let password_input = password_input.clone();
                            let remote_dir_input = remote_dir_input.clone();
                            move |_event, _window, cx| {
                                let url =
                                    cx.read_entity(&url_input, |i, _| i.content().to_string());
                                let username =
                                    cx.read_entity(&username_input, |i, _| i.content().to_string());
                                let password =
                                    cx.read_entity(&password_input, |i, _| i.content().to_string());
                                let remote_dir = cx
                                    .read_entity(&remote_dir_input, |i, _| i.content().to_string());
                                let config = WebdavConfig {
                                    url,
                                    username,
                                    password,
                                    remote_dir,
                                };
                                let save_ok = config.save().is_ok();
                                if save_ok {
                                    send_notification_msg("配置已保存", false);
                                    cx.update_entity(&settings, |s, cx| {
                                        s.sync.status = crate::state::SyncStatus::Success;
                                        s.sync.status_message = Some("配置已保存".to_string());
                                        cx.notify();
                                    });
                                } else {
                                    send_notification_msg("配置保存失败", true);
                                    cx.update_entity(&settings, |s, cx| {
                                        s.sync.status = crate::state::SyncStatus::Error;
                                        s.sync.status_message = Some("配置保存失败".to_string());
                                        cx.notify();
                                    });
                                }
                            }
                        }),
                ),
        )
}

#[allow(clippy::too_many_arguments)]
fn render_actions_section(
    colors: &ThemeColors,
    sync_state: &crate::state::SyncState,
    url_input: Entity<TextInput>,
    username_input: Entity<TextInput>,
    password_input: Entity<TextInput>,
    remote_dir_input: Entity<TextInput>,
    settings: Entity<SettingsState>,
    _cx: &mut Context<SettingsApp>,
) -> Div {
    let is_syncing = sync_state.is_syncing;

    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .py(px(16.0))
        .px(px(16.0))
        .rounded(px(12.0))
        .bg(colors.surface)
        .border_1()
        .border_color(colors.border)
        .child(
            div()
                .text_size(px(16.0))
                .font_weight(FontWeight::BOLD)
                .text_color(colors.foreground)
                .child("同步操作"),
        )
        .child(
            div()
                .text_size(px(13.0))
                .text_color(colors.foreground_muted)
                .child("将 ~/.config/xime/ 目录上传到服务器或从服务器下载。"),
        )
        .child(
            div()
                .flex()
                .gap(px(12.0))
                .mt(px(4.0))
                .child(
                    div()
                        .id("sync-upload-btn")
                        .py(px(10.0))
                        .px(px(24.0))
                        .rounded(px(10.0))
                        .bg(colors.primary)
                        .text_color(colors.on_primary)
                        .text_size(px(14.0))
                        .font_weight(FontWeight::MEDIUM)
                        .cursor_pointer()
                        .when(is_syncing, |this| this.opacity(0.5))
                        .hover(|style| style.bg(colors.primary_hover))
                        .child("☁ 上传到服务器")
                        .on_click({
                            let url_input = url_input.clone();
                            let username_input = username_input.clone();
                            let password_input = password_input.clone();
                            let remote_dir_input = remote_dir_input.clone();
                            let settings = settings.clone();
                            move |_event, _window, cx| {
                                let config = get_config(
                                    cx,
                                    &url_input,
                                    &username_input,
                                    &password_input,
                                    &remote_dir_input,
                                );
                                if config.is_none() {
                                    cx.update_entity(&settings, |s, cx| {
                                        s.sync.status = crate::state::SyncStatus::Error;
                                        s.sync.status_message =
                                            Some("请先保存 WebDAV 配置".to_string());
                                        cx.notify();
                                    });
                                    return;
                                }
                                let config = config.unwrap();
                                cx.update_entity(&settings, |s, cx| {
                                    s.sync.is_syncing = true;
                                    s.sync.status = crate::state::SyncStatus::Idle;
                                    s.sync.status_message = Some("正在上传...".to_string());
                                    cx.notify();
                                });
                                let result = WebdavSync::upload(&config);
                                send_notification(&result);
                                cx.update_entity(&settings, |s, cx| {
                                    s.sync.is_syncing = false;
                                    match result {
                                        Ok(msg) => {
                                            s.sync.status = crate::state::SyncStatus::Success;
                                            s.sync.status_message = Some(msg);
                                        }
                                        Err(e) => {
                                            s.sync.status = crate::state::SyncStatus::Error;
                                            s.sync.status_message = Some(e);
                                        }
                                    }
                                    cx.notify();
                                });
                            }
                        }),
                )
                .child(
                    div()
                        .id("sync-download-btn")
                        .py(px(10.0))
                        .px(px(24.0))
                        .rounded(px(10.0))
                        .bg(colors.surface_variant)
                        .text_color(colors.foreground)
                        .text_size(px(14.0))
                        .font_weight(FontWeight::MEDIUM)
                        .cursor_pointer()
                        .when(is_syncing, |this| this.opacity(0.5))
                        .hover(|style| style.bg(colors.border_variant))
                        .border_1()
                        .border_color(colors.border_variant)
                        .child("☁ 从服务器下载")
                        .on_click({
                            let url_input = url_input.clone();
                            let username_input = username_input.clone();
                            let password_input = password_input.clone();
                            let remote_dir_input = remote_dir_input.clone();
                            let settings = settings.clone();
                            move |_event, _window, cx| {
                                let config = get_config(
                                    cx,
                                    &url_input,
                                    &username_input,
                                    &password_input,
                                    &remote_dir_input,
                                );
                                if config.is_none() {
                                    cx.update_entity(&settings, |s, cx| {
                                        s.sync.status = crate::state::SyncStatus::Error;
                                        s.sync.status_message =
                                            Some("请先保存 WebDAV 配置".to_string());
                                        cx.notify();
                                    });
                                    return;
                                }
                                let config = config.unwrap();
                                cx.update_entity(&settings, |s, cx| {
                                    s.sync.is_syncing = true;
                                    s.sync.status = crate::state::SyncStatus::Idle;
                                    s.sync.status_message = Some("正在下载...".to_string());
                                    cx.notify();
                                });
                                let result = WebdavSync::download(&config);
                                send_notification(&result);
                                cx.update_entity(&settings, |s, cx| {
                                    s.sync.is_syncing = false;
                                    match result {
                                        Ok(msg) => {
                                            s.sync.status = crate::state::SyncStatus::Success;
                                            s.sync.status_message = Some(msg);
                                        }
                                        Err(e) => {
                                            s.sync.status = crate::state::SyncStatus::Error;
                                            s.sync.status_message = Some(e);
                                        }
                                    }
                                    cx.notify();
                                });
                            }
                        }),
                ),
        )
}

/// 发送桌面通知（使用 notify-send）
fn send_notification(result: &Result<String, String>) {
    let (summary, body, urgency) = match result {
        Ok(msg) => ("Xime", msg.as_str(), "normal"),
        Err(e) => ("Xime 错误", e.as_str(), "critical"),
    };
    send_notify_raw(summary, body, urgency);
}

/// 发送桌面通知（简化版）
fn send_notification_msg(msg: &str, is_error: bool) {
    let urgency = if is_error { "critical" } else { "normal" };
    send_notify_raw("Xime", msg, urgency);
}

fn send_notify_raw(summary: &str, body: &str, urgency: &str) {
    let _ = std::process::Command::new("notify-send")
        .args(["--urgency", urgency, "--app-name", "Xime", summary, body])
        .output();
}

fn get_config(
    cx: &mut App,
    url_input: &Entity<TextInput>,
    username_input: &Entity<TextInput>,
    password_input: &Entity<TextInput>,
    remote_dir_input: &Entity<TextInput>,
) -> Option<WebdavConfig> {
    let saved = WebdavConfig::load();
    if saved.is_valid() {
        return Some(saved);
    }
    let url = cx.read_entity(url_input, |i, _| i.content().to_string());
    let username = cx.read_entity(username_input, |i, _| i.content().to_string());
    let password = cx.read_entity(password_input, |i, _| i.content().to_string());
    let remote_dir = cx.read_entity(remote_dir_input, |i, _| i.content().to_string());
    let config = WebdavConfig {
        url,
        username,
        password,
        remote_dir,
    };
    if config.is_valid() {
        config.save().ok()?;
        Some(config)
    } else {
        None
    }
}
