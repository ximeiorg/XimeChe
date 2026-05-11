# XIME-Wayland 开发进度

## 当前状态
**基本输入法功能已完成！候选窗口基础实现完成！**
- KWin input-method-v1 连接成功
- Rime 引擎部署成功（luna_pinyin 方案）
- 键盘事件处理机制已实现
- keymap fd 和 xkb 转换已实现
- zwp_input_panel_surface_v1 + set_overlay_panel() 已实现
- 基础 SHM buffer 创建已实现

## 今日完成
1. **候选窗口 Wayland 基础实现**
   - 添加 wl_compositor 和 wl_shm 绑定
   - 实现 create_candidate_surface() 创建 panel surface
   - 实现 show_candidate_window() 显示候选框
   - 实现 hide_candidate_window() 隐藏候选框
   - 使用 zwp_input_panel_surface_v1.set_overlay_panel() 定位

2. **SHM buffer 基础**
   - 使用 nix crate 创建匿名文件 (O_TMPFILE)
   - 实现 create_shm_pool() 和 create_buffer()

3. **集成到主循环**
   - 当有候选词时显示候选窗口
   - 无候选词时隐藏候选窗口
   - deactivate 时隐藏候选窗口

## 待完成
1. **候选窗口内容渲染**
   - 当前只显示空白窗口
   - 需要绘制候选词文本
   - 需要实现 Cairo 或类似渲染

2. **光标跟随优化**
   - set_overlay_panel() 由 compositor 定位
   - 需要验证实际定位效果

3. **候选窗口交互**
   - 点击选择候选词
   - 翻页功能
   - 高亮显示

## 测试方法
打开 Kate 或其他支持 text-input 的应用，点击文本框，输入拼音触发候选词。

## 已完成模块

### 1. librime-sys (FFI 绑定) ✅
### 2. librime (Rime 引擎封装) ✅
### 3. xime-wayland (Wayland 协议层) ✅
- input-method-v1 协议绑定成功
- keyboard grab 按键捕获已实现
- keymap fd 处理已实现
- wl_compositor 绑定 ✅
- wl_shm 绑定 ✅
- zwp_input_panel_surface_v1 绑定 ✅
- 基础 SHM buffer 创建 ✅

### 4. xime-xkb (键码转换层) ✅
- key_from_keycode 转换
- set_keymap_from_owned_fd 处理 Wayland keymap

### 5. xime-ui (UI 层) 🔄
- CandidateList 数据结构 ✅
- 基础 slint 组件已创建（未使用）
- Wayland surface 基础 ✅
- 内容渲染待实现

### 6. xime-core (主程序) ✅
- 自动协议检测
- Rime 部署和初始化
- 按键处理主循环
- 基础候选窗口集成 ✅

## 运行状态
```
DEBUG: WAYLAND_SOCKET=242 at startup
DEBUG: input-method-v1 available, using v1 backend
DEBUG: Deploy success
DEBUG: Input method v1 running event loop...
DEBUG: Created candidate panel surface with overlay_panel
DEBUG: Candidate window shown (when candidates appear)
```

## 参考资料发现
- fcitx5 使用 zwp_input_panel_surface_v1.set_overlay_panel() 定位候选框
- compositor 负责定位，无需手动计算光标位置
- fcitx5 对 XWayland 应用使用 X11 窗口作为 fallback（因为 Wayland 窗口无法自由定位）
- 对于纯 Wayland 应用，overlay_panel 由 compositor 在光标附近定位

## 下一步
1. 实现候选窗口内容渲染（绘制候选词文本）
2. 测试实际运行效果
3. 优化 UI 样式和交互