use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// WebDAV 同步配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WebdavConfig {
    pub url: String,
    pub username: String,
    pub password: String,
    #[serde(default = "default_remote_dir")]
    pub remote_dir: String,
}

fn default_remote_dir() -> String {
    "xime".to_string()
}

impl WebdavConfig {
    fn config_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
        let path = PathBuf::from(home).join(".config/xime/webdav.yaml");
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        path
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(config) = serde_yaml::from_str::<WebdavConfig>(&content) {
                    return config;
                }
            }
        }
        WebdavConfig::default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        let content =
            serde_yaml::to_string(self).map_err(|e| format!("序列化 WebDAV 配置失败: {}", e))?;

        // Save with restricted permissions (600) for password security
        fs::write(&path, &content).map_err(|e| format!("写入 WebDAV 配置失败: {}", e))?;

        // Set file permissions to 600 (owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = fs::metadata(&path) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o600);
                fs::set_permissions(&path, perms).ok();
            }
        }

        Ok(())
    }

    pub fn is_valid(&self) -> bool {
        !self.url.is_empty() && !self.username.is_empty()
    }
}

/// Xime 配置目录路径
fn xime_config_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    PathBuf::from(home).join(".config/xime")
}

/// WebDAV 同步操作
pub struct WebdavSync;

impl WebdavSync {
    /// 上传 Rime 配置到 WebDAV 服务器
    /// 将所有文件打包为 tar.gz 后上传
    pub fn upload(config: &WebdavConfig) -> Result<String, String> {
        if !config.is_valid() {
            return Err("请先填写 WebDAV 服务器地址和用户名".to_string());
        }

        let xime_dir = xime_config_dir();
        if !xime_dir.exists() {
            return Err("Xime 配置目录不存在".to_string());
        }

        // 创建临时 tar.gz 文件
        let temp_dir = std::env::temp_dir().join(format!("xime-sync-{}", std::process::id()));
        fs::create_dir_all(&temp_dir).map_err(|e| format!("创建临时目录失败: {}", e))?;
        let tarball_path = temp_dir.join("xime-backup.tar.gz");

        // 使用 tar + gzip 打包
        let result = std::process::Command::new("tar")
            .args([
                "czf",
                &tarball_path.to_string_lossy(),
                "-C",
                &xime_dir
                    .parent()
                    .unwrap_or(Path::new("/"))
                    .to_string_lossy(),
                "xime",
            ])
            .output()
            .map_err(|e| format!("执行 tar 命令失败: {}", e))?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            fs::remove_dir_all(&temp_dir).ok();
            return Err(format!("打包 Rime 配置失败: {}", stderr));
        }

        // 读取打包后的数据
        let data = fs::read(&tarball_path).map_err(|e| format!("读取打包文件失败: {}", e))?;

        // 构建 WebDAV URL
        let url = build_upload_url(config);

        // 上传文件
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

        let response = client
            .put(&url)
            .basic_auth(&config.username, Some(&config.password))
            .header("Content-Type", "application/gzip")
            .body(data)
            .send()
            .map_err(|e| format!("上传失败: {}", e))?;

        // 清理临时文件
        fs::remove_dir_all(&temp_dir).ok();

        if response.status().is_success()
            || response.status().as_u16() == 201
            || response.status().as_u16() == 204
        {
            Ok("上传成功！".to_string())
        } else {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            Err(format!(
                "上传失败 (HTTP {}): {}",
                status,
                body.lines().next().unwrap_or("")
            ))
        }
    }

    /// 从 WebDAV 服务器下载 Rime 配置
    pub fn download(config: &WebdavConfig) -> Result<String, String> {
        if !config.is_valid() {
            return Err("请先填写 WebDAV 服务器地址和用户名".to_string());
        }

        let xime_dir = xime_config_dir();
        fs::create_dir_all(&xime_dir).map_err(|e| format!("创建 Xime 目录失败: {}", e))?;

        // 构建 WebDAV URL
        let url = build_download_url(config);

        // 下载文件
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

        let response = client
            .get(&url)
            .basic_auth(&config.username, Some(&config.password))
            .send()
            .map_err(|e| format!("下载失败: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            if status.as_u16() == 404 {
                return Err("服务器上未找到备份文件".to_string());
            }
            return Err(format!(
                "下载失败 (HTTP {}): 请检查服务器地址和凭据",
                status
            ));
        }

        let data = response
            .bytes()
            .map_err(|e| format!("读取响应数据失败: {}", e))?;

        // 创建临时文件
        let temp_dir = std::env::temp_dir().join(format!("xime-sync-{}", std::process::id()));
        fs::create_dir_all(&temp_dir).map_err(|e| format!("创建临时目录失败: {}", e))?;
        let tarball_path = temp_dir.join("xime-backup.tar.gz");

        fs::write(&tarball_path, &data).map_err(|e| format!("写入临时文件失败: {}", e))?;

        // 先备份当前配置
        let backup_dir = xime_config_dir().with_extension("xime.backup");
        if backup_dir.exists() {
            fs::remove_dir_all(&backup_dir).ok();
        }
        if xime_dir.exists() {
            // 重命名当前目录为备份
            fs::rename(&xime_dir, &backup_dir).map_err(|e| format!("备份当前配置失败: {}", e))?;
        }
        fs::create_dir_all(&xime_dir).map_err(|e| format!("创建 Xime 目录失败: {}", e))?;

        // 解压到 xime 目录
        let result = std::process::Command::new("tar")
            .args([
                "xzf",
                &tarball_path.to_string_lossy(),
                "-C",
                &xime_dir.to_string_lossy(),
                "--strip-components=1",
            ])
            .output()
            .map_err(|e| format!("执行 tar 解压命令失败: {}", e))?;

        // 清理临时文件
        fs::remove_dir_all(&temp_dir).ok();

        if result.status.success() {
            // 删除备份
            fs::remove_dir_all(&backup_dir).ok();
            Ok("下载成功！配置已更新。".to_string())
        } else {
            // 解压失败，恢复备份
            let stderr = String::from_utf8_lossy(&result.stderr);
            if backup_dir.exists() {
                fs::remove_dir_all(&xime_dir).ok();
                fs::rename(&backup_dir, &xime_dir).ok();
            }
            Err(format!("解压配置失败: {}", stderr))
        }
    }

    /// 测试 WebDAV 服务器连接（在后台线程执行）
    pub fn test_connection(config: &WebdavConfig) -> Result<String, String> {
        if !config.is_valid() {
            return Err("请先填写 WebDAV 服务器地址和用户名".to_string());
        }

        let base_url = config.url.trim_end_matches('/').to_string();
        let remote_dir = config.remote_dir.trim_matches('/');
        let url = format!("{}/{}/", base_url, remote_dir);

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

        // 用 GET 请求测试，简单可靠
        let response = client
            .get(&url)
            .basic_auth(&config.username, Some(&config.password))
            .send()
            .map_err(|e| {
                if e.is_timeout() {
                    "连接超时：服务器无响应，请检查地址是否正确".to_string()
                } else if e.is_connect() {
                    format!("无法连接：{}", e)
                } else {
                    format!("请求失败：{}", e)
                }
            })?;

        let status = response.status();

        match status.as_u16() {
            200..=299 => Ok("✅ 连接成功！服务器可达，认证通过。".to_string()),
            401 => Err("❌ 认证失败：用户名或密码错误 (HTTP 401)".to_string()),
            403 => Err("❌ 权限不足：服务器拒绝访问 (HTTP 403)".to_string()),
            404 => Ok("ℹ️ 服务器可达，但远程目录暂不存在（上传时将自动创建）。".to_string()),
            s if s >= 500 => Err(format!("❌ 服务器错误 (HTTP {})", status)),
            _ => Err(format!("❌ 未预期的响应 (HTTP {})", status)),
        }
    }
}

/// 构建上传用的 URL
fn build_upload_url(config: &WebdavConfig) -> String {
    let url = config.url.trim_end_matches('/').to_string();
    let remote_dir = config.remote_dir.trim_matches('/');
    format!("{}/{}/xime-backup.tar.gz", url, remote_dir)
}

/// 构建下载用的 URL
fn build_download_url(config: &WebdavConfig) -> String {
    build_upload_url(config)
}
