//! 语音悬浮面板的 layer-shell 实现（v1 / v2 共用）。
//!
//! 为什么不用 `zwp_input_panel_surface_v1`：那是给**屏幕键盘**用的面板协议。
//! KWin 6 实现了它，但只显示「键盘 surface 所属」的面板；第三方输入法创建的
//! input panel surface 建得出来、commit 也不报错，就是**不显示**（2026-11-18
//! 实测：surface 创建成功后屏幕上什么都没有，语音期间用户只能看到候选栏）。
//!
//! `zwlr_layer_shell_v1` 是通用方案：KWin 6 给 v5、cosmic-comp 给 v4、
//! wlroots 系全有。用 overlay 层 + 两侧锚定（左右等宽 margin 换居中）+ 固定
//! 高度，得到「屏幕底部居中、不占工作区、不吃键盘」的浮层，与商业输入法的
//! 听写面板形态一致。
//!
//! configure 之前 attach buffer 无效，所以首次 `show` 只建 surface 便返回；
//! daemon 在语音期间有 ~30fps 重绘节拍，最迟下一帧补上画面。

use std::os::unix::io::{AsFd, OwnedFd};
use std::slice;
use std::sync::{Arc, Mutex};

use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_shm::WlShm;
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Dispatch, QueueHandle};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::{
    Layer, ZwlrLayerShellV1,
};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{
    Anchor, KeyboardInteractivity, ZwlrLayerSurfaceV1,
};

use tracing::debug;
use xime_ui::voice::{self, VoiceView};
use xime_ui::{IcedSurface, PanelTheme};

/// 面板的 layer shell 资源（surface 只建一次，每帧只换 buffer）。
pub struct VoicePanel {
    layer_shell: Option<ZwlrLayerShellV1>,
    layer: Option<ZwlrLayerSurfaceV1>,
    surface: Option<WlSurface>,
    buffer: Option<WlBuffer>,
    pool: Option<WlShmPool>,
    renderer: Option<IcedSurface>,
}

/// 后端是否提供 `zwlr_layer_shell_v1`（没有就没有语音面板）。
pub fn bind_layer_shell<D>(
    globals: &wayland_client::globals::GlobalList,
    qh: &QueueHandle<D>,
    data: D,
) -> Option<ZwlrLayerShellV1>
where
    D: Dispatch<ZwlrLayerShellV1, D> + Send + Sync + 'static,
{
    // KWin 6 给 v5、cosmic-comp 给 v4；功能上只用 v1 子集。
    globals.bind(qh, 1..=5, data).ok()
}

impl VoicePanel {
    pub fn new(layer_shell: Option<ZwlrLayerShellV1>) -> Self {
        Self {
            layer_shell,
            layer: None,
            surface: None,
            buffer: None,
            pool: None,
            renderer: None,
        }
    }

    pub fn available(&self) -> bool {
        self.layer_shell.is_some()
    }

    /// 绘制一帧。首次调用会创建 layer surface 并请求 configure（此时返回
    /// `Ok` 但画面还没出，下一帧补上）。
    #[allow(clippy::too_many_arguments)]
    pub fn show<D>(
        &mut self,
        compositor: &WlCompositor,
        shm: &WlShm,
        qh: &QueueHandle<D>,
        data: D,
        configured: &Arc<Mutex<bool>>,
        slot: &Arc<Mutex<Option<WlSurface>>>,
        view: &VoiceView,
        theme: &PanelTheme,
    ) -> Result<(), String>
    where
        D: Dispatch<WlSurface, D>
            + Dispatch<WlBuffer, D>
            + Dispatch<WlShmPool, D>
            + Dispatch<ZwlrLayerShellV1, D>
            + Dispatch<ZwlrLayerSurfaceV1, D>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        let width = voice::VOICE_WIDTH;
        let height = voice::VOICE_HEIGHT;

        let is_configured = configured.lock().map(|c| *c).unwrap_or(false);
        debug!(
            "Voice panel show: layer={} configured={is_configured}",
            self.layer.is_some()
        );
        if self.layer.is_none() || !is_configured {
            // closed 过（合成器要求移除）或还没建：重建一次。
            if self.layer.is_some() {
                self.destroy();
            }
            self.create(compositor, qh, data.clone(), configured, slot)?;
            return Ok(());
        }

        let surface = self
            .surface
            .clone()
            .ok_or_else(|| "voice panel surface missing".to_string())?;

        if let Some(buffer) = self.buffer.take() {
            buffer.destroy();
        }
        if let Some(pool) = self.pool.take() {
            pool.destroy();
        }

        let stride = width * 4;
        let size = stride * height;
        let fd = create_anonymous_file(size)?;
        let pool = shm.create_pool(fd.as_fd(), size as i32, qh, data.clone());
        self.pool = Some(pool.clone());

        let mut renderer = self.renderer.take().unwrap_or_default();
        {
            let buf_size = (width * height * 4) as usize;
            let size_nonzero = std::num::NonZero::new(buf_size).expect("size is non-zero");
            let ptr = unsafe {
                nix::sys::mman::mmap(
                    None,
                    size_nonzero,
                    nix::sys::mman::ProtFlags::PROT_READ | nix::sys::mman::ProtFlags::PROT_WRITE,
                    nix::sys::mman::MapFlags::MAP_SHARED,
                    &fd,
                    0,
                )
                .map_err(|e| format!("mmap voice panel: {e}"))?
            };
            let pixels: &mut [u8] =
                unsafe { slice::from_raw_parts_mut(ptr.as_ptr() as *mut u8, buf_size) };
            renderer.draw_voice_overlay(pixels, view, theme);
            unsafe {
                nix::sys::mman::munmap(ptr, buf_size)
                    .map_err(|e| format!("munmap voice panel: {e}"))?;
            }
        }
        self.renderer = Some(renderer);

        let buffer = pool.create_buffer(
            0,
            width as i32,
            height as i32,
            stride as i32,
            wayland_client::protocol::wl_shm::Format::Argb8888,
            qh,
            data,
        );
        self.buffer = Some(buffer.clone());

        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, width as i32, height as i32);
        surface.commit();
        Ok(())
    }

    fn create<D>(
        &mut self,
        compositor: &WlCompositor,
        qh: &QueueHandle<D>,
        data: D,
        configured: &Arc<Mutex<bool>>,
        slot: &Arc<Mutex<Option<WlSurface>>>,
    ) -> Result<(), String>
    where
        D: Dispatch<WlSurface, D>
            + Dispatch<ZwlrLayerShellV1, D>
            + Dispatch<ZwlrLayerSurfaceV1, D>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        let layer_shell = self
            .layer_shell
            .as_ref()
            .ok_or_else(|| "compositor has no zwlr_layer_shell_v1".to_string())?;
        let width = voice::VOICE_WIDTH;
        let height = voice::VOICE_HEIGHT;

        let surface = compositor.create_surface(qh, data.clone());
        let layer = layer_shell.get_layer_surface(
            &surface,
            None, // 交给合成器选屏（单屏即当前屏）
            Layer::Overlay,
            "xime-voice".to_string(),
            qh,
            data,
        );
        // 水平居中靠「两侧锚定 + 等宽 margin」实现。
        //
        // layer shell 没有「居中」这个锚点：只锚底边时水平位置由合成器定
        // （KWin 实测是贴左边，不是居中）；两侧都锚定则宽度由 margin 决定。
        // 所以先按满宽配置拿 configure 给的可用宽度，再在 configure 里把
        // 左右 margin 设成 (可用宽 - 面板宽)/2，得到确定的居中。
        layer.set_anchor(Anchor::Bottom | Anchor::Left | Anchor::Right);
        layer.set_size(0, height); // 宽度交给 margin
        layer.set_margin(0, 0, voice::VOICE_MARGIN_BOTTOM, 0);
        // -1 = 不占工作区（纯浮层，不把窗口往上顶）。
        layer.set_exclusive_zone(-1);
        // 键盘交互 none：面板不吃键，右 Ctrl / Esc 仍归 daemon 的 grab。
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        surface.commit(); // 请求 configure

        if let Ok(mut registered) = slot.lock() {
            *registered = Some(surface.clone());
        }
        if let Ok(mut flag) = configured.lock() {
            *flag = false;
        }
        debug!("Voice panel: layer surface created ({}x{})", width, height);
        self.surface = Some(surface);
        self.layer = Some(layer);
        Ok(())
    }

    /// 撤下画面但保留 surface（下次 show 直接重绘）。
    pub fn hide(&mut self) {
        if let Some(surface) = &self.surface {
            surface.attach(None::<&WlBuffer>, 0, 0);
            surface.commit();
        }
    }

    /// 彻底销毁（合成器 closed、收摊）。
    pub fn destroy(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            buffer.destroy();
        }
        if let Some(pool) = self.pool.take() {
            pool.destroy();
        }
        if let Some(layer) = self.layer.take() {
            layer.destroy();
        }
        if let Some(surface) = self.surface.take() {
            surface.destroy();
        }
    }
}

/// memfd 匿名文件（与候选窗同款：tmpfs 写满时 set_len 仍成功、写页才 SIGBUS，
/// 所以用 memfd 而非 /tmp 的 O_TMPFILE）。
fn create_anonymous_file(size: u32) -> Result<OwnedFd, String> {
    let fd =
        nix::sys::memfd::memfd_create(c"xime-voice", nix::sys::memfd::MemFdCreateFlag::MFD_CLOEXEC)
            .map_err(|e| format!("memfd_create: {e}"))?;
    let file = std::fs::File::from(fd);
    file.set_len(size as u64)
        .map_err(|e| format!("set_len: {e}"))?;
    Ok(file.into())
}
